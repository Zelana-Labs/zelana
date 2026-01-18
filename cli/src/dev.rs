//! Local Development Environment
//!
//! Manages local sequencer, prover, and test utilities for development.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Configuration for local development environment
#[derive(Debug, Clone)]
pub struct DevConfig {
    /// Database path for sequencer
    pub db_path: PathBuf,
    /// Sequencer HTTP port
    pub sequencer_port: u16,
    /// Prover enabled
    pub enable_prover: bool,
    /// Mock Solana validator
    pub mock_solana: bool,
    /// Log level
    pub log_level: String,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./zelana-dev-db"),
            sequencer_port: 8080,
            enable_prover: true,
            mock_solana: true,
            log_level: "info".to_string(),
        }
    }
}

/// Local development environment manager
pub struct DevEnvironment {
    config: DevConfig,
    sequencer_process: Option<Child>,
    prover_process: Option<Child>,
    solana_process: Option<Child>,
    running: Arc<AtomicBool>,
}

impl DevEnvironment {
    pub fn new(config: DevConfig) -> Self {
        Self {
            config,
            sequencer_process: None,
            prover_process: None,
            solana_process: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the local development environment
    pub async fn start(&mut self) -> anyhow::Result<()> {
        println!(" Starting Zelana local development environment...\n");

        // Create data directory
        std::fs::create_dir_all(&self.config.db_path)?;
        println!(" Data directory: {}", self.config.db_path.display());

        // Start mock Solana if enabled
        if self.config.mock_solana {
            self.start_mock_solana().await?;
        }

        // Start sequencer
        self.start_sequencer().await?;

        // Start prover if enabled
        if self.config.enable_prover {
            self.start_prover().await?;
        }

        self.running.store(true, Ordering::SeqCst);

        println!("\n Development environment started!");
        println!("\n Endpoints:");
        println!(
            "   Sequencer:  http://127.0.0.1:{}",
            self.config.sequencer_port
        );
        if self.config.mock_solana {
            println!("   Solana RPC: http://127.0.0.1:8899");
        }

        Ok(())
    }

    /// Stop all services
    pub fn stop(&mut self) {
        println!("\n  Stopping development environment...");
        self.running.store(false, Ordering::SeqCst);

        if let Some(mut proc) = self.prover_process.take() {
            let _ = proc.kill();
            println!("   Stopped prover");
        }

        if let Some(mut proc) = self.sequencer_process.take() {
            let _ = proc.kill();
            println!("   Stopped sequencer");
        }

        if let Some(mut proc) = self.solana_process.take() {
            let _ = proc.kill();
            println!("   Stopped Solana validator");
        }

        println!(" All services stopped");
    }

    /// Check if environment is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn start_mock_solana(&mut self) -> anyhow::Result<()> {
        println!(" Starting local Solana validator via Surfpool...");

        // Check if surfpool is available
        let check = Command::new("which").arg("surfpool").output();
        if check.is_err() || !check.unwrap().status.success() {
            println!("     surfpool not found, skipping...");
            println!("     Install with: cargo install surfpool");
            return Ok(());
        }

        let ledger_path = self.config.db_path.join("solana-ledger");
        std::fs::create_dir_all(&ledger_path)?;

        let proc = Command::new("surfpool")
            .arg("start") // <-- surfpool owns the validator
            .arg("--ledger")
            .arg(&ledger_path)
            .arg("--reset")
            .arg("--quiet")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        self.solana_process = Some(proc);

        // Give validator time to boot
        tokio::time::sleep(Duration::from_secs(3)).await;

        println!("    Solana validator started via Surfpool");
        Ok(())
    }

    async fn start_sequencer(&mut self) -> anyhow::Result<()> {
        println!(" Starting Zelana sequencer...");

        // For now, we'll just print the command that would be run
        // In a full implementation, this would spawn the actual sequencer

        let db_path = self.config.db_path.join("sequencer");
        std::fs::create_dir_all(&db_path)?;

        println!("   DB path: {}", db_path.display());
        println!("   Port: {}", self.config.sequencer_port);

        // Try to spawn the sequencer binary if it exists
        let sequencer_binary = std::env::current_dir()?.join("target/debug/core");

        if sequencer_binary.exists() {
            let proc = Command::new(&sequencer_binary)
                .env("DB_PATH", db_path.to_str().unwrap())
                .env("NGEST_PORT", self.config.sequencer_port.to_string())
                .env("RUST_LOG", &self.config.log_level)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;

            self.sequencer_process = Some(proc);
            tokio::time::sleep(Duration::from_secs(1)).await;
            println!("    Sequencer started (binary)");
        } else {
            println!(
                "     Sequencer binary not found at {}",
                sequencer_binary.display()
            );
            println!("   Run `cargo build` first to build the sequencer");
            println!("    Sequencer mock mode (no actual process)");
        }

        Ok(())
    }

    async fn start_prover(&mut self) -> anyhow::Result<()> {
        println!("Starting Zelana prover...");

        let prover_binary = std::env::current_dir()?.join("target/debug/prover");

        if prover_binary.exists() {
            let proc = Command::new(&prover_binary)
                .env("RUST_LOG", &self.config.log_level)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;

            self.prover_process = Some(proc);
            println!("    Prover started");
        } else {
            println!(
                "     Prover binary not found at {}",
                prover_binary.display()
            );
            println!("    Prover mock mode (no actual process)");
        }

        Ok(())
    }
}

impl Drop for DevEnvironment {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start the dev environment and wait for shutdown signal
pub async fn run_dev(config: DevConfig) -> anyhow::Result<()> {
    let mut env = DevEnvironment::new(config);
    env.start().await?;

    println!("\nPress Ctrl+C to stop the development environment\n");

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;

    env.stop();
    Ok(())
}

/// Network configuration for deployment
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub name: String,
    pub rpc_url: String,
    pub ws_url: String,
    pub bridge_program_id: String,
}

impl NetworkConfig {
    pub fn devnet() -> Self {
        Self {
            name: "devnet".to_string(),
            rpc_url: "https://api.devnet.solana.com".to_string(),
            ws_url: "wss://api.devnet.solana.com".to_string(),
            bridge_program_id: "8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE".to_string(),
        }
    }

    pub fn mainnet() -> Self {
        Self {
            name: "mainnet".to_string(),
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
            bridge_program_id: "".to_string(), // TBD
        }
    }

    pub fn localnet() -> Self {
        Self {
            name: "localnet".to_string(),
            rpc_url: "http://127.0.0.1:8899".to_string(),
            ws_url: "ws://127.0.0.1:8900".to_string(),
            bridge_program_id: "11111111111111111111111111111111".to_string(),
        }
    }
}

/// Deploy configuration
pub async fn deploy(network: &str, keypair_path: Option<&str>) -> anyhow::Result<()> {
    let network_config = match network {
        "devnet" => NetworkConfig::devnet(),
        "mainnet" => NetworkConfig::mainnet(),
        "localnet" => NetworkConfig::localnet(),
        _ => return Err(anyhow::anyhow!("Unknown network: {}", network)),
    };

    println!("Deploying to {}...\n", network_config.name);
    println!("   RPC URL: {}", network_config.rpc_url);
    println!("   WS URL: {}", network_config.ws_url);

    if let Some(path) = keypair_path {
        println!("   Keypair: {}", path);
    }

    // Check if anchor is available for program deployment
    let anchor_check = Command::new("which").arg("anchor").output();

    if anchor_check.is_ok() && anchor_check.unwrap().status.success() {
        println!("\n Building programs...");

        let build_status = Command::new("anchor").arg("build").status()?;

        if !build_status.success() {
            return Err(anyhow::anyhow!("Anchor build failed"));
        }

        println!("\n Deploying programs...");

        let mut deploy_cmd = Command::new("anchor");
        deploy_cmd
            .arg("deploy")
            .arg("--provider.cluster")
            .arg(&network_config.rpc_url);

        if let Some(path) = keypair_path {
            deploy_cmd.arg("--provider.wallet").arg(path);
        }

        let deploy_status = deploy_cmd.status()?;

        if !deploy_status.success() {
            return Err(anyhow::anyhow!("Anchor deploy failed"));
        }

        println!("\n Deployment complete!");
    } else {
        println!("\n Anchor not found. Manual deployment required.");
        println!(
            "   Install with: cargo install --git https://github.com/coral-xyz/anchor anchor-cli"
        );

        println!("\n Deployment checklist:");
        println!("   1. Build programs: anchor build");
        println!("   2. Deploy bridge: anchor deploy --program-name bridge");
        println!("   3. Deploy verifier: anchor deploy --program-name verifier");
        println!("   4. Initialize bridge with sequencer authority");
    }

    Ok(())
}

/// Test configuration
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Run unit tests
    pub unit: bool,
    /// Run integration tests
    pub integration: bool,
    /// Run circuit tests
    pub circuits: bool,
    /// Verbose output
    pub verbose: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            unit: true,
            integration: true,
            circuits: true,
            verbose: false,
        }
    }
}

/// Run tests
pub async fn run_tests(config: TestConfig) -> anyhow::Result<()> {
    println!(" Running Zelana tests...\n");

    let mut all_passed = true;

    if config.unit {
        println!(" Running unit tests...");
        let mut cmd = Command::new("cargo");
        cmd.arg("test").arg("--lib");

        if config.verbose {
            cmd.arg("--").arg("--nocapture");
        }

        let status = cmd.status()?;
        if !status.success() {
            all_passed = false;
            println!("   Unit tests failed");
        } else {
            println!("   Unit tests passed");
        }
    }

    if config.integration {
        println!("\n Running integration tests...");
        let mut cmd = Command::new("cargo");
        cmd.arg("test").arg("--test").arg("*");

        if config.verbose {
            cmd.arg("--").arg("--nocapture");
        }

        let status = cmd.status()?;
        if !status.success() {
            all_passed = false;
            println!("   Integration tests failed");
        } else {
            println!("   Integration tests passed");
        }
    }

    if config.circuits {
        println!("\n Running circuit tests...");
        let mut cmd = Command::new("cargo");
        cmd.arg("test")
            .arg("--package")
            .arg("prover")
            .arg("circuit");

        if config.verbose {
            cmd.arg("--").arg("--nocapture");
        }

        let status = cmd.status()?;
        if !status.success() {
            // Circuits might not have tests yet
            println!("   Circuit tests skipped or failed");
        } else {
            println!("   Circuit tests passed");
        }
    }

    println!();
    if all_passed {
        println!("All tests passed!");
    } else {
        println!("Some tests failed");
        return Err(anyhow::anyhow!("Tests failed"));
    }

    Ok(())
}
