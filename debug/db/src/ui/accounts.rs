use ratatui::{
    Frame,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::{App, Panel, format_balance_with_separators};
use crate::theme::*;

pub fn render_accounts(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Accounts;

    let items_to_show: Vec<(usize, &(String, _))> = if app.search_query.is_empty() {
        app.accounts.iter().enumerate().collect()
    } else {
        app.filtered_accounts
            .iter()
            .map(|&idx| (idx, &app.accounts[idx]))
            .collect()
    };

    // Calculate available width dynamically
    let total_width = area.width.saturating_sub(2) as usize; // Remove borders

    // Apply offset for scrolling
    let visible_items: Vec<_> = items_to_show
        .iter()
        .skip(app.accounts_offset)
        .take((area.height.saturating_sub(3)) as usize) // visible lines
        .collect();

    let items: Vec<ListItem> = visible_items
        .iter()
        .enumerate()
        .map(|(visible_idx, (_original_idx, (id, state)))| {
            let balance_str = format_balance_with_separators(state.balance);
            let balance_len = balance_str.len() + 3; // Add spacing

            // Calculate how much space we have for the ID
            let id_width = if total_width > balance_len + 10 {
                total_width - balance_len - 2 // Extra spacing
            } else {
                total_width.saturating_sub(balance_len).max(10)
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

            // Add extra spacing for better visual separation
            let content = format!(
                " {} {:width$}  {}",
                if visible_idx + app.accounts_offset == app.accounts_scroll && is_active {
                    "▸"
                } else {
                    " "
                },
                formatted_id,
                balance_str,
                width = id_width
            );

            // Highlight based on absolute scroll position
            let absolute_idx = visible_idx + app.accounts_offset;
            let style = if absolute_idx == app.accounts_scroll && is_active {
                Style::default()
                    .fg(COLOR_BG)
                    .bg(COLOR_PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_TEXT).bg(COLOR_BG)
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
        format!(" Accounts ({}) ", app.accounts.len())
    } else {
        format!(
            " Accounts ({}/{}) ",
            items_to_show.len(),
            app.accounts.len()
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
