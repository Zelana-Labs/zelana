use std::env;
use std::path::PathBuf;
use std::collections::HashMap;

use rocksdb::{IteratorMode, Options, ColumnFamilyDescriptor, DB};
use anyhow::{Context, Result};
use zelana_account::{AccountState};
use hex;

const CF_ACCOUNTS: &str = "accounts";


#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let db_path = env::var("DB_PATH").unwrap_or_else(|_| "./data/sequencer_db".to_string());
    
    // Enable raw mode for better terminal control
    crossterm::terminal::enable_raw_mode()?;
    
    // Print header once
    print!("\x1B[2J\x1B[1;1H"); // Clear screen
    println!("╔════════════════════════════════════════════════════════════════════╗\r");
    println!("║              ROCKSDB DATABASE INSPECTOR (LIVE MODE)                ║\r");
    println!("╚════════════════════════════════════════════════════════════════════╝\r");
    println!("\r");
    println!("[Auto-refresh every 2s. Press 'q' to quit]\r");
    println!("\r");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    
    let mut previous_accounts: HashMap<String, AccountState> = HashMap::new();
    let mut db_info_line = 6; // Track where we print db info
    
    // Handle Ctrl+C gracefully
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move {
        loop {
            if crossterm::event::poll(std::time::Duration::from_millis(100)).unwrap() {
                if let Ok(event) = crossterm::event::read() {
                    if let crossterm::event::Event::Key(key) = event {
                        if key.code == crossterm::event::KeyCode::Char('q') {
                            let _ = tx.send(()).await;
                            break;
                        }
                    }
                }
            }
        }
    });
    
    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {
                match inspect_and_update(&db_path, &mut previous_accounts, &mut db_info_line) {
                    Ok(_) => {},
                    Err(e) => {
                        print!("\x1B[{};1H", db_info_line);
                        print!("\x1B[K❌ Error: {:?}\r", e);
                        db_info_line += 1;
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                }
            }
            _ = rx.recv() => {
                break;
            }
        }
    }
    
    // Cleanup
    crossterm::terminal::disable_raw_mode()?;
    println!("\n👋 Exiting inspector...\n");
    
    Ok(())
}

fn inspect_and_update(
    db_path: &str, 
    previous_accounts: &mut HashMap<String, AccountState>,
    db_info_line: &mut usize,
) -> Result<()> {
    let rocks_path = PathBuf::from(db_path);

    // Open the database
    let mut opts = Options::default();
    opts.create_if_missing(false);
    opts.create_missing_column_families(false);
    opts.set_max_open_files(16);

    let cf_opts = Options::default();

    let (db, is_secondary) = match DB::open_cf_descriptors_read_only(
        &opts, 
        &rocks_path, 
        vec![ColumnFamilyDescriptor::new(CF_ACCOUNTS, cf_opts.clone())], 
        false
    ) {
        Ok(db) => (db, false),
        Err(_) => {
            let secondary_path = PathBuf::from(format!("{}_secondary", db_path));
            let secondary_db = DB::open_cf_as_secondary(
                &opts, 
                &rocks_path, 
                &secondary_path, 
                &[CF_ACCOUNTS]
            ).context("Failed to open as secondary")?;
            (secondary_db, true)
        }
    };
    
    // Sync with primary if using secondary instance
    if is_secondary {
        db.try_catch_up_with_primary()
            .context("Failed to sync with primary")?;
    }

    // Update database info
    *db_info_line = 6;
    print!("\x1B[{};1H", *db_info_line);
    *db_info_line += 1;
    
    let db_size = get_directory_size(&rocks_path)?;
    print!("\x1B[K📂 Path: {} | 💾 Size: {} | ⏰ {}\r", 
        truncate(&rocks_path.display().to_string(), 30),
        format_size(db_size),
        chrono::Local::now().format("%H:%M:%S")
    );
    
    print!("\x1B[{};1H", *db_info_line);
    *db_info_line += 1;
    print!("\x1B[K\r");

    // Get current accounts
    let cf = db.cf_handle(CF_ACCOUNTS).context("Column family 'accounts' missing")?;
    let mut current_accounts: HashMap<String, AccountState> = HashMap::new();

    for entry in db.iterator_cf(&cf, IteratorMode::Start) {
        let (key_bytes, value_bytes) = entry?;
        
        if key_bytes.len() == 32 {
            let mut account_id = [0u8; 32];
            account_id.copy_from_slice(&key_bytes);
            let account_hex = hex::encode(account_id);
            
            if let Ok(state) = wincode::deserialize::<AccountState>(&value_bytes) {
                current_accounts.insert(account_hex, AccountState { balance: state.balance, nonce: 0 });
            }
        }
    }

    // Always redraw the table structure for now (simpler and more reliable)
    let start_line = *db_info_line;
    let mut current_line = start_line;
    
    print!("\x1B[{};1H", current_line);
    current_line += 1;
    print!("\x1B[K┌────────────────────────────────────────────────────────────────────┐\r");
    
    print!("\x1B[{};1H", current_line);
    current_line += 1;
    print!("\x1B[K│  {:^62} │\r", format!("ACCOUNTS ({})", current_accounts.len()));
    
    print!("\x1B[{};1H", current_line);
    current_line += 1;
    print!("\x1B[K└────────────────────────────────────────────────────────────────────┘\r");
    
    print!("\x1B[{};1H", current_line);
    current_line += 1;
    print!("\x1B[K╔══════════════════════════════════════════════════════════════════╦══════════════════════╗\r");
    
    print!("\x1B[{};1H", current_line);
    current_line += 1;
    print!("\x1B[K║ {:^64} ║ {:>20} ║\r", "Account ID", "Balance");
    
    print!("\x1B[{};1H", current_line);
    current_line += 1;
    print!("\x1B[K╠══════════════════════════════════════════════════════════════════╬══════════════════════╣\r");

    // Print all account rows
    let mut sorted_accounts: Vec<_> = current_accounts.iter().collect();
    sorted_accounts.sort_by_key(|(k, _)| *k);

    for (account_id, data) in sorted_accounts {
        print!("\x1B[{};1H", current_line);
        current_line += 1;
        print!("\x1B[K║ {:<64} ║ {:>20} ║\r", account_id, data.balance);
    }

    // Print bottom border
    print!("\x1B[{};1H", current_line);
    current_line += 1;
    print!("\x1B[K╚══════════════════════════════════════════════════════════════════╩══════════════════════╝\r");
    
    // Clear any extra lines below
    for _ in 0..5 {
        print!("\x1B[{};1H", current_line);
        current_line += 1;
        print!("\x1B[K\r");
    }
    
    std::io::Write::flush(&mut std::io::stdout()).unwrap();

    *previous_accounts = current_accounts;
    *db_info_line = current_line;

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("...{}", &s[s.len() - (max_len - 3)..])
    }
}

fn get_directory_size(path: &PathBuf) -> Result<u64> {
    let mut total_size = 0u64;
    
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            
            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += get_directory_size(&entry.path())?;
            }
        }
    }
    
    Ok(total_size)
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}