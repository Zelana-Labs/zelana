//! Zelana Configuration
//!
//! Shared configuration crate for all Zelana components.
//!
//! Handles loading configuration from:
//! 1. ZL_CONFIG env var (explicit path)
//! 2. ./config.toml (current directory)
//! 3. ~/.zelana/config.toml (user home)
//!
//! Environment variables take precedence over TOML config.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{env, fs};

/// Global config instance for convenience access
pub static GLOBAL_CONFIG: OnceLock<ZelanaConfig> = OnceLock::new();

const CONFIG_FILE_NAME: &str = "config.toml";
const CONFIG_DIR_NAME: &str = ".zelana";

// ============================================================================
// Default Constants (avoid repeated allocations)
// ============================================================================

const DEFAULT_SEQUENCER: &str = "127.0.0.1:8080";
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_DB_PATH: &str = "./zelana-db";
const DEFAULT_WS_URL: &str = "ws://127.0.0.1:8900/";
const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8899";
const DEFAULT_BRIDGE_PROGRAM: &str = "9HXapBN9otLGnQNGv1HRk91DGqMNvMAvQqohL7gPW1sd";
const DEFAULT_VERIFIER_PROGRAM: &str = "7rsVijhQ1ipfc6uxzcs4R2gBtD9L5ZLubSc6vPKXgawo";

const DEFAULT_MAX_RETRIES: u32 = 5;
const DEFAULT_RETRY_BASE_MS: u64 = 5000;
const DEFAULT_POLL_INTERVAL_MS: u64 = 100;
const DEFAULT_MAX_TRANSACTIONS: usize = 100;
const DEFAULT_MAX_BATCH_AGE_SECS: u64 = 60;
const DEFAULT_MAX_SHIELDED: usize = 10;
const DEFAULT_MIN_TRANSACTIONS: usize = 1;
const DEFAULT_THRESHOLD_K: usize = 2;
const DEFAULT_THRESHOLD_N: usize = 3;

// ============================================================================
// Config Structs
// ============================================================================

/// Root configuration structure (matches TOML layout)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZelanaConfig {
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub pipeline: PipelineTomlConfig,
    #[serde(default)]
    pub batch: BatchTomlConfig,
    #[serde(default)]
    pub solana: SolanaConfig,
    #[serde(default)]
    pub features: FeatureFlags,
}

/// API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_sequencer")]
    pub sequencer: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub udp_port: Option<u16>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            sequencer: DEFAULT_SEQUENCER.into(),
            port : DEFAULT_PORT.into(),
            udp_port: None,
        }
    }
}

fn default_sequencer() -> String {
    DEFAULT_SEQUENCER.into()
}

fn default_port() -> u16 {
    DEFAULT_PORT.into()
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: DEFAULT_DB_PATH.into(),
        }
    }
}

fn default_db_path() -> String {
    DEFAULT_DB_PATH.into()
}

/// Pipeline configuration (TOML format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTomlConfig {
    #[serde(default)]
    pub prover_mode: ProverModeToml,
    #[serde(default)]
    pub settlement_enabled: bool,
    #[serde(default)]
    pub proving_key_path: Option<String>,
    #[serde(default)]
    pub verifying_key_path: Option<String>,
    #[serde(default)]
    pub noir_coordinator_url: Option<String>,
    #[serde(default)]
    pub noir_proof_timeout_secs: Option<u64>,
    #[serde(default)]
    pub sequencer_keypair_path: Option<String>,
    #[serde(default = "default_max_retries")]
    pub max_settlement_retries: u32,
    #[serde(default = "default_retry_base_ms")]
    pub settlement_retry_base_ms: u64,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
}

/// Prover mode for TOML config
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProverModeToml {
    #[default]
    Mock,
    Groth16,
    Noir,
}

impl Default for PipelineTomlConfig {
    fn default() -> Self {
        Self {
            prover_mode: ProverModeToml::Mock,
            settlement_enabled: false,
            proving_key_path: None,
            verifying_key_path: None,
            noir_coordinator_url: None,
            noir_proof_timeout_secs: None,
            sequencer_keypair_path: None,
            max_settlement_retries: DEFAULT_MAX_RETRIES,
            settlement_retry_base_ms: DEFAULT_RETRY_BASE_MS,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        }
    }
}

fn default_max_retries() -> u32 {
    DEFAULT_MAX_RETRIES
}
fn default_retry_base_ms() -> u64 {
    DEFAULT_RETRY_BASE_MS
}
fn default_poll_interval() -> u64 {
    DEFAULT_POLL_INTERVAL_MS
}

/// Batch configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTomlConfig {
    #[serde(default = "default_max_transactions")]
    pub max_transactions: usize,
    #[serde(default = "default_max_batch_age")]
    pub max_batch_age_secs: u64,
    #[serde(default = "default_max_shielded")]
    pub max_shielded: usize,
    #[serde(default = "default_min_transactions")]
    pub min_transactions: usize,
}

impl Default for BatchTomlConfig {
    fn default() -> Self {
        Self {
            max_transactions: DEFAULT_MAX_TRANSACTIONS,
            max_batch_age_secs: DEFAULT_MAX_BATCH_AGE_SECS,
            max_shielded: DEFAULT_MAX_SHIELDED,
            min_transactions: DEFAULT_MIN_TRANSACTIONS,
        }
    }
}

fn default_max_transactions() -> usize {
    DEFAULT_MAX_TRANSACTIONS
}
fn default_max_batch_age() -> u64 {
    DEFAULT_MAX_BATCH_AGE_SECS
}
fn default_max_shielded() -> usize {
    DEFAULT_MAX_SHIELDED
}
fn default_min_transactions() -> usize {
    DEFAULT_MIN_TRANSACTIONS
}

/// Solana connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaConfig {
    #[serde(default = "default_ws_url")]
    pub ws_url: String,
    #[serde(default = "default_rpc_url")]
    pub rpc_url: String,
    #[serde(default = "default_bridge_program")]
    pub bridge_program_id: String,
    #[serde(default = "default_verifier_program")]
    pub verifier_program_id: String,
    #[serde(default)]
    pub domain: Option<String>,
}

impl Default for SolanaConfig {
    fn default() -> Self {
        Self {
            ws_url: DEFAULT_WS_URL.into(),
            rpc_url: DEFAULT_RPC_URL.into(),
            bridge_program_id: DEFAULT_BRIDGE_PROGRAM.into(),
            verifier_program_id: DEFAULT_VERIFIER_PROGRAM.into(),
            domain: None,
        }
    }
}

fn default_ws_url() -> String {
    DEFAULT_WS_URL.into()
}
fn default_rpc_url() -> String {
    DEFAULT_RPC_URL.into()
}
fn default_bridge_program() -> String {
    DEFAULT_BRIDGE_PROGRAM.into()
}
fn default_verifier_program() -> String {
    DEFAULT_VERIFIER_PROGRAM.into()
}

/// Feature flags
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureFlags {
    #[serde(default)]
    pub dev_mode: bool,
    #[serde(default)]
    pub fast_withdrawals: bool,
    #[serde(default)]
    pub threshold_encryption: bool,
    #[serde(default = "default_threshold_k")]
    pub threshold_k: usize,
    #[serde(default = "default_threshold_n")]
    pub threshold_n: usize,
    #[serde(default)]
    pub threshold_dev: bool,
}

fn default_threshold_k() -> usize {
    DEFAULT_THRESHOLD_K
}
fn default_threshold_n() -> usize {
    DEFAULT_THRESHOLD_N
}

// ============================================================================
// Environment Variable Helpers
// ============================================================================

/// Set field from env var if present
fn env_string(key: &str, field: &mut String) {
    if let Ok(v) = env::var(key) {
        *field = v;
    }
}

/// Set Option<String> from env var if present
fn env_option_string(key: &str, field: &mut Option<String>) {
    if let Ok(v) = env::var(key) {
        *field = Some(v);
    }
}

/// Set field from env var if present and parseable
fn env_parse<T: std::str::FromStr>(key: &str, field: &mut T) {
    if let Ok(v) = env::var(key) {
        if let Ok(parsed) = v.parse() {
            *field = parsed;
        }
    }
}

/// Set Option<T> from env var if present and parseable
fn env_parse_option<T: std::str::FromStr>(key: &str, field: &mut Option<T>) {
    if let Ok(v) = env::var(key) {
        if let Ok(parsed) = v.parse() {
            *field = Some(parsed);
        }
    }
}

/// Check if env var is set to a truthy value ("1" or "true")
fn env_bool(key: &str) -> Option<bool> {
    env::var(key)
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Check if env var exists (presence = true)
fn env_exists(key: &str) -> bool {
    env::var(key).is_ok()
}

// ============================================================================
// Implementation
// ============================================================================

impl ZelanaConfig {
    /// Load configuration from config file with env var overrides
    pub fn load() -> Result<Self> {
        let mut config = match Self::find_config_file() {
            Some(path) => {
                log::info!("Loading config from: {}", path.display());
                let contents = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read config file: {}", path.display()))?;
                toml::from_str(&contents)
                    .with_context(|| format!("Failed to parse config file: {}", path.display()))?
            }
            None => {
                log::info!("No config file found, using defaults and environment variables");
                Self::default()
            }
        };

        config.apply_env_overrides();
        Ok(config)
    }

    /// Load configuration from a specific file path
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let mut config: Self = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        config.apply_env_overrides();
        Ok(config)
    }

    /// Find the config file path
    fn find_config_file() -> Option<PathBuf> {
        // 1. Check ZL_CONFIG env var
        if let Ok(path) = env::var("ZL_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Some(path);
            }
        }

        // 2. Check ./config.toml (current directory)
        let local_path = PathBuf::from(CONFIG_FILE_NAME);
        if local_path.exists() {
            return Some(local_path);
        }

        // 3. Check ~/.zelana/config.toml
        dirs::home_dir()
            .map(|h| h.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
            .filter(|p| p.exists())
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Database
        env_string("ZL_DB_PATH", &mut self.database.path);

        // API
        env_string("ZL_API_HOST", &mut self.api.sequencer);
        env_parse_option("ZL_UDP_PORT", &mut self.api.udp_port);

        // Solana
        env_string("SOLANA_WS_URL", &mut self.solana.ws_url);
        env_string("SOLANA_RPC_URL", &mut self.solana.rpc_url);
        env_string("ZL_BRIDGE_PROGRAM", &mut self.solana.bridge_program_id);
        env_string(
            "ZL_VERIFIER_PROGRAM_ID",
            &mut self.solana.verifier_program_id,
        );
        env_option_string("ZL_DOMAIN", &mut self.solana.domain);

        // Pipeline - prover mode
        if let Ok(v) = env::var("ZL_PROVER_MODE") {
            self.pipeline.prover_mode = match v.to_ascii_lowercase().as_str() {
                "groth16" => ProverModeToml::Groth16,
                "noir" => ProverModeToml::Noir,
                _ => ProverModeToml::Mock,
            };
        }

        // Legacy: ZL_MOCK_PROVER
        if let Some(enabled) = env_bool("ZL_MOCK_PROVER") {
            self.pipeline.prover_mode = if enabled {
                ProverModeToml::Mock
            } else {
                ProverModeToml::Groth16
            };
        }

        if let Some(v) = env_bool("ZL_SETTLEMENT_ENABLED") {
            self.pipeline.settlement_enabled = v;
        }

        env_option_string("ZL_PROVING_KEY", &mut self.pipeline.proving_key_path);
        env_option_string("ZL_VERIFYING_KEY", &mut self.pipeline.verifying_key_path);
        env_option_string(
            "ZL_NOIR_COORDINATOR_URL",
            &mut self.pipeline.noir_coordinator_url,
        );
        env_parse_option(
            "ZL_NOIR_PROOF_TIMEOUT_SECS",
            &mut self.pipeline.noir_proof_timeout_secs,
        );
        env_option_string(
            "ZL_SEQUENCER_KEYPAIR",
            &mut self.pipeline.sequencer_keypair_path,
        );
        env_parse(
            "ZL_SETTLEMENT_RETRIES",
            &mut self.pipeline.max_settlement_retries,
        );

        // Batch
        env_parse("BATCH_MAX_TXS", &mut self.batch.max_transactions);
        env_parse("BATCH_MAX_AGE", &mut self.batch.max_batch_age_secs);
        env_parse("BATCH_MAX_SHIELDED", &mut self.batch.max_shielded);

        // Features
        if let Some(v) = env_bool("DEV_MODE") {
            self.features.dev_mode = v;
        }
        if env_exists("FAST_WITHDRAW_ENABLED") {
            self.features.fast_withdrawals = true;
        }
        if env_exists("THRESHOLD_ENABLED") {
            self.features.threshold_encryption = true;
        }
        env_parse("THRESHOLD_K", &mut self.features.threshold_k);
        env_parse("THRESHOLD_N", &mut self.features.threshold_n);
        if env_exists("THRESHOLD_DEV") {
            self.features.threshold_dev = true;
        }
    }

    /// Get the default config file path
    pub fn default_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
    }

    /// Generate a sample config file
    pub fn generate_sample() -> String {
        let mut sample = Self::default();
        sample.features.dev_mode = true;
        sample.solana.domain = Some("solana".into());
        toml::to_string_pretty(&sample).unwrap_or_default()
    }

    /// Get the global config instance, initializing it if necessary.
    ///
    /// This is the recommended way to access config in most code.
    /// Falls back to defaults if loading fails.
    pub fn global() -> &'static ZelanaConfig {
        GLOBAL_CONFIG.get_or_init(|| {
            Self::load().unwrap_or_else(|e| {
                log::warn!("Failed to load config: {}, using defaults", e);
                Self::default()
            })
        })
    }

    /// Try to get the global config instance.
    ///
    /// Returns `None` if config hasn't been initialized yet.
    pub fn try_global() -> Option<&'static ZelanaConfig> {
        GLOBAL_CONFIG.get()
    }

    /// Initialize the global config with a specific instance.
    ///
    /// Returns `Err(config)` if already initialized.
    pub fn set_global(config: ZelanaConfig) -> Result<(), ZelanaConfig> {
        GLOBAL_CONFIG.set(config)
    }
}

/// Shorthand for `ZelanaConfig::global()`.
#[inline]
pub fn global_config() -> &'static ZelanaConfig {
    ZelanaConfig::global()
}

// ============================================================================
// Parsed Config (lazy-initialized constants)
// ============================================================================

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::LazyLock;

/// Pre-parsed Solana configuration - access fields directly like a constant.
///
/// # Example
/// ```ignore
/// use zelana_config::SOLANA;
///
/// let program = SOLANA.bridge_program;  // Pubkey - no function call!
/// let rpc = SOLANA.rpc_url;             // &'static str
/// ```
pub static SOLANA: LazyLock<SolanaRuntime> = LazyLock::new(|| {
    let cfg = ZelanaConfig::global();
    SolanaRuntime {
        bridge_program: Pubkey::from_str(&cfg.solana.bridge_program_id)
            .expect("Invalid bridge_program_id in config"),
        verifier_program: Pubkey::from_str(&cfg.solana.verifier_program_id)
            .expect("Invalid verifier_program_id in config"),
        rpc_url: &cfg.solana.rpc_url,
        ws_url: &cfg.solana.ws_url,
        domain: cfg.solana.domain.as_deref(),
    }
});

/// Pre-parsed Solana configuration with `Pubkey` fields.
pub struct SolanaRuntime {
    /// Bridge program ID (pre-parsed)
    pub bridge_program: Pubkey,
    /// Verifier program ID (pre-parsed)
    pub verifier_program: Pubkey,
    /// Solana RPC URL
    pub rpc_url: &'static str,
    /// Solana WebSocket URL
    pub ws_url: &'static str,
    /// Domain identifier (if set)
    pub domain: Option<&'static str>,
}

/// Pre-parsed API configuration - access fields directly like a constant.
///
/// # Example
/// ```ignore
/// use zelana_config::API;
///
/// let seq = API.sequencer_url;  // &'static str - no function call!
/// ```
pub static API: LazyLock<ApiRuntime> = LazyLock::new(|| {
    let cfg = ZelanaConfig::global();
    ApiRuntime {
        sequencer_url: &cfg.api.sequencer,
        port: &cfg.api.port,
        udp_port: cfg.api.udp_port,
    }
});

/// Pre-parsed API configuration.
pub struct ApiRuntime<'a> {
    /// Sequencer HTTP API URL
    pub sequencer_url: &'static str,
    pub port: &'a u16,
    /// UDP port (if configured)
    pub udp_port: Option<u16>,
}

/// Database configuration constant.
pub static DATABASE: LazyLock<DatabaseRuntime> = LazyLock::new(|| {
    let cfg = ZelanaConfig::global();
    DatabaseRuntime {
        path: &cfg.database.path,
    }
});

pub struct DatabaseRuntime {
    pub path: &'static str,
}

/// Pipeline configuration constant.
pub static PIPELINE: LazyLock<PipelineRuntime> = LazyLock::new(|| {
    let cfg = ZelanaConfig::global();
    PipelineRuntime {
        prover_mode: cfg.pipeline.prover_mode.clone(),
        settlement_enabled: cfg.pipeline.settlement_enabled,
        proving_key_path: cfg.pipeline.proving_key_path.as_deref(),
        verifying_key_path: cfg.pipeline.verifying_key_path.as_deref(),
        noir_coordinator_url: cfg.pipeline.noir_coordinator_url.as_deref(),
        noir_proof_timeout_secs: cfg.pipeline.noir_proof_timeout_secs,
        sequencer_keypair_path: cfg.pipeline.sequencer_keypair_path.as_deref(),
        max_settlement_retries: cfg.pipeline.max_settlement_retries,
        settlement_retry_base_ms: cfg.pipeline.settlement_retry_base_ms,
        poll_interval_ms: cfg.pipeline.poll_interval_ms,
    }
});

pub struct PipelineRuntime {
    pub prover_mode: ProverModeToml,
    pub settlement_enabled: bool,
    pub proving_key_path: Option<&'static str>,
    pub verifying_key_path: Option<&'static str>,
    pub noir_coordinator_url: Option<&'static str>,
    pub noir_proof_timeout_secs: Option<u64>,
    pub sequencer_keypair_path: Option<&'static str>,
    pub max_settlement_retries: u32,
    pub settlement_retry_base_ms: u64,
    pub poll_interval_ms: u64,
}

/// Batch configuration constant.
pub static BATCH: LazyLock<BatchRuntime> = LazyLock::new(|| {
    let cfg = ZelanaConfig::global();
    BatchRuntime {
        max_transactions: cfg.batch.max_transactions,
        max_batch_age_secs: cfg.batch.max_batch_age_secs,
        max_shielded: cfg.batch.max_shielded,
        min_transactions: cfg.batch.min_transactions,
    }
});

pub struct BatchRuntime {
    pub max_transactions: usize,
    pub max_batch_age_secs: u64,
    pub max_shielded: usize,
    pub min_transactions: usize,
}

/// Feature flags constant.
pub static FEATURES: LazyLock<FeaturesRuntime> = LazyLock::new(|| {
    let cfg = ZelanaConfig::global();
    FeaturesRuntime {
        dev_mode: cfg.features.dev_mode,
        fast_withdrawals: cfg.features.fast_withdrawals,
        threshold_encryption: cfg.features.threshold_encryption,
        threshold_k: cfg.features.threshold_k,
        threshold_n: cfg.features.threshold_n,
        threshold_dev: cfg.features.threshold_dev,
    }
});

pub struct FeaturesRuntime {
    pub dev_mode: bool,
    pub fast_withdrawals: bool,
    pub threshold_encryption: bool,
    pub threshold_k: usize,
    pub threshold_n: usize,
    pub threshold_dev: bool,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ZelanaConfig::default();
        assert_eq!(config.api.sequencer, DEFAULT_SEQUENCER);
        assert_eq!(config.database.path, DEFAULT_DB_PATH);
        assert_eq!(config.pipeline.prover_mode, ProverModeToml::Mock);
        assert!(!config.pipeline.settlement_enabled);
    }

    #[test]
    fn test_generate_sample() {
        let sample = ZelanaConfig::generate_sample();
        assert!(sample.contains("[api]"));
        assert!(sample.contains("[database]"));
        assert!(sample.contains("[pipeline]"));
        assert!(sample.contains("[features]"));
    }

    #[test]
    fn test_parse_sample() {
        let sample = ZelanaConfig::generate_sample();
        let parsed: ZelanaConfig = toml::from_str(&sample).unwrap();
        assert_eq!(parsed.api.sequencer, DEFAULT_SEQUENCER);
        assert!(parsed.features.dev_mode);
    }

    #[test]
    fn test_constants_match_defaults() {
        let config = ZelanaConfig::default();
        assert_eq!(config.solana.rpc_url, DEFAULT_RPC_URL);
        assert_eq!(config.solana.ws_url, DEFAULT_WS_URL);
        assert_eq!(config.batch.max_transactions, DEFAULT_MAX_TRANSACTIONS);
    }
}
