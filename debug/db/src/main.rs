use std::{env, path::PathBuf, sync::Arc};

use rocksdb::{IteratorMode, Options, ColumnFamilyDescriptor, DB};
use anyhow::{Context, Result};
use zelana_account::{AccountState, AccountId};
use hex;

const CF_ACCOUNTS: &str = "accounts";

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let db_path = env::var("DB_PATH").unwrap_or_else(|_| "./data/sequencer_db".to_string());
    let rocks_path = PathBuf::from(&db_path);

    // Open the database the same way as your RocksDbStore
    let mut opts = Options::default();
    opts.create_if_missing(false); // Don't create if inspecting
    opts.create_missing_column_families(false);

    let cf_opts = Options::default();
    let families = vec![ColumnFamilyDescriptor::new(CF_ACCOUNTS, cf_opts)];

    let db = DB::open_cf_descriptors(&opts, &rocks_path, families)
        .map_err(|e| anyhow::anyhow!("Failed to open RocksDB: {}", e))?;

    println!("\n╔════════════════════════════════════════════════════════════════════╗");
    println!("║                      ROCKSDB DATABASE INSPECTOR                    ║");
    println!("║ Path: {:<58} ║", truncate(&rocks_path.display().to_string(), 58));
    println!("╚════════════════════════════════════════════════════════════════════╝");

    // ========== ACCOUNTS ==========
    let cf = db
        .cf_handle(CF_ACCOUNTS)
        .context("Column family 'accounts' missing")?;
    
    let mut rows: Vec<Vec<String>> = Vec::new();

    for entry in db.iterator_cf(&cf, IteratorMode::Start) {
        let (key_bytes, value_bytes) = entry?;
        
        if key_bytes.len() == 32 {
            let mut account_id = [0u8; 32];
            account_id.copy_from_slice(&key_bytes);
            
            // Deserialize using wincode (match your implementation)
            match wincode::deserialize::<AccountState>(&value_bytes) {
                Ok(state) => {
                    rows.push(vec![
                        hex::encode(account_id),
                        state.balance.to_string(),
                    ]);
                }
                Err(e) => {
                    rows.push(vec![
                        hex::encode(account_id),
                        format!("Error: {}", e),
                    ]);
                }
            }
        }
    }

    print_table_header("ACCOUNTS", rows.len());
    if rows.is_empty() {
        print_empty_table();
    } else {
        print_wrapped_table(
            &["Account ID", "Balance"],
            &[64, 20],
            &["<", ">"],
            &rows,
        );
    }

    println!("\n╔════════════════════════════════════════════════════════════════════╗");
    println!("║                     INSPECTION COMPLETE                            ║");
    println!("╚════════════════════════════════════════════════════════════════════╝\n");

    Ok(())
}

fn print_table_header(name: &str, count: usize) {
    println!("┌────────────────────────────────────────────────────────────────────┐");
    println!("│ 📊 {:^62} │", format!("{} ({})", name, count));
    println!("└────────────────────────────────────────────────────────────────────┘");
}

fn print_empty_table() {
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║                            (No data)                               ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("...{}", &s[s.len() - (max_len - 3)..])
    }
}

/// Generic table printer with wrapping.
fn print_wrapped_table(
    headers: &[&str],
    widths: &[usize],
    aligns: &[&str],
    rows: &[Vec<String>],
) {
    assert_eq!(headers.len(), widths.len());
    assert_eq!(widths.len(), aligns.len());

    // top border
    print!("╔");
    for (i, w) in widths.iter().enumerate() {
        print!("{}", "═".repeat(w + 2));
        if i < widths.len() - 1 {
            print!("╦");
        }
    }
    println!("╗");

    // header row
    print!("║");
    for ((h, w), _) in headers.iter().zip(widths.iter()).zip(aligns.iter()) {
        let formatted = format!("{:^width$}", h, width = *w);
        print!(" {} ║", formatted);
    }
    println!();

    // header separator
    print!("╠");
    for (i, w) in widths.iter().enumerate() {
        print!("{}", "═".repeat(w + 2));
        if i < widths.len() - 1 {
            print!("╬");
        }
    }
    println!("╣");

    // data rows
    for (row_idx, row) in rows.iter().enumerate() {
        let is_last = row_idx == rows.len() - 1;
        print_wrapped_row(row, widths, aligns, is_last);
    }

    // bottom border
    if !rows.is_empty() {
        print!("╚");
        for (i, w) in widths.iter().enumerate() {
            print!("{}", "═".repeat(w + 2));
            if i < widths.len() - 1 {
                print!("╩");
            }
        }
        println!("╝");
    }
}

/// Wrap a single cell string into multiple lines of at most `width` chars.
fn wrap_cell(s: &str, width: usize) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let end = (i + width).min(s.len());
        out.push(s[i..end].to_string());
        i = end;
    }
    out
}

/// Print one logical table row, wrapping long cells onto multiple lines.
fn print_wrapped_row(cells: &[String], widths: &[usize], aligns: &[&str], is_last: bool) {
    let wrapped: Vec<Vec<String>> = cells
        .iter()
        .zip(widths.iter())
        .map(|(s, &w)| wrap_cell(s, w))
        .collect();

    let max_lines = wrapped.iter().map(|lines| lines.len()).max().unwrap_or(0);

    for line_idx in 0..max_lines {
        print!("║");
        for (col_idx, col_lines) in wrapped.iter().enumerate() {
            let w = widths[col_idx];
            let content = col_lines.get(line_idx).map(|s| s.as_str()).unwrap_or("");

            let formatted = match aligns[col_idx] {
                ">" => format!("{:>width$}", content, width = w),
                "^" => format!("{:^width$}", content, width = w),
                _ => format!("{:<width$}", content, width = w),
            };

            print!(" {} ║", formatted);
        }
        println!();
    }

    if !is_last {
        print!("╟");
        for (i, &w) in widths.iter().enumerate() {
            print!("{}", "─".repeat(w + 2));
            if i < widths.len() - 1 {
                print!("╫");
            }
        }
        println!("╢");
    }
}