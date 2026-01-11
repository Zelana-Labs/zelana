use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, Mode};
use crate::theme::*;
use crate::ui::{
    accounts::render_accounts, nullifiers::render_nullifiers, transactions::render_transactions,
};

pub fn render_ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Minimalist header - clean and clear
    let header_text = if app.mode == Mode::Search {
        format!("  RocksDB Inspector  │  Search: {}_", app.search_query)
    } else {
        "  RocksDB Inspector  │  Database Viewer".to_string()
    };

    let header = Paragraph::new(header_text)
        .style(
            Style::default()
                .fg(COLOR_PRIMARY)
                .bg(COLOR_BG)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(COLOR_BORDER).bg(COLOR_BG)),
        );
    f.render_widget(header, chunks[0]);

    // Main content - two columns (left: accounts, right: transactions + nullifiers)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Left side: Accounts panel
    app.accounts_area = Some(columns[0]);
    render_accounts(f, app, columns[0]);

    // Right side: Split into transactions (top) and nullifiers (bottom)
    let right_panels = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(columns[1]);

    // Transactions panel (top right)
    app.transactions_area = Some(right_panels[0]);
    render_transactions(f, app, right_panels[0]);

    // Nullifiers panel (bottom right)
    app.nullifiers_area = Some(right_panels[1]);
    render_nullifiers(f, app, right_panels[1]);

    // Clear, organized footer with instructions
    let status_width = if !app.status_message.is_empty() {
        (app.status_message.len() + 4).min(40)
    } else {
        15
    };

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(status_width as u16)])
        .split(chunks[2]);

    let footer_text = if app.mode == Mode::Search {
        "  [ESC] Exit  │  [ENTER] Apply Search".to_string()
    } else {
        "  [hjkl / ↑↓←→] Navigate  │  [/] Search  │  [c] Copy  │  [q] Quit".to_string()
    };

    let footer_left = Paragraph::new(footer_text)
        .style(Style::default().fg(COLOR_TEXT).bg(COLOR_BG))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(COLOR_BORDER).bg(COLOR_BG)),
        );
    f.render_widget(footer_left, footer_chunks[0]);

    // Status message on the right - clean and simple
    let status_text = if !app.status_message.is_empty() {
        format!(" {} ", app.status_message)
    } else {
        " Ready ".to_string()
    };
    let footer_right = Paragraph::new(status_text)
        .style(
            Style::default()
                .fg(if !app.status_message.is_empty() {
                    COLOR_SUCCESS
                } else {
                    COLOR_TEXT
                })
                .bg(COLOR_BG),
        )
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(COLOR_BORDER).bg(COLOR_BG)),
        );
    f.render_widget(footer_right, footer_chunks[1]);
}
