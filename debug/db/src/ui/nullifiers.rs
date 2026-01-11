use ratatui::{
    Frame,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::{App, Panel};
use crate::theme::*;

pub fn render_nullifiers(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Nullifiers;

    let items_to_show: Vec<(usize, &String)> = if app.search_query.is_empty() {
        app.nullifiers.iter().enumerate().collect()
    } else {
        app.filtered_nullifiers
            .iter()
            .map(|&idx| (idx, &app.nullifiers[idx]))
            .collect()
    };

    // Calculate available width dynamically
    let total_width = area.width.saturating_sub(2) as usize; // Remove borders

    // Apply offset for scrolling
    let visible_items: Vec<_> = items_to_show
        .iter()
        .skip(app.nullifiers_offset)
        .take((area.height.saturating_sub(3)) as usize)
        .collect();

    let items: Vec<ListItem> = visible_items
        .iter()
        .enumerate()
        .map(|(visible_idx, (_original_idx, nullifier))| {
            let available_width = total_width.saturating_sub(4); // Space for indicator and padding

            let formatted_nullifier = if nullifier.len() <= available_width {
                nullifier.to_string()
            } else if available_width > 10 {
                let half = (available_width - 2) / 2;
                format!(
                    "{}..{}",
                    &nullifier[..half],
                    &nullifier[nullifier.len() - (available_width - half - 2)..]
                )
            } else {
                nullifier
                    .chars()
                    .take(available_width.saturating_sub(2))
                    .collect::<String>()
                    + ".."
            };

            let absolute_idx = visible_idx + app.nullifiers_offset;
            let content = format!(
                " {} {}",
                if absolute_idx == app.nullifiers_scroll && is_active {
                    "▸"
                } else {
                    " "
                },
                formatted_nullifier
            );

            // Highlight based on absolute scroll position
            let style = if absolute_idx == app.nullifiers_scroll && is_active {
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
        format!(" Nullifiers ({}) ", app.nullifiers.len())
    } else {
        format!(
            " Nullifiers ({}/{}) ",
            items_to_show.len(),
            app.nullifiers.len()
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
