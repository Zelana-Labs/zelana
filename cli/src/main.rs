use std::{env, fs::OpenOptions, fs};
use zelana_keypair::Keypair;
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: program <command> [args]");
        println!("Commands:");
        println!("  genkey        - Generate new keypair");
        println!("  add <a> <b>   - Add two numbers");
        println!("  upper <text>  - Convert to uppercase");
        return;
    }

    let cmd = &args[1];

    match cmd.as_str() {
        "genkey" => {
            if args.len() > 2 {
                println!("Usage: genkey");
                return;
            }
            if let Err(e) = genkey() {
                eprintln!("Error generating key: {}", e);
            }
        }
        "add" => {
            if args.len() < 4 {
                println!("Usage: add <a> <b>");
                return;
            }
            add(&args[2], &args[3]);
        }
        "upper" => {
            if args.len() < 3 {
                println!("Usage: upper <text>");
                return;
            }
            upper(&args[2]);
        }
        _ => println!("Unknown command: {}", cmd),
    }
}

fn genkey() -> std::io::Result<()> {
    // Check if file already exists
    if fs::metadata("id.json").is_ok() {
        eprintln!("Error: id.json already exists. Remove it first or use a different path.");
        std::process::exit(1);
    }

    let key = Keypair::new_random();
    let pubkeys = key.public_keys().as_bs58();

    // Create JSON array format like Solana (array of 64 bytes)
    // We need to add to_seed() method to Keypair first
    let seed = key.to_seed();
    
    // Format as JSON array
    let json_array: Vec<u8> = seed.to_vec();
    let json = serde_json::to_string(&json_array)?;

    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open("id.json")?;
    
    #[cfg(unix)]
    {
        // chmod 600 (rw-------)
        let mut perms = f.metadata()?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions("id.json", perms)?;
    }
    
    f.write_all(json.as_bytes())?;

    println!("Wrote new keypair to id.json");
    println!("pubkeys: {:?}", pubkeys);
    
    Ok(())
}

fn add(a: &str, b: &str) {
    match (a.parse::<i32>(), b.parse::<i32>()) {
        (Ok(x), Ok(y)) => println!("{} + {} = {}", x, y, x + y),
        _ => println!("Error: Both arguments must be numbers"),
    }
}

fn upper(text: &str) {
    println!("{}", text.to_uppercase());
}