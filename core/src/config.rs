#![allow(dead_code)] // Config generation/loading utilities
//! Configuration Module
//!
//! Handles loading configuration from:
//! 1. ~/.zelana/config.toml (if exists)
//! 2. Environment variables (override TOML values)
//!
//! Environment variables take precedence over TOML config.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{env, fs};

use crate::sequencer::{BatchConfig, PipelineConfig, ProverMode, SettlerConfig};

const CONFIG_FILE_NAME: &str = "config.toml";
const CONFIG_DIR_NAME: &str = ".zelana";

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
    #[serde(default = "default_api_host")]
    pub host: String,
    #[serde(default = "default_api_port")]
    pub port: u16,
    #[serde(default)]
    pub udp_port: Option<u16>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: default_api_host(),
            port: default_api_port(),
            udp_port: None,
        }
    }
}

fn default_api_host() -> String {
    "127.0.0.1".to_string()
}

fn default_api_port() -> u16 {
    8080
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
            path: default_db_path(),
        }
    }
}

fn default_db_path() -> String {
    "./zelana-db".to_string()
}

/// Pipeline configuration (TOML format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTomlConfig {
    /// Prover mode: "mock", "groth16", or "noir" (default: "mock")
    #[serde(default)]
    pub prover_mode: ProverModeToml,
    #[serde(default)]
    pub settlement_enabled: bool,
    #[serde(default)]
    pub proving_key_path: Option<String>,
    #[serde(default)]
    pub verifying_key_path: Option<String>,
    /// Noir coordinator URL (for prover_mode = "noir")
    #[serde(default)]
    pub noir_coordinator_url: Option<String>,
    /// Noir proof timeout in seconds (default: 300)
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

/// Prover mode for TOML config (string-based for easier config)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProverModeToml {
    #[default]
    Mock,
    Groth16,
    Noir,
}

impl From<ProverModeToml> for ProverMode {
    fn from(mode: ProverModeToml) -> Self {
        match mode {
            ProverModeToml::Mock => ProverMode::Mock,
            ProverModeToml::Groth16 => ProverMode::Groth16,
            ProverModeToml::Noir => ProverMode::Noir,
        }
    }
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
            max_settlement_retries: default_max_retries(),
            settlement_retry_base_ms: default_retry_base_ms(),
            poll_interval_ms: default_poll_interval(),
        }
    }
}

fn default_max_retries() -> u32 {
    5
}

fn default_retry_base_ms() -> u64 {
    5000
}

fn default_poll_interval() -> u64 {
    100
}

/// Batch configuration (TOML format)
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
            max_transactions: default_max_transactions(),
            max_batch_age_secs: default_max_batch_age(),
            max_shielded: default_max_shielded(),
            min_transactions: default_min_transactions(),
        }
    }
}

fn default_max_transactions() -> usize {
    100
}

fn default_max_batch_age() -> u64 {
    60
}

fn default_max_shielded() -> usize {
    10
}

fn default_min_transactions() -> usize {
    1
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
    #[serde(default)]
    pub verifier_program_id: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
}

impl Default for SolanaConfig {
    fn default() -> Self {
        Self {
            ws_url: default_ws_url(),
            rpc_url: default_rpc_url(),
            bridge_program_id: default_bridge_program(),
            verifier_program_id: None,
            domain: None,
        }
    }
}

fn default_ws_url() -> String {
    "ws://127.0.0.1:8900/".to_string()
}

fn default_rpc_url() -> String {
    "http://127.0.0.1:8899".to_string()
}

fn default_bridge_program() -> String {
    "8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE".to_string()
}

/// Feature flags
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureFlags {
    /// Enable dev mode (testing endpoints)
    #[serde(default)]
    pub dev_mode: bool,
    /// Enable fast withdrawals
    #[serde(default)]
    pub fast_withdrawals: bool,
    /// Enable threshold encryption
    #[serde(default)]
    pub threshold_encryption: bool,
    /// Threshold K (min shares needed)
    #[serde(default = "default_threshold_k")]
    pub threshold_k: usize,
    /// Threshold N (total shares)
    #[serde(default = "default_threshold_n")]
    pub threshold_n: usize,
    /// Enable dev mode for threshold (creates test committee)
    #[serde(default)]
    pub threshold_dev: bool,
}

fn default_threshold_k() -> usize {
    2
}

fn default_threshold_n() -> usize {
    3
}

impl ZelanaConfig {
    /// Load configuration from ~/.zelana/config.toml with env var overrides
    pub fn load() -> Result<Self> {
        // Start with defaults
        let mut config = Self::default();

        // Try to load from config file
        if let Some(config_path) = Self::find_config_file() {
            log::info!("Loading config from: {}", config_path.display());
            let contents = fs::read_to_string(&config_path).with_context(|| {
                format!("Failed to read config file: {}", config_path.display())
            })?;
            config = toml::from_str(&contents).with_context(|| {
                format!("Failed to parse config file: {}", config_path.display())
            })?;
        } else {
            log::info!("No config file found, using defaults and environment variables");
        }

        // Apply environment variable overrides
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

        // 2. Check ~/.zelana/config.toml
        if let Some(home_dir) = dirs::home_dir() {
            let config_path = home_dir.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME);
            if config_path.exists() {
                return Some(config_path);
            }
        }

        // 3. Check ./config.toml (current directory)
        let local_path = PathBuf::from(CONFIG_FILE_NAME);
        if local_path.exists() {
            return Some(local_path);
        }

        None
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Database
        if let Ok(v) = env::var("ZL_DB_PATH") {
            self.database.path = v;
        }

        // API
        if let Ok(v) = env::var("ZL_API_HOST") {
            self.api.host = v;
        }
        if let Ok(v) = env::var("ZL_API_PORT") {
            if let Ok(port) = v.parse() {
                self.api.port = port;
            }
        }
        if let Ok(v) = env::var("ZL_UDP_PORT") {
            if let Ok(port) = v.parse() {
                self.api.udp_port = Some(port);
            }
        }

        // Solana
        if let Ok(v) = env::var("SOLANA_WS_URL") {
            self.solana.ws_url = v;
        }
        if let Ok(v) = env::var("SOLANA_RPC_URL") {
            self.solana.rpc_url = v;
        }
        if let Ok(v) = env::var("ZL_BRIDGE_PROGRAM") {
            self.solana.bridge_program_id = v;
        }
        if let Ok(v) = env::var("ZL_VERIFIER_PROGRAM_ID") {
            self.solana.verifier_program_id = Some(v);
        }
        if let Ok(v) = env::var("ZL_DOMAIN") {
            self.solana.domain = Some(v);
        }

        // Pipeline - prover mode
        if let Ok(v) = env::var("ZL_PROVER_MODE") {
            match v.to_lowercase().as_str() {
                "mock" => self.pipeline.prover_mode = ProverModeToml::Mock,
                "groth16" => self.pipeline.prover_mode = ProverModeToml::Groth16,
                "noir" => self.pipeline.prover_mode = ProverModeToml::Noir,
                _ => {} // ignore invalid values
            }
        }
        // Legacy: ZL_MOCK_PROVER still supported for backwards compatibility
        if let Ok(v) = env::var("ZL_MOCK_PROVER") {
            if v != "0" && v.to_lowercase() != "false" {
                self.pipeline.prover_mode = ProverModeToml::Mock;
            } else {
                self.pipeline.prover_mode = ProverModeToml::Groth16;
            }
        }
        if let Ok(v) = env::var("ZL_SETTLEMENT_ENABLED") {
            self.pipeline.settlement_enabled = v == "1" || v.to_lowercase() == "true";
        }
        if let Ok(v) = env::var("ZL_PROVING_KEY") {
            self.pipeline.proving_key_path = Some(v);
        }
        if let Ok(v) = env::var("ZL_VERIFYING_KEY") {
            self.pipeline.verifying_key_path = Some(v);
        }
        if let Ok(v) = env::var("ZL_NOIR_COORDINATOR_URL") {
            self.pipeline.noir_coordinator_url = Some(v);
        }
        if let Ok(v) = env::var("ZL_NOIR_PROOF_TIMEOUT_SECS") {
            if let Ok(n) = v.parse() {
                self.pipeline.noir_proof_timeout_secs = Some(n);
            }
        }
        if let Ok(v) = env::var("ZL_SEQUENCER_KEYPAIR") {
            self.pipeline.sequencer_keypair_path = Some(v);
        }
        if let Ok(v) = env::var("ZL_SETTLEMENT_RETRIES") {
            if let Ok(n) = v.parse() {
                self.pipeline.max_settlement_retries = n;
            }
        }

        // Batch
        if let Ok(v) = env::var("BATCH_MAX_TXS") {
            if let Ok(n) = v.parse() {
                self.batch.max_transactions = n;
            }
        }
        if let Ok(v) = env::var("BATCH_MAX_AGE") {
            if let Ok(n) = v.parse() {
                self.batch.max_batch_age_secs = n;
            }
        }
        if let Ok(v) = env::var("BATCH_MAX_SHIELDED") {
            if let Ok(n) = v.parse() {
                self.batch.max_shielded = n;
            }
        }

        // Features
        if let Ok(v) = env::var("DEV_MODE") {
            self.features.dev_mode = v == "1" || v.to_lowercase() == "true";
        }
        if env::var("FAST_WITHDRAW_ENABLED").is_ok() {
            self.features.fast_withdrawals = true;
        }
        if env::var("THRESHOLD_ENABLED").is_ok() {
            self.features.threshold_encryption = true;
        }
        if let Ok(v) = env::var("THRESHOLD_K") {
            if let Ok(n) = v.parse() {
                self.features.threshold_k = n;
            }
        }
        if let Ok(v) = env::var("THRESHOLD_N") {
            if let Ok(n) = v.parse() {
                self.features.threshold_n = n;
            }
        }
        if env::var("THRESHOLD_DEV").is_ok() {
            self.features.threshold_dev = true;
        }
    }

    /// Convert to BatchConfig
    pub fn to_batch_config(&self) -> BatchConfig {
        BatchConfig {
            max_transactions: self.batch.max_transactions,
            max_batch_age_secs: self.batch.max_batch_age_secs,
            max_shielded: self.batch.max_shielded,
            min_transactions: self.batch.min_transactions,
        }
    }

    /// Convert to PipelineConfig
    pub fn to_pipeline_config(&self) -> PipelineConfig {
        let batch_config = self.to_batch_config();

        // Build settler config if settlement is enabled
        let settler_config =
            if self.pipeline.settlement_enabled {
                let mut domain = [0u8; 32];
                let domain_str = self.solana.domain.as_deref().unwrap_or("solana");
                let domain_bytes = domain_str.as_bytes();
                domain[..domain_bytes.len().min(32)]
                    .copy_from_slice(&domain_bytes[..domain_bytes.len().min(32)]);

                Some(SettlerConfig {
                    rpc_url: self.solana.rpc_url.clone(),
                    bridge_program_id: self.solana.bridge_program_id.clone(),
                    verifier_program_id: self.solana.verifier_program_id.clone().unwrap_or_else(
                        || "8TveT3mvH59qLzZNwrTT6hBqDHEobW2XnCPb7xZLBYHd".to_string(),
                    ),
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

    /// Get the default config file path
    pub fn default_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
    }

    /// Generate a sample config file
    pub fn generate_sample() -> String {
        let sample = Self {
            api: ApiConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                udp_port: None,
            },
            database: DatabaseConfig {
                path: "./zelana-db".to_string(),
            },
            pipeline: PipelineTomlConfig {
                prover_mode: ProverModeToml::Mock,
                settlement_enabled: false,
                proving_key_path: None,
                verifying_key_path: None,
                noir_coordinator_url: None,
                noir_proof_timeout_secs: None,
                sequencer_keypair_path: None,
                max_settlement_retries: 5,
                settlement_retry_base_ms: 5000,
                poll_interval_ms: 100,
            },
            batch: BatchTomlConfig {
                max_transactions: 100,
                max_batch_age_secs: 60,
                max_shielded: 10,
                min_transactions: 1,
            },
            solana: SolanaConfig {
                ws_url: "ws://127.0.0.1:8900/".to_string(),
                rpc_url: "http://127.0.0.1:8899".to_string(),
                bridge_program_id: "8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE".to_string(),
                verifier_program_id: None,
                domain: Some("solana".to_string()),
            },
            features: FeatureFlags {
                dev_mode: true,
                fast_withdrawals: false,
                threshold_encryption: false,
                threshold_k: 2,
                threshold_n: 3,
                threshold_dev: false,
            },
        };

        toml::to_string_pretty(&sample).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ZelanaConfig::default();
        assert_eq!(config.api.port, 8080);
        assert_eq!(config.database.path, "./zelana-db");
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
        assert_eq!(parsed.api.port, 8080);
        assert!(parsed.features.dev_mode);
    }
}
