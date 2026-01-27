//! Configuration bridge - re-exports shared config + adds core-specific conversions

pub use zelana_config::*;

use crate::sequencer::{BatchConfig, PipelineConfig, ProverMode, SettlerConfig};

/// Convert ProverModeToml (config) to ProverMode (core)
impl From<ProverModeToml> for ProverMode {
    fn from(mode: ProverModeToml) -> Self {
        match mode {
            ProverModeToml::Mock => ProverMode::Mock,
            ProverModeToml::Groth16 => ProverMode::Groth16,
            ProverModeToml::Noir => ProverMode::Noir,
        }
    }
}

/// Extension trait for ZelanaConfig - adds core-specific conversion methods
pub trait ZelanaConfigExt {
    /// Convert to BatchConfig (core-specific type)
    fn to_batch_config(&self) -> BatchConfig;

    /// Convert to PipelineConfig (core-specific type)
    fn to_pipeline_config(&self) -> PipelineConfig;
}

impl ZelanaConfigExt for ZelanaConfig {
    fn to_batch_config(&self) -> BatchConfig {
        BatchConfig {
            max_transactions: self.batch.max_transactions,
            max_batch_age_secs: self.batch.max_batch_age_secs,
            max_shielded: self.batch.max_shielded,
            min_transactions: self.batch.min_transactions,
        }
    }

    fn to_pipeline_config(&self) -> PipelineConfig {
        let batch_config = self.to_batch_config();

        // Build settler config if settlement is enabled
        let settler_config = if self.pipeline.settlement_enabled {
            let mut domain = [0u8; 32];
            let domain_str = self.solana.domain.as_deref().unwrap_or("solana");
            let domain_bytes = domain_str.as_bytes();
            domain[..domain_bytes.len().min(32)]
                .copy_from_slice(&domain_bytes[..domain_bytes.len().min(32)]);

            Some(SettlerConfig {
                rpc_url: self.solana.rpc_url.clone(),
                bridge_program_id: self.solana.bridge_program_id.clone(),
                verifier_program_id: self.solana.verifier_program_id.clone(),
                domain,
                commitment: solana_commitment_config::CommitmentConfig::confirmed(),
                max_retries: self.pipeline.max_settlement_retries,
                retry_delay_ms: self.pipeline.settlement_retry_base_ms,
            })
        } else {
            None
        };

        PipelineConfig {
            prover_mode: self.pipeline.prover_mode.clone().into(),
            proving_key_path: self.pipeline.proving_key_path.clone(),
            verifying_key_path: self.pipeline.verifying_key_path.clone(),
            noir_coordinator_url: self.pipeline.noir_coordinator_url.clone(),
            noir_proof_timeout_secs: self.pipeline.noir_proof_timeout_secs,
            settlement_enabled: self.pipeline.settlement_enabled,
            sequencer_keypair_path: self.pipeline.sequencer_keypair_path.clone(),
            max_settlement_retries: self.pipeline.max_settlement_retries,
            settlement_retry_base_ms: self.pipeline.settlement_retry_base_ms,
            poll_interval_ms: self.pipeline.poll_interval_ms,
            batch_config,
            settler_config,
            dev_mode: self.features.dev_mode,
        }
    }
}
