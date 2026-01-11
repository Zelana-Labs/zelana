mod airdrop;
mod dev;

use std::io::Write;
use std::path::Path;
use std::{env, fs, fs::OpenOptions};
use zelana_keypair::Keypair;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::main]
async fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    let cmd = &args[1];

    match cmd.as_str() {
        "dev" => {
            let config = parse_dev_args(&args[2..]);
            if let Err(e) = dev::run_dev(config).await {
                eprintln!("❌ Error running dev environment: {}", e);
                std::process::exit(1);
            }
        }
        "test" => {
            let config = parse_test_args(&args[2..]);
            if let Err(e) = dev::run_tests(config).await {
                eprintln!("❌ Error running tests: {}", e);
                std::process::exit(1);
            }
        }
        "deploy" => {
            let network = args.get(2).map(|s| s.as_str()).unwrap_or("devnet");
            let keypair = args.get(3).map(|s| s.as_str());

            // Handle --network flag
            let (network, keypair) = if network == "--network" {
                let net = args.get(3).map(|s| s.as_str()).unwrap_or("devnet");
                let kp = args.get(4).map(|s| s.as_str());
                (net, kp)
            } else {
                (network, keypair)
            };

            if let Err(e) = dev::deploy(network, keypair).await {
                eprintln!("❌ Error deploying: {}", e);
                std::process::exit(1);
            }
        }
        "genkey" => {
            let filename = args.get(2).cloned();
            if let Err(e) = genkey(filename) {
                eprintln!("❌ Error generating key: {}", e);
                std::process::exit(1);
            }
        }
        "airdrop" => {
            if args.len() < 3 {
                println!("Usage: airdrop <amount> [filename]");
                println!("  amount   - Amount in lamports to bridge to L2");
                println!("  filename - Optional keypair file (default: id.json)");
                return;
            }

            let amount: u64 = match args[2].parse() {
                Ok(amt) => amt,
                Err(_) => {
                    eprintln!("❌ Error: Amount must be a valid number");
                    return;
                }
            };

            let filename = args.get(3).cloned();

            if let Err(e) = airdrop(amount, filename).await {
                eprintln!("❌ Error during airdrop and bridge: {}", e);
                std::process::exit(1);
            }
        }
        "add" => {
            if args.len() < 4 {
                println!("Usage: add <a> <b>");
                return;
            }
            add(&args[2], &args[3]);
        }
        "help" | "--help" | "-h" => {
            print_usage();
        }
        _ => {
            println!("❌ Unknown command: {}", cmd);
            println!();
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("Zelana CLI - Privacy-First L2 Development Tool");
    println!();
    println!("USAGE:");
    println!("  zelana <command> [args]");
    println!();
    println!("DEVELOPMENT COMMANDS:");
    println!("  dev [options]              Start local sequencer + prover");
    println!("  test [options]             Run integration tests");
    println!("  deploy --network <net>     Deploy to network (devnet/mainnet/localnet)");
    println!();
    println!("ACCOUNT COMMANDS:");
    println!("  genkey [filename]          Generate new Zelana keypair");
    println!("  airdrop <amount> [file]    Request Solana airdrop and bridge to L2");
    println!();
    println!("OTHER COMMANDS:");
    println!("  help                       Show this help message");
    println!();
    println!("DEV OPTIONS:");
    println!("  --port <port>              Sequencer port (default: 8080)");
    println!("  --no-prover                Disable prover");
    println!("  --no-solana                Don't start local Solana validator");
    println!("  --verbose                  Verbose logging");
    println!();
    println!("TEST OPTIONS:");
    println!("  --unit                     Run only unit tests");
    println!("  --integration              Run only integration tests");
    println!("  --circuits                 Run only circuit tests");
    println!("  --verbose                  Verbose output");
    println!();
    println!("EXAMPLES:");
    println!("  zelana dev                           # Start local dev environment");
    println!("  zelana dev --port 9000               # Use custom port");
    println!("  zelana test                          # Run all tests");
    println!("  zelana test --integration            # Run only integration tests");
    println!("  zelana deploy --network devnet       # Deploy to devnet");
    println!("  zelana genkey                        # Generate keypair");
    println!("  zelana airdrop 1000000000            # Airdrop and bridge");
    println!();
    println!("ENVIRONMENT VARIABLES:");
    println!("  SOLANA_RPC_URL       Solana RPC endpoint");
    println!("  ZELANA_DB_PATH       Database path for sequencer");
    println!("  RUST_LOG             Log level (debug/info/warn/error)");
}

fn parse_dev_args(args: &[String]) -> dev::DevConfig {
    let mut config = dev::DevConfig::default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                if let Some(port_str) = args.get(i + 1) {
                    if let Ok(port) = port_str.parse() {
                        config.sequencer_port = port;
                    }
                    i += 1;
                }
            }
            "--no-prover" => {
                config.enable_prover = false;
            }
            "--no-solana" => {
                config.mock_solana = false;
            }
            "--verbose" | "-v" => {
                config.log_level = "debug".to_string();
            }
            "--db-path" => {
                if let Some(path) = args.get(i + 1) {
                    config.db_path = std::path::PathBuf::from(path);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    config
}

fn parse_test_args(args: &[String]) -> dev::TestConfig {
    let mut config = dev::TestConfig::default();
    let mut any_specific = false;

    for arg in args {
        match arg.as_str() {
            "--unit" => {
                any_specific = true;
                config.unit = true;
            }
            "--integration" => {
                any_specific = true;
                config.integration = true;
            }
            "--circuits" => {
                any_specific = true;
                config.circuits = true;
            }
            "--verbose" | "-v" => {
                config.verbose = true;
            }
            _ => {}
        }
    }

    // If specific tests were requested, disable others
    if any_specific {
        if !args.iter().any(|a| a == "--unit") {
            config.unit = false;
        }
        if !args.iter().any(|a| a == "--integration") {
            config.integration = false;
        }
        if !args.iter().any(|a| a == "--circuits") {
            config.circuits = false;
        }
    }

    config
}

fn genkey(filename: Option<String>) -> anyhow::Result<()> {
    // Get home directory and construct path
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;

    let config_dir = Path::new(&home)
        .join(".config")
        .join("solana")
        .join("zelana");

    // Use provided filename or default to "id.json"
    let key_filename = filename.unwrap_or_else(|| "id.json".to_string());
    let key_path = config_dir.join(&key_filename);

    // Create directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
        println!("📁 Created directory: {}", config_dir.display());

        #[cfg(unix)]
        {
            // Set directory permissions to 700 (rwx------)
            let mut perms = fs::metadata(&config_dir)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(&config_dir, perms)?;
        }
    }

    // Check if file already exists
    if key_path.exists() {
        return Err(anyhow::anyhow!(
            "File {} already exists. Remove it first or use a different filename.",
            key_path.display()
        ));
    }

    println!("🔐 Generating new keypair...");
    let key = Keypair::new_random();
    let pubkeys = key.public_keys().as_bs58();

    // Create JSON array format like Solana (array of 64 bytes)
    let seed = key.to_seed();

    // Format as JSON array
    let json_array: Vec<u8> = seed.to_vec();
    let json = serde_json::to_string(&json_array)?;

    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&key_path)?;

    #[cfg(unix)]
    {
        // chmod 600 (rw-------)
        let mut perms = f.metadata()?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&key_path, perms)?;
    }

    f.write_all(json.as_bytes())?;

    println!("✅ Wrote new keypair to {}", key_path.display());
    println!("🔑 Public keys: {:?}", pubkeys);

    Ok(())
}

fn add(a: &str, b: &str) {
    match (a.parse::<i32>(), b.parse::<i32>()) {
        (Ok(x), Ok(y)) => println!("{} + {} = {}", x, y, x + y),
        _ => println!("❌ Error: Both arguments must be numbers"),
    }
}

async fn airdrop(amount: u64, filename: Option<String>) -> anyhow::Result<()> {
    // Determine keypair path
    let key_path = if let Some(filename) = filename {
        // If filename provided, use it as-is (could be relative or absolute)
        Path::new(&filename).to_path_buf()
    } else {
        // Default to ~/.config/solana/zelana/id.json
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;

        Path::new(&home)
            .join(".config")
            .join("solana")
            .join("zelana")
            .join("id.json")
    };

    // Load the keypair
    println!("🔑 Loading keypair from {}...", key_path.display());
    let keypair = Keypair::from_file(
        key_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
    )?;

    // Create bridge config from environment
    let config = airdrop::BridgeConfig::default();

    // Execute airdrop and bridge flow
    airdrop::airdrop_and_bridge_flow(&keypair, amount, &config).await?;

    Ok(())
}
