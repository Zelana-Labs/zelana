#![allow(dead_code)] // Future feature: LP-based fast withdrawals
//! Fast Withdrawals
//!
//! Allows users to withdraw funds immediately by paying a fee to liquidity providers.
//!
//! ```text
//! Standard Withdrawal Flow:
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ User → L2 Withdraw → Wait 7 days (challenge period) → L1 Funds │
//! └─────────────────────────────────────────────────────────────────┘
//!
//! Fast Withdrawal Flow:
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ User → L2 Withdraw → LP fronts L1 funds (immediate) → User     │
//! │                    → LP claims after challenge period           │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! LPs stake capital on L1 and front funds to users for a fee.
//! After the challenge period, LPs claim the original withdrawal amount.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use log::{info, warn};
use tokio::sync::Mutex;

/// Fee configuration for fast withdrawals
#[derive(Debug, Clone)]
pub struct FastWithdrawConfig {
    /// Base fee in basis points (1 bp = 0.01%)
    pub base_fee_bps: u16,
    /// Minimum fee in lamports
    pub min_fee: u64,
    /// Maximum withdrawal amount for fast path
    pub max_amount: u64,
    /// LP collateral requirement (multiplier on max_amount)
    pub collateral_ratio: f64,
}

impl Default for FastWithdrawConfig {
    fn default() -> Self {
        Self {
            base_fee_bps: 50,          // 0.5%
            min_fee: 10_000,           // 10,000 lamports
            max_amount: 1_000_000_000, // 1 SOL equivalent
            collateral_ratio: 2.0,     // 2x collateral
        }
    }
}

/// Liquidity provider registration
#[derive(Debug, Clone)]
pub struct LiquidityProvider {
    /// LP's L1 (Solana) address
    pub l1_address: [u8; 32],
    /// LP's L2 address (for fee collection)
    pub l2_address: [u8; 32],
    /// Staked collateral on L1 (in lamports)
    pub collateral: u64,
    /// Available liquidity (collateral - pending claims)
    pub available: u64,
    /// Custom fee in basis points (overrides base_fee if set)
    pub custom_fee_bps: Option<u16>,
    /// Whether LP is active
    pub active: bool,
    /// Registration timestamp
    pub registered_at: Instant,
}

impl LiquidityProvider {
    /// Calculate available capacity for fast withdrawals
    pub fn capacity(&self) -> u64 {
        self.available
    }

    /// Check if LP can fulfill a withdrawal
    pub fn can_fulfill(&self, amount: u64) -> bool {
        self.active && self.available >= amount
    }
}

/// A pending fast withdrawal claim
#[derive(Debug, Clone)]
pub struct FastWithdrawClaim {
    /// Unique claim ID
    pub claim_id: [u8; 32],
    /// Original withdrawal tx hash
    pub withdrawal_tx_hash: [u8; 32],
    /// LP that fronted the funds
    pub lp_address: [u8; 32],
    /// User's L1 destination address
    pub user_l1_address: [u8; 32],
    /// Amount fronted (net after fee)
    pub amount_fronted: u64,
    /// Fee paid to LP
    pub fee: u64,
    /// Original withdrawal amount
    pub original_amount: u64,
    /// When the claim becomes claimable (after challenge period)
    pub claimable_at: Instant,
    /// State of the claim
    pub state: ClaimState,
}

/// State of a fast withdrawal claim
#[derive(Debug, Clone, PartialEq)]
pub enum ClaimState {
    /// LP has fronted funds, waiting for challenge period
    Pending,
    /// Challenge period passed, LP can claim
    Claimable,
    /// LP has claimed the funds
    Claimed,
    /// Claim was disputed/invalidated
    Invalidated,
}

/// Fast withdrawal service
pub struct FastWithdrawService {
    config: FastWithdrawConfig,
    /// Registered liquidity providers
    lps: HashMap<[u8; 32], LiquidityProvider>,
    /// Pending claims
    claims: HashMap<[u8; 32], FastWithdrawClaim>,
    /// Challenge period duration
    challenge_period: Duration,
}

impl FastWithdrawService {
    /// Create a new fast withdrawal service
    pub fn new(config: FastWithdrawConfig) -> Self {
        Self {
            config,
            lps: HashMap::new(),
            claims: HashMap::new(),
            challenge_period: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
        }
    }

    /// Create with custom challenge period (for testing)
    pub fn with_challenge_period(config: FastWithdrawConfig, challenge_period: Duration) -> Self {
        Self {
            config,
            lps: HashMap::new(),
            claims: HashMap::new(),
            challenge_period,
        }
    }

    /// Register a new liquidity provider
    pub fn register_lp(
        &mut self,
        l1_address: [u8; 32],
        l2_address: [u8; 32],
        collateral: u64,
        custom_fee_bps: Option<u16>,
    ) -> Result<()> {
        if self.lps.contains_key(&l1_address) {
            bail!("LP already registered");
        }

        let required_collateral =
            (self.config.max_amount as f64 * self.config.collateral_ratio) as u64;
        if collateral < required_collateral {
            bail!(
                "Insufficient collateral: {} < {} required",
                collateral,
                required_collateral
            );
        }

        let lp = LiquidityProvider {
            l1_address,
            l2_address,
            collateral,
            available: collateral,
            custom_fee_bps,
            active: true,
            registered_at: Instant::now(),
        };

        self.lps.insert(l1_address, lp);
        info!("LP registered: {}", hex::encode(l1_address));
        Ok(())
    }

    /// Deactivate an LP (they can still claim pending, but no new requests)
    pub fn deactivate_lp(&mut self, l1_address: &[u8; 32]) -> Result<()> {
        let lp = self.lps.get_mut(l1_address).context("LP not found")?;
        lp.active = false;
        info!("LP deactivated: {}", hex::encode(l1_address));
        Ok(())
    }

    /// Get quote for fast withdrawal
    pub fn get_quote(&self, amount: u64) -> Option<FastWithdrawQuote> {
        if amount > self.config.max_amount {
            return None;
        }

        // Find best LP (lowest fee with capacity)
        let best_lp = self
            .lps
            .values()
            .filter(|lp| lp.can_fulfill(amount))
            .min_by_key(|lp| lp.custom_fee_bps.unwrap_or(self.config.base_fee_bps))?;

        let fee_bps = best_lp.custom_fee_bps.unwrap_or(self.config.base_fee_bps);
        let fee = self.calculate_fee(amount, fee_bps);
        let amount_received = amount.saturating_sub(fee);

        Some(FastWithdrawQuote {
            amount,
            fee,
            amount_received,
            fee_bps,
            lp_address: best_lp.l1_address,
            expires_in_secs: 60, // Quote valid for 60 seconds
        })
    }

    /// Execute a fast withdrawal
    pub fn execute_fast_withdraw(
        &mut self,
        withdrawal_tx_hash: [u8; 32],
        user_l1_address: [u8; 32],
        amount: u64,
        lp_address: [u8; 32],
    ) -> Result<FastWithdrawClaim> {
        // Validate amount
        if amount > self.config.max_amount {
            bail!("Amount exceeds maximum for fast withdrawal");
        }

        // First get LP info immutably to calculate fee
        let (fee_bps, can_fulfill) = {
            let lp = self.lps.get(&lp_address).context("LP not found")?;
            (
                lp.custom_fee_bps.unwrap_or(self.config.base_fee_bps),
                lp.can_fulfill(amount),
            )
        };

        if !can_fulfill {
            bail!("LP cannot fulfill this withdrawal");
        }

        // Calculate fee (now we don't hold any borrow)
        let fee = self.calculate_fee(amount, fee_bps);
        let amount_fronted = amount.saturating_sub(fee);

        // Now get mutable borrow to update LP
        let lp = self.lps.get_mut(&lp_address).context("LP not found")?;
        lp.available = lp.available.saturating_sub(amount);

        // Create claim
        let claim_id = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&withdrawal_tx_hash);
            hasher.update(&lp_address);
            hasher.update(&Instant::now().elapsed().as_nanos().to_le_bytes());
            *hasher.finalize().as_bytes()
        };

        let claim = FastWithdrawClaim {
            claim_id,
            withdrawal_tx_hash,
            lp_address,
            user_l1_address,
            amount_fronted,
            fee,
            original_amount: amount,
            claimable_at: Instant::now() + self.challenge_period,
            state: ClaimState::Pending,
        };

        self.claims.insert(claim_id, claim.clone());

        info!(
            "Fast withdrawal executed: {} -> {}, fee: {}",
            hex::encode(withdrawal_tx_hash),
            hex::encode(user_l1_address),
            fee
        );

        Ok(claim)
    }

    /// Process LP claim after challenge period
    pub fn claim(&mut self, claim_id: &[u8; 32]) -> Result<FastWithdrawClaim> {
        let claim = self.claims.get_mut(claim_id).context("Claim not found")?;

        if claim.state != ClaimState::Pending {
            bail!("Claim already processed");
        }

        if Instant::now() < claim.claimable_at {
            let remaining = claim.claimable_at.duration_since(Instant::now());
            bail!(
                "Challenge period not over, {} seconds remaining",
                remaining.as_secs()
            );
        }

        // Update state
        claim.state = ClaimState::Claimable;

        // Restore LP available (they'll get the full original amount from L1 bridge)
        if let Some(lp) = self.lps.get_mut(&claim.lp_address) {
            lp.available = lp.available.saturating_add(claim.original_amount);
        }

        let result = claim.clone();
        claim.state = ClaimState::Claimed;

        info!("LP claimed: {}", hex::encode(claim_id));
        Ok(result)
    }

    /// Invalidate a claim (if withdrawal was invalid/fraudulent)
    pub fn invalidate_claim(&mut self, claim_id: &[u8; 32], reason: &str) -> Result<()> {
        let claim = self.claims.get_mut(claim_id).context("Claim not found")?;

        if claim.state != ClaimState::Pending {
            bail!("Claim already processed");
        }

        claim.state = ClaimState::Invalidated;

        // LP loses their fronted amount (slashed)
        // In a real implementation, this would go to a slashing pool or the challenger

        warn!(
            "Claim invalidated: {}, reason: {}",
            hex::encode(claim_id),
            reason
        );

        Ok(())
    }

    /// Get all pending claims for an LP
    pub fn get_lp_claims(&self, lp_address: &[u8; 32]) -> Vec<&FastWithdrawClaim> {
        self.claims
            .values()
            .filter(|c| &c.lp_address == lp_address && c.state == ClaimState::Pending)
            .collect()
    }

    /// Get claim by ID
    pub fn get_claim(&self, claim_id: &[u8; 32]) -> Option<&FastWithdrawClaim> {
        self.claims.get(claim_id)
    }

    /// Get LP info
    pub fn get_lp(&self, l1_address: &[u8; 32]) -> Option<&LiquidityProvider> {
        self.lps.get(l1_address)
    }

    /// List all active LPs
    pub fn list_active_lps(&self) -> Vec<&LiquidityProvider> {
        self.lps.values().filter(|lp| lp.active).collect()
    }

    /// Calculate fee for a given amount
    fn calculate_fee(&self, amount: u64, fee_bps: u16) -> u64 {
        let fee = (amount as u128 * fee_bps as u128 / 10_000) as u64;
        fee.max(self.config.min_fee)
    }

    /// Update claimable states (called periodically)
    pub fn update_claimable_states(&mut self) {
        let now = Instant::now();
        for claim in self.claims.values_mut() {
            if claim.state == ClaimState::Pending && now >= claim.claimable_at {
                claim.state = ClaimState::Claimable;
            }
        }
    }
}

/// Quote for a fast withdrawal
#[derive(Debug, Clone)]
pub struct FastWithdrawQuote {
    /// Original withdrawal amount
    pub amount: u64,
    /// Fee to LP
    pub fee: u64,
    /// Amount user will receive
    pub amount_received: u64,
    /// Fee in basis points
    pub fee_bps: u16,
    /// LP that will fulfill
    pub lp_address: [u8; 32],
    /// Quote validity in seconds
    pub expires_in_secs: u32,
}

/// Thread-safe wrapper for the fast withdrawal service
pub struct FastWithdrawManager {
    inner: Arc<Mutex<FastWithdrawService>>,
}

impl FastWithdrawManager {
    pub fn new(config: FastWithdrawConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FastWithdrawService::new(config))),
        }
    }

    pub async fn register_lp(
        &self,
        l1_address: [u8; 32],
        l2_address: [u8; 32],
        collateral: u64,
        custom_fee_bps: Option<u16>,
    ) -> Result<()> {
        self.inner
            .lock()
            .await
            .register_lp(l1_address, l2_address, collateral, custom_fee_bps)
    }

    pub async fn get_quote(&self, amount: u64) -> Option<FastWithdrawQuote> {
        self.inner.lock().await.get_quote(amount)
    }

    pub async fn execute_fast_withdraw(
        &self,
        withdrawal_tx_hash: [u8; 32],
        user_l1_address: [u8; 32],
        amount: u64,
        lp_address: [u8; 32],
    ) -> Result<FastWithdrawClaim> {
        self.inner.lock().await.execute_fast_withdraw(
            withdrawal_tx_hash,
            user_l1_address,
            amount,
            lp_address,
        )
    }

    pub async fn claim(&self, claim_id: &[u8; 32]) -> Result<FastWithdrawClaim> {
        self.inner.lock().await.claim(claim_id)
    }

    pub async fn get_claim(&self, claim_id: &[u8; 32]) -> Option<FastWithdrawClaim> {
        self.inner.lock().await.get_claim(claim_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_lp() {
        let mut service = FastWithdrawService::new(FastWithdrawConfig::default());

        let l1_address = [1u8; 32];
        let l2_address = [2u8; 32];
        let collateral = 2_000_000_000; // 2 SOL

        service
            .register_lp(l1_address, l2_address, collateral, None)
            .expect("should register");

        let lp = service.get_lp(&l1_address).expect("LP should exist");
        assert!(lp.active);
        assert_eq!(lp.collateral, collateral);
    }

    #[test]
    fn test_get_quote() {
        let mut service = FastWithdrawService::new(FastWithdrawConfig::default());

        service
            .register_lp([1u8; 32], [2u8; 32], 2_000_000_000, None)
            .unwrap();

        let quote = service.get_quote(100_000_000).expect("should get quote");
        assert!(quote.fee > 0);
        assert_eq!(quote.amount_received, 100_000_000 - quote.fee);
    }

    #[test]
    fn test_execute_fast_withdraw() {
        let mut service = FastWithdrawService::with_challenge_period(
            FastWithdrawConfig::default(),
            Duration::from_secs(1), // Short for testing
        );

        let lp_address = [1u8; 32];
        service
            .register_lp(lp_address, [2u8; 32], 2_000_000_000, None)
            .unwrap();

        let claim = service
            .execute_fast_withdraw([10u8; 32], [20u8; 32], 100_000_000, lp_address)
            .expect("should execute");

        assert_eq!(claim.state, ClaimState::Pending);
        assert!(claim.fee > 0);
    }

    #[test]
    fn test_insufficient_collateral() {
        let mut service = FastWithdrawService::new(FastWithdrawConfig::default());

        let result = service.register_lp([1u8; 32], [2u8; 32], 100, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_fee_calculation() {
        let service = FastWithdrawService::new(FastWithdrawConfig {
            base_fee_bps: 50, // 0.5%
            min_fee: 10_000,
            ..Default::default()
        });

        // 0.5% of 100M = 500K, but min is 10K, so 500K
        let fee = service.calculate_fee(100_000_000, 50);
        assert_eq!(fee, 500_000);

        // Small amount should use min fee
        let fee = service.calculate_fee(1_000, 50);
        assert_eq!(fee, 10_000);
    }
}
