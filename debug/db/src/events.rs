use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};

use crate::app::{App, Mode};

pub fn handle_events(app: &mut App) -> Result<bool> {
    if event::poll(std::time::Duration::from_millis(100))? {
        match event::read()? {
            Event::Key(key) => {
                if app.mode == Mode::Search {
                    handle_search_mode(app, key.code);
                } else {
                    if handle_normal_mode(app, key.code)? {
                        return Ok(true); // Quit signal
                    }
                }
            }
            Event::Mouse(mouse_event) => {
                if let event::MouseEventKind::Down(event::MouseButton::Left) = mouse_event.kind {
                    app.handle_mouse_click(mouse_event.column, mouse_event.row);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

fn handle_search_mode(app: &mut App, key_code: KeyCode) {
    match key_code {
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
}

fn handle_normal_mode(app: &mut App, key_code: KeyCode) -> Result<bool> {
    match key_code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
            app.search_query.clear();
            app.status_message.clear();
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.copy_current_item();
        }
        // Vim-style navigation
        KeyCode::Char('h') | KeyCode::Char('H') => app.previous_panel(),
        KeyCode::Char('l') | KeyCode::Char('L') => app.next_panel(),
        KeyCode::Char('k') | KeyCode::Char('K') => app.scroll_up(),
        KeyCode::Char('j') | KeyCode::Char('J') => app.scroll_down(),
        // Also keep traditional navigation
        KeyCode::Tab => app.next_panel(),
        KeyCode::BackTab => app.previous_panel(),
        KeyCode::Up => app.scroll_up(),
        KeyCode::Down => app.scroll_down(),
        KeyCode::Left => app.previous_panel(),
        KeyCode::Right => app.next_panel(),
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
    Ok(false)
}
