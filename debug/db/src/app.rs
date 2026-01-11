use anyhow::Result;
use zelana_account::AccountState;
use zelana_transaction::Transaction;

use crate::db;

#[derive(Clone, Copy, PartialEq)]
pub enum Panel {
    Accounts,
    Transactions,
    Nullifiers,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Normal,
    Search,
}

pub struct App {
    pub active_panel: Panel,
    pub mode: Mode,
    pub accounts: Vec<(String, AccountState)>,
    pub transactions: Vec<(String, Transaction)>,
    pub nullifiers: Vec<String>,
    pub filtered_accounts: Vec<usize>,
    pub filtered_transactions: Vec<usize>,
    pub filtered_nullifiers: Vec<usize>,
    pub accounts_scroll: usize,
    pub transactions_scroll: usize,
    pub nullifiers_scroll: usize,
    pub accounts_offset: usize,
    pub transactions_offset: usize,
    pub nullifiers_offset: usize,
    pub search_query: String,
    pub status_message: String,
    // Store panel areas for mouse click detection
    pub accounts_area: Option<ratatui::layout::Rect>,
    pub transactions_area: Option<ratatui::layout::Rect>,
    pub nullifiers_area: Option<ratatui::layout::Rect>,
}

pub fn format_balance_with_separators(balance: u64) -> String {
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
    pub fn new() -> Self {
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
            accounts_offset: 0,
            transactions_offset: 0,
            nullifiers_offset: 0,
            search_query: String::new(),
            status_message: String::new(),
            accounts_area: None,
            transactions_area: None,
            nullifiers_area: None,
        }
    }

    pub fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Accounts => Panel::Transactions,
            Panel::Transactions => Panel::Nullifiers,
            Panel::Nullifiers => Panel::Accounts,
        };
        self.reset_scroll();
    }

    pub fn previous_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Accounts => Panel::Nullifiers,
            Panel::Transactions => Panel::Accounts,
            Panel::Nullifiers => Panel::Transactions,
        };
        self.reset_scroll();
    }

    pub fn reset_scroll(&mut self) {
        match self.active_panel {
            Panel::Accounts => {
                self.accounts_scroll = 0;
                self.accounts_offset = 0;
            }
            Panel::Transactions => {
                self.transactions_scroll = 0;
                self.transactions_offset = 0;
            }
            Panel::Nullifiers => {
                self.nullifiers_scroll = 0;
                self.nullifiers_offset = 0;
            }
        }
    }

    pub fn scroll_up(&mut self) {
        match self.active_panel {
            Panel::Accounts => {
                if self.accounts_scroll > 0 {
                    self.accounts_scroll -= 1;
                    if self.accounts_scroll < self.accounts_offset {
                        self.accounts_offset = self.accounts_scroll;
                    }
                }
            }
            Panel::Transactions => {
                if self.transactions_scroll > 0 {
                    self.transactions_scroll -= 1;
                    if self.transactions_scroll < self.transactions_offset {
                        self.transactions_offset = self.transactions_scroll;
                    }
                }
            }
            Panel::Nullifiers => {
                if self.nullifiers_scroll > 0 {
                    self.nullifiers_scroll -= 1;
                    if self.nullifiers_scroll < self.nullifiers_offset {
                        self.nullifiers_offset = self.nullifiers_scroll;
                    }
                }
            }
        }
    }

    pub fn scroll_down(&mut self) {
        match self.active_panel {
            Panel::Accounts => {
                let max = if self.search_query.is_empty() {
                    self.accounts.len()
                } else {
                    self.filtered_accounts.len()
                };
                if self.accounts_scroll < max.saturating_sub(1) {
                    self.accounts_scroll += 1;
                    // Adjust offset based on visible height (will be set during render)
                    if let Some(area) = self.accounts_area {
                        let visible_height = area.height.saturating_sub(3) as usize; // borders + title
                        if self.accounts_scroll >= self.accounts_offset + visible_height {
                            self.accounts_offset = self.accounts_scroll - visible_height + 1;
                        }
                    }
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
                    if let Some(area) = self.transactions_area {
                        let visible_height = area.height.saturating_sub(3) as usize;
                        if self.transactions_scroll >= self.transactions_offset + visible_height {
                            self.transactions_offset =
                                self.transactions_scroll - visible_height + 1;
                        }
                    }
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
                    if let Some(area) = self.nullifiers_area {
                        let visible_height = area.height.saturating_sub(3) as usize;
                        if self.nullifiers_scroll >= self.nullifiers_offset + visible_height {
                            self.nullifiers_offset = self.nullifiers_scroll - visible_height + 1;
                        }
                    }
                }
            }
        }
    }

    pub fn apply_search(&mut self) {
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

    pub fn copy_current_item(&mut self) {
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

                self.status_message = format!(
                    "Install xclip or xsel. Text: {}",
                    &text[..text.len().min(20)]
                );
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

    pub fn handle_mouse_click(&mut self, x: u16, y: u16) {
        // Check if click is in accounts area
        if let Some(area) = self.accounts_area {
            if x > area.x
                && x < area.x + area.width - 1
                && y > area.y + 1
                && y < area.y + area.height - 1
            {
                self.active_panel = Panel::Accounts;
                // Calculate which item was clicked
                // y - (area.y + 2) gives us the row index in the visible area
                let clicked_row = (y.saturating_sub(area.y + 2)) as usize;
                let actual_index = clicked_row + self.accounts_offset;
                let max_items = if self.search_query.is_empty() {
                    self.accounts.len()
                } else {
                    self.filtered_accounts.len()
                };
                if actual_index < max_items {
                    self.accounts_scroll = actual_index;
                }
                return;
            }
        }

        // Check if click is in transactions area
        if let Some(area) = self.transactions_area {
            if x > area.x
                && x < area.x + area.width - 1
                && y > area.y + 1
                && y < area.y + area.height - 1
            {
                self.active_panel = Panel::Transactions;
                let clicked_row = (y.saturating_sub(area.y + 2)) as usize;
                let actual_index = clicked_row + self.transactions_offset;
                let max_items = if self.search_query.is_empty() {
                    self.transactions.len()
                } else {
                    self.filtered_transactions.len()
                };
                if actual_index < max_items {
                    self.transactions_scroll = actual_index;
                }
                return;
            }
        }

        // Check if click is in nullifiers area
        if let Some(area) = self.nullifiers_area {
            if x > area.x
                && x < area.x + area.width - 1
                && y > area.y + 1
                && y < area.y + area.height - 1
            {
                self.active_panel = Panel::Nullifiers;
                let clicked_row = (y.saturating_sub(area.y + 2)) as usize;
                let actual_index = clicked_row + self.nullifiers_offset;
                let max_items = if self.search_query.is_empty() {
                    self.nullifiers.len()
                } else {
                    self.filtered_nullifiers.len()
                };
                if actual_index < max_items {
                    self.nullifiers_scroll = actual_index;
                }
                return;
            }
        }
    }

    pub fn load_data(&mut self, db_path: &str) -> Result<()> {
        let (accounts, transactions, nullifiers) = db::load_database(db_path)?;

        self.accounts = accounts;
        self.transactions = transactions;
        self.nullifiers = nullifiers;

        // Reapply search if active
        if !self.search_query.is_empty() {
            self.apply_search();
        }

        Ok(())
    }
}
