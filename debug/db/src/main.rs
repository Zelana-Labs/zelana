use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, DB};
use zelana_account::AccountState;
use zelana_transaction::Transaction;
use hex;

const CF_ACCOUNTS: &str = "accounts";
const CF_TRANSACTIONS: &str = "transactions";
const CF_NULLIFIERS: &str = "nullifiers";

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Accounts,
    Transactions,
    Nullifiers,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Search,
}

struct App {
    active_panel: Panel,
    mode: Mode,
    accounts: Vec<(String, AccountState)>,
    transactions: Vec<(String, Transaction)>,
    nullifiers: Vec<String>,
    filtered_accounts: Vec<usize>,
    filtered_transactions: Vec<usize>,
    filtered_nullifiers: Vec<usize>,
    accounts_scroll: usize,
    transactions_scroll: usize,
    nullifiers_scroll: usize,
    search_query: String,
    status_message: String,
}

fn format_balance_with_separators(balance: u64) -> String {
    let balance_str = balance.to_string();
    let mut result = String::new();
    let len = balance_str.len();
    
    for (i, c) in balance_str.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push('.');
        }
        result.push(c);
    }
    
    result
}

impl App {
    fn new() -> Self {
        Self {
            active_panel: Panel::Accounts,
            mode: Mode::Normal,
            accounts: Vec::new(),
            transactions: Vec::new(),
            nullifiers: Vec::new(),
            filtered_accounts: Vec::new(),
            filtered_transactions: Vec::new(),
            filtered_nullifiers: Vec::new(),
            accounts_scroll: 0,
            transactions_scroll: 0,
            nullifiers_scroll: 0,
            search_query: String::new(),
            status_message: String::new(),
        }
    }

    fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Accounts => Panel::Transactions,
            Panel::Transactions => Panel::Nullifiers,
            Panel::Nullifiers => Panel::Accounts,
        };
        self.reset_scroll();
    }

    fn previous_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Accounts => Panel::Nullifiers,
            Panel::Transactions => Panel::Accounts,
            Panel::Nullifiers => Panel::Transactions,
        };
        self.reset_scroll();
    }

    fn reset_scroll(&mut self) {
        match self.active_panel {
            Panel::Accounts => self.accounts_scroll = 0,
            Panel::Transactions => self.transactions_scroll = 0,
            Panel::Nullifiers => self.nullifiers_scroll = 0,
        }
    }

    fn scroll_up(&mut self) {
        match self.active_panel {
            Panel::Accounts => {
                self.accounts_scroll = self.accounts_scroll.saturating_sub(1);
            }
            Panel::Transactions => {
                self.transactions_scroll = self.transactions_scroll.saturating_sub(1);
            }
            Panel::Nullifiers => {
                self.nullifiers_scroll = self.nullifiers_scroll.saturating_sub(1);
            }
        }
    }

    fn scroll_down(&mut self) {
        match self.active_panel {
            Panel::Accounts => {
                let max = if self.search_query.is_empty() {
                    self.accounts.len()
                } else {
                    self.filtered_accounts.len()
                };
                if self.accounts_scroll < max.saturating_sub(1) {
                    self.accounts_scroll += 1;
                }
            }
            Panel::Transactions => {
                let max = if self.search_query.is_empty() {
                    self.transactions.len()
                } else {
                    self.filtered_transactions.len()
                };
                if self.transactions_scroll < max.saturating_sub(1) {
                    self.transactions_scroll += 1;
                }
            }
            Panel::Nullifiers => {
                let max = if self.search_query.is_empty() {
                    self.nullifiers.len()
                } else {
                    self.filtered_nullifiers.len()
                };
                if self.nullifiers_scroll < max.saturating_sub(1) {
                    self.nullifiers_scroll += 1;
                }
            }
        }
    }

    fn apply_search(&mut self) {
        let query = self.search_query.to_lowercase();
        
        if query.is_empty() {
            self.filtered_accounts.clear();
            self.filtered_transactions.clear();
            self.filtered_nullifiers.clear();
            self.reset_scroll();
            return;
        }

        self.filtered_accounts = self
            .accounts
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| id.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        self.filtered_transactions = self
            .transactions
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| id.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        self.filtered_nullifiers = self
            .nullifiers
            .iter()
            .enumerate()
            .filter(|(_, nf)| nf.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        self.reset_scroll();
    }

    fn copy_current_item(&mut self) {
        let text_to_copy = match self.active_panel {
            Panel::Accounts => {
                if self.search_query.is_empty() {
                    if self.accounts_scroll < self.accounts.len() {
                        Some(self.accounts[self.accounts_scroll].0.clone())
                    } else {
                        None
                    }
                } else {
                    if self.accounts_scroll < self.filtered_accounts.len() {
                        let idx = self.filtered_accounts[self.accounts_scroll];
                        Some(self.accounts[idx].0.clone())
                    } else {
                        None
                    }
                }
            }
            Panel::Transactions => {
                if self.search_query.is_empty() {
                    if self.transactions_scroll < self.transactions.len() {
                        Some(self.transactions[self.transactions_scroll].0.clone())
                    } else {
                        None
                    }
                } else {
                    if self.transactions_scroll < self.filtered_transactions.len() {
                        let idx = self.filtered_transactions[self.transactions_scroll];
                        Some(self.transactions[idx].0.clone())
                    } else {
                        None
                    }
                }
            }
            Panel::Nullifiers => {
                if self.search_query.is_empty() {
                    if self.nullifiers_scroll < self.nullifiers.len() {
                        Some(self.nullifiers[self.nullifiers_scroll].clone())
                    } else {
                        None
                    }
                } else {
                    if self.nullifiers_scroll < self.filtered_nullifiers.len() {
                        let idx = self.filtered_nullifiers[self.nullifiers_scroll];
                        Some(self.nullifiers[idx].clone())
                    } else {
                        None
                    }
                }
            }
        };

        if let Some(text) = text_to_copy {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                // Try xclip first
                if let Ok(_) = Command::new("xclip")
                    .arg("-selection")
                    .arg("clipboard")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(mut stdin) = child.stdin.take() {
                            stdin.write_all(text.as_bytes())?;
                        }
                        child.wait()
                    })
                {
                    self.status_message = "Copied to clipboard (xclip)".to_string();
                    return;
                }
                
                // Try xsel as fallback
                if let Ok(_) = Command::new("xsel")
                    .arg("--clipboard")
                    .arg("--input")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(mut stdin) = child.stdin.take() {
                            stdin.write_all(text.as_bytes())?;
                        }
                        child.wait()
                    })
                {
                    self.status_message = "Copied to clipboard (xsel)".to_string();
                    return;
                }
                
                self.status_message = format!("Install xclip or xsel. Text: {}", &text[..text.len().min(20)]);
            }
            
            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                if let Ok(_) = Command::new("pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(mut stdin) = child.stdin.take() {
                            stdin.write_all(text.as_bytes())?;
                        }
                        child.wait()
                    })
                {
                    self.status_message = "Copied to clipboard".to_string();
                } else {
                    self.status_message = "Failed to copy".to_string();
                }
            }
            
            #[cfg(target_os = "windows")]
            {
                use std::process::Command;
                if let Ok(_) = Command::new("clip")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(mut stdin) = child.stdin.take() {
                            stdin.write_all(text.as_bytes())?;
                        }
                        child.wait()
                    })
                {
                    self.status_message = "Copied to clipboard".to_string();
                } else {
                    self.status_message = "Failed to copy".to_string();
                }
            }
        } else {
            self.status_message = "Nothing to copy".to_string();
        }
    }

    fn load_data(&mut self, db_path: &str) -> Result<()> {
        let rocks_path = PathBuf::from(db_path);
        let mut opts = Options::default();
        opts.create_if_missing(false);
        opts.create_missing_column_families(false);
        let cf_opts = Options::default();

        let db = DB::open_cf_descriptors_read_only(
            &opts,
            &rocks_path,
            vec![
                ColumnFamilyDescriptor::new(CF_ACCOUNTS, cf_opts.clone()),
                ColumnFamilyDescriptor::new(CF_TRANSACTIONS, cf_opts.clone()),
                ColumnFamilyDescriptor::new(CF_NULLIFIERS, cf_opts),
            ],
            false,
        )
        .or_else(|_| {
            let secondary_path = PathBuf::from(format!("{}_secondary", db_path));
            DB::open_cf_as_secondary(
                &opts,
                &rocks_path,
                &secondary_path,
                &[CF_ACCOUNTS, CF_TRANSACTIONS, CF_NULLIFIERS],
            )
            .context("Failed to open secondary DB")
        })?;

        // Load accounts
        let mut accounts = Vec::new();
        if let Some(cf) = db.cf_handle(CF_ACCOUNTS) {
            for entry in db.iterator_cf(&cf, IteratorMode::Start) {
                let (key_bytes, value_bytes) = entry?;
                if key_bytes.len() == 32 {
                    let account_hex = hex::encode(&key_bytes);
                    if let Ok(state) = wincode::deserialize::<AccountState>(&value_bytes) {
                        accounts.push((account_hex, state));
                    }
                }
            }
        }
        accounts.sort_by(|a, b| b.1.balance.cmp(&a.1.balance));
        self.accounts = accounts;

        // Load transactions
        let mut transactions = Vec::new();
        if let Some(cf) = db.cf_handle(CF_TRANSACTIONS) {
            for entry in db.iterator_cf(&cf, IteratorMode::Start) {
                let (key_bytes, value_bytes) = entry?;
                let tx_id = hex::encode(&key_bytes);
                if let Ok(tx) = wincode::deserialize::<Transaction>(&value_bytes) {
                    transactions.push((tx_id, tx));
                }
            }
        }
        transactions.reverse();
        self.transactions = transactions;

        // Load nullifiers
        let mut nullifiers = Vec::new();
        if let Some(cf) = db.cf_handle(CF_NULLIFIERS) {
            for entry in db.iterator_cf(&cf, IteratorMode::Start) {
                let (key_bytes, _) = entry?;
                nullifiers.push(hex::encode(&key_bytes));
            }
        }
        nullifiers.reverse();
        self.nullifiers = nullifiers;

        // Reapply search if active
        if !self.search_query.is_empty() {
            self.apply_search();
        }

        Ok(())
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(f.area());

    // Header
    let header_text = if app.mode == Mode::Search {
        format!("RocksDB Inspector - Search: {}_", app.search_query)
    } else {
        "RocksDB Inspector".to_string()
    };
    let header = Paragraph::new(header_text)
        .style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(header, chunks[0]);

    // Main content - two columns (left: accounts, right: transactions + nullifiers)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(chunks[1]);

    // Left side: Accounts panel
    render_accounts(f, app, columns[0]);
    
    // Right side: Split into transactions (top) and nullifiers (bottom)
    let right_panels = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(columns[1]);
    
    // Transactions panel (top right)
    render_transactions(f, app, right_panels[0]);
    
    // Nullifiers panel (bottom right)
    render_nullifiers(f, app, right_panels[1]);

    // Footer
    let footer_text = if app.mode == Mode::Search {
        format!("ESC: Exit search | Enter: Apply | {}", app.status_message)
    } else {
        format!("Tab: Switch | ↑↓: Scroll | /: Search | C: Copy | Q: Quit | {}", app.status_message)
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default());
    f.render_widget(footer, chunks[2]);
}

fn render_accounts(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Accounts;
    
    let items_to_show: Vec<(usize, &(String, AccountState))> = if app.search_query.is_empty() {
        app.accounts.iter().enumerate().collect()
    } else {
        app.filtered_accounts
            .iter()
            .map(|&idx| (idx, &app.accounts[idx]))
            .collect()
    };

    let items: Vec<ListItem> = items_to_show
        .iter()
        .enumerate()
        .map(|(display_idx, (_, (id, state)))| {
            let short_id = if id.len() > 40 {
                format!("{}..{}", &id[..20], &id[id.len()-8..])
            } else {
                id.clone()
            };
            
            let content = format!("{:<45} {:>20}", short_id, format_balance_with_separators(state.balance));
            
            let style = if display_idx == app.accounts_scroll && is_active {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let border_style = if is_active {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let title = if app.search_query.is_empty() {
        format!("Accounts [{}]", app.accounts.len())
    } else {
        format!("Accounts [{}/{}]", items_to_show.len(), app.accounts.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        );

    f.render_widget(list, area);
}

fn render_transactions(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Transactions;
    
    let items_to_show: Vec<(usize, &(String, Transaction))> = if app.search_query.is_empty() {
        app.transactions.iter().enumerate().collect()
    } else {
        app.filtered_transactions
            .iter()
            .map(|&idx| (idx, &app.transactions[idx]))
            .collect()
    };

    let items: Vec<ListItem> = items_to_show
        .iter()
        .enumerate()
        .map(|(display_idx, (_, (id, tx)))| {
            let short_id = if id.len() > 28 {
                format!("{}..{}", &id[..14], &id[id.len()-6..])
            } else {
                id.clone()
            };

            let tx_type = match &tx.tx_type {
                zelana_transaction::TransactionType::Transfer(_) => "Transfer ",
                zelana_transaction::TransactionType::Shielded(_) => "Shielded ",
                zelana_transaction::TransactionType::Deposit(_) => "Deposit  ",
                zelana_transaction::TransactionType::Withdraw(_) => "Withdraw ",
            };

            let content = format!("{:<32} {}", short_id, tx_type);
            
            let style = if display_idx == app.transactions_scroll && is_active {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let border_style = if is_active {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let title = if app.search_query.is_empty() {
        format!("Transactions [{}]", app.transactions.len())
    } else {
        format!("Transactions [{}/{}]", items_to_show.len(), app.transactions.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        );

    f.render_widget(list, area);
}

fn render_nullifiers(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Nullifiers;
    
    let items_to_show: Vec<(usize, &String)> = if app.search_query.is_empty() {
        app.nullifiers.iter().enumerate().collect()
    } else {
        app.filtered_nullifiers
            .iter()
            .map(|&idx| (idx, &app.nullifiers[idx]))
            .collect()
    };

    let items: Vec<ListItem> = items_to_show
        .iter()
        .enumerate()
        .map(|(display_idx, (_, nullifier))| {
            let short = if nullifier.len() > 48 {
                format!("{}..{}", &nullifier[..24], &nullifier[nullifier.len()-12..])
            } else {
                nullifier.to_string()
            };

            let style = if display_idx == app.nullifiers_scroll && is_active {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            ListItem::new(short).style(style)
        })
        .collect();

    let border_style = if is_active {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let title = if app.search_query.is_empty() {
        format!("Nullifiers [{}]", app.nullifiers.len())
    } else {
        format!("Nullifiers [{}/{}]", items_to_show.len(), app.nullifiers.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        );

    f.render_widget(list, area);
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let db_path = env::var("DB_PATH").unwrap_or_else(|_| "./data/sequencer_db".to_string());

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.load_data(&db_path)?;

    let res = run_app(&mut terminal, &mut app, &db_path).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    db_path: &str,
) -> Result<()> 
where
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut last_update = tokio::time::Instant::now();

    loop {
        terminal.draw(|f| ui(f, app)).map_err(|e| anyhow::anyhow!("Draw error: {}", e))?;

        // Refresh data every 500ms
        if last_update.elapsed() >= Duration::from_millis(500) {
            app.load_data(db_path)?;
            last_update = tokio::time::Instant::now();
        }

        // Handle input with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if app.mode == Mode::Search {
                    match key.code {
                        KeyCode::Esc => {
                            app.mode = Mode::Normal;
                            app.search_query.clear();
                            app.apply_search();
                            app.status_message = "Search cancelled".to_string();
                        }
                        KeyCode::Enter => {
                            app.mode = Mode::Normal;
                            app.apply_search();
                            app.status_message = format!("Search applied: {}", app.search_query);
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.apply_search();
                        }
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            app.apply_search();
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(()),
                        KeyCode::Char('/') => {
                            app.mode = Mode::Search;
                            app.search_query.clear();
                            app.status_message.clear();
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            app.copy_current_item();
                        }
                        KeyCode::Tab => app.next_panel(),
                        KeyCode::BackTab => app.previous_panel(),
                        KeyCode::Up => app.scroll_up(),
                        KeyCode::Down => app.scroll_down(),
                        KeyCode::PageUp => {
                            for _ in 0..10 {
                                app.scroll_up();
                            }
                        }
                        KeyCode::PageDown => {
                            for _ in 0..10 {
                                app.scroll_down();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Clear status message after 3 seconds
        if !app.status_message.is_empty() && last_update.elapsed() >= Duration::from_secs(3) {
            app.status_message.clear();
        }
    }
}