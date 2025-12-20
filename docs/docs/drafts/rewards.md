---
sidebar_position: 4
---
# Rewards

## Core Token: zeSOL (zelanaSOL)
- A **hybrid Liquid Staking Token (LST)** launched via Sanctum white-label.
- **On L1 (Solana Mainnet)**: Standard LST—users deposit SOL → mint zeSOL →underlying SOL staked to Zelana Labs validator(s) → earns native Solana staking yields (~7-12% APY, including MEV via Jito, look into BAM integration).
- **On L2 (Zelana Rollup)**: Bridged zelanaSOL becomes a **shielded privacy coin** (encrypted
balances/transfers)—enabling anonymous private txs without leaks. No new emissions or governance token—pure real yields, no dilution.

## Validator Structure & Rewards
- **Two Layers of Validators** (both rewarded in zelanaSOL from a unified pool):
- **L1 Validators**: Zelana Labs nodes (potentially delegated later) stake deposited/bridged SOL → capture base yields/commissions/MEV.
- **L2 Custom Validators**: Sovereign operator network running ZK-rollup (and maybe prover layerr) for shielded batches and encrypted state transitions.
- **Reward Sources** (One pooled treasury):
	- L1 staking rewards from all deposited/bridged SOL.
	- L2 micro-fees from private transactions (subsidized for UX, but protocol skims for the pool).
- **Distribution**: Pro-rata to bonded stake + performance bonuses (uptime/proof speed). L2
claims via anonymous ZK-proofs. Slashing on faults redistributes to honest operators.
- **Effective Yields**: 8-15%+ APY in zelanaSOL (stacks passively on L1, scales with
TVL/fees)—competitive double-dip without restaking dependencies.

## Key Design Choices
- **Sovereign Custom Approach**: Full on-rollup rewards program (Anchor-based
escrow/slashing)—no reliance on AVS (e.g., Solayer/Picasso) to preserve privacy purity and
control.
- **Privacy Focus**: End-to-end shielded (bonds, claims, txs) —audit keys for compliance.
- **Bootstrap Plan**: Early treasury seeds, quests; partner with Arcium (for tech/traction) and
Sanctum (LST launch/liquidity). Grants/partnerships to fund development.

**In essence**: Zelana is the "private JitoSOL"—liquid yields on L1, fully shielded utility on L2,
rewarding both layers' validators with real SOL-derived income in a tokenless, high-privacy
flywheel.