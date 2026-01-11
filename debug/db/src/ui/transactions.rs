use ratatui::{
    Frame,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::{App, Panel};
use crate::theme::*;

pub fn render_transactions(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Transactions;

    let items_to_show: Vec<(usize, &(String, _))> = if app.search_query.is_empty() {
        app.transactions.iter().enumerate().collect()
    } else {
        app.filtered_transactions
            .iter()
            .map(|&idx| (idx, &app.transactions[idx]))
            .collect()
    };

    // Calculate available width dynamically
    let total_width = area.width.saturating_sub(2) as usize; // Remove borders
    let tx_type_width = 10; // Fixed width for transaction type

    // Apply offset for scrolling
    let visible_items: Vec<_> = items_to_show
        .iter()
        .skip(app.transactions_offset)
        .take((area.height.saturating_sub(3)) as usize)
        .collect();

    let items: Vec<ListItem> = visible_items
        .iter()
        .enumerate()
        .map(|(visible_idx, (_original_idx, (id, tx)))| {
            let (tx_type, tx_color) = match &tx.tx_type {
                zelana_transaction::TransactionType::Transfer(_) => ("Transfer ", COLOR_INFO),
                zelana_transaction::TransactionType::Shielded(_) => ("Shielded ", COLOR_ACCENT),
                zelana_transaction::TransactionType::Deposit(_) => ("Deposit  ", COLOR_SUCCESS),
                zelana_transaction::TransactionType::Withdraw(_) => ("Withdraw ", COLOR_WARNING),
            };

            // Calculate ID width
            let id_width = if total_width > tx_type_width + 10 {
                total_width - tx_type_width - 4 // Extra spacing
            } else {
                total_width.saturating_sub(tx_type_width).max(10)
            };

            let formatted_id = if id.len() <= id_width {
                id.clone()
            } else if id_width > 10 {
                let half = (id_width - 2) / 2;
                format!(
                    "{}..{}",
                    &id[..half],
                    &id[id.len() - (id_width - half - 2)..]
                )
            } else {
                id.chars()
                    .take(id_width.saturating_sub(2))
                    .collect::<String>()
                    + ".."
            };

            let absolute_idx = visible_idx + app.transactions_offset;
            let content = format!(
                " {} {:width$}  {}",
                if absolute_idx == app.transactions_scroll && is_active {
                    "▸"
                } else {
                    " "
                },
                formatted_id,
                tx_type,
                width = id_width
            );

            // Highlight based on absolute scroll position
            let style = if absolute_idx == app.transactions_scroll && is_active {
                Style::default()
                    .fg(COLOR_BG)
                    .bg(COLOR_PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(tx_color).bg(COLOR_BG)
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let border_color = if is_active {
        COLOR_PRIMARY
    } else {
        COLOR_BORDER
    };

    let title = if app.search_query.is_empty() {
        format!(" Transactions ({}) ", app.transactions.len())
    } else {
        format!(
            " Transactions ({}/{}) ",
            items_to_show.len(),
            app.transactions.len()
        )
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border_color).bg(COLOR_BG))
                .style(Style::default().bg(COLOR_BG)),
        )
        .style(Style::default().bg(COLOR_BG));

    f.render_widget(list, area);
}
