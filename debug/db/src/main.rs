use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, DB};
use tokio::sync::mpsc;
use zelana_account::AccountState;
use hex;

const CF_ACCOUNTS: &str = "accounts";
const MAX_VISIBLE_ACCOUNTS: usize = 20;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let db_path = env::var("DB_PATH").unwrap_or_else(|_| "./data/sequencer_db".to_string());

    let mut stdout = std::io::stdout();

    execute!(stdout, EnterAlternateScreen, Hide)?;
    terminal::enable_raw_mode()?;

    let mut previous_accounts: HashMap<String, AccountState> = HashMap::new();
    let mut scroll_offset: usize = 0;

    let (tx, mut rx) = mpsc::channel(10);

    // Key input task
    tokio::spawn({
        let tx = tx.clone();
        async move {
            loop {
                if event::poll(std::time::Duration::from_millis(50)).unwrap() {
                    if let Event::Key(key) = event::read().unwrap() {
                        match key.code {
                            KeyCode::Char('q') => { let _ = tx.send(KeyAction::Quit).await; break; }
                            KeyCode::Up => { let _ = tx.send(KeyAction::Up).await; }
                            KeyCode::Down => { let _ = tx.send(KeyAction::Down).await; }
                            KeyCode::PageUp => { let _ = tx.send(KeyAction::PageUp).await; }
                            KeyCode::PageDown => { let _ = tx.send(KeyAction::PageDown).await; }
                            KeyCode::Home => { let _ = tx.send(KeyAction::Home).await; }
                            KeyCode::End => { let _ = tx.send(KeyAction::End).await; }
                            _ => {}
                        }
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                inspect_and_draw(&db_path, &mut previous_accounts, scroll_offset)?;
            }
            Some(action) = rx.recv() => {
                match action {
                    KeyAction::Quit => break,
                    KeyAction::Up => { 
                        if scroll_offset > 0 { 
                            scroll_offset -= 1; 
                        } 
                    },
                    KeyAction::Down => { 
                        scroll_offset += 1; 
                    },
                    KeyAction::PageUp => { 
                        scroll_offset = scroll_offset.saturating_sub(MAX_VISIBLE_ACCOUNTS); 
                    },
                    KeyAction::PageDown => { 
                        scroll_offset += MAX_VISIBLE_ACCOUNTS; 
                    },
                    KeyAction::Home => { 
                        scroll_offset = 0; 
                    },
                    KeyAction::End => { 
                        scroll_offset = usize::MAX; 
                    },
                }
            }
        }
    }

    // Cleanup
    execute!(stdout, Show, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    println!("👋 Exiting inspector...");

    Ok(())
}

enum KeyAction { Quit, Up, Down, PageUp, PageDown, Home, End }

fn inspect_and_draw(
    db_path: &str,
    previous_accounts: &mut HashMap<String, AccountState>,
    scroll_offset: usize
) -> Result<()> {
    let rocks_path = PathBuf::from(db_path);
    let mut opts = Options::default();
    opts.create_if_missing(false);
    opts.create_missing_column_families(false);
    let cf_opts = Options::default();

    let db = DB::open_cf_descriptors_read_only(
        &opts,
        &rocks_path,
        vec![ColumnFamilyDescriptor::new(CF_ACCOUNTS, cf_opts)],
        false,
    ).or_else(|_| {
        let secondary_path = PathBuf::from(format!("{}_secondary", db_path));
        DB::open_cf_as_secondary(&opts, &rocks_path, &secondary_path, &[CF_ACCOUNTS]).context("Failed to open secondary DB")
    })?;

    let cf = db.cf_handle(CF_ACCOUNTS).context("Missing 'accounts' CF")?;
    let mut accounts: Vec<(String, AccountState)> = vec![];

    for entry in db.iterator_cf(&cf, IteratorMode::Start) {
        let (key_bytes, value_bytes) = entry?;
        if key_bytes.len() == 32 {
            let account_hex = hex::encode(&key_bytes);
            if let Ok(state) = wincode::deserialize::<AccountState>(&value_bytes) {
                accounts.push((account_hex, AccountState { balance: state.balance, nonce: 0 }));
            }
        }
    }
    accounts.sort_by_key(|(k, _)| k.clone());
    *previous_accounts = accounts.iter().cloned().collect();

    // Clamp scroll offset to valid range
    let max_scroll = accounts.len().saturating_sub(MAX_VISIBLE_ACCOUNTS);
    let scroll_offset = scroll_offset.min(max_scroll);
    
    // Get visible slice
    let visible_start = scroll_offset;
    let visible_end = (scroll_offset + MAX_VISIBLE_ACCOUNTS).min(accounts.len());
    let visible_accounts = &accounts[visible_start..visible_end];

    let mut stdout = std::io::stdout();
    execute!(stdout, MoveTo(0,0), Clear(ClearType::All))?;

    // Header
    execute!(stdout, Print("╔════════════════════════════════════════════════════════════════════╗\r\n"))?;
    execute!(stdout, Print("║              ROCKSDB DATABASE INSPECTOR (LIVE MODE)                ║\r\n"))?;
    execute!(stdout, Print("╚════════════════════════════════════════════════════════════════════╝\r\n"))?;
    execute!(stdout, Print("[↑/↓: scroll | PgUp/PgDn: page | Home/End: jump | 'q': quit]\r\n\r\n"))?;

    // Table header
    execute!(stdout, Print("╔══════════════════════════════════════════════════════════════════╦══════════════════════╗\r\n"))?;
    execute!(stdout, Print(format!("║ {:^64} ║ {:>20} ║\r\n", "Account ID", "Balance")))?;
    execute!(stdout, Print("╠══════════════════════════════════════════════════════════════════╬══════════════════════╣\r\n"))?;

    // Table rows (always exactly 20 rows for consistent layout)
    for i in 0..MAX_VISIBLE_ACCOUNTS {
        if i < visible_accounts.len() {
            let (account_id, data) = &visible_accounts[i];
            execute!(stdout, Print(format!("║ {:<64} ║ {:>20} ║\r\n", account_id, data.balance)))?;
        } else {
            // Empty row for consistent height
            execute!(stdout, Print(format!("║ {:64} ║ {:>20} ║\r\n", "", "")))?;
        }
    }

    // Table footer
    execute!(stdout, Print("╚══════════════════════════════════════════════════════════════════╩══════════════════════╝\r\n"))?;
    
    // Status line
    if accounts.is_empty() {
        execute!(stdout, Print("No accounts found\r\n"))?;
    } else {
        let actual_visible = visible_accounts.len();
        
        if actual_visible > 0 {
            execute!(stdout, Print(format!("Showing {}-{} of {} accounts\r\n",
                visible_start + 1,
                visible_start + actual_visible,
                accounts.len()
            )))?;
        } else {
            execute!(stdout, Print(format!("Total: {} accounts\r\n", accounts.len())))?;
        }
    }

    stdout.flush()?;
    Ok(())
}