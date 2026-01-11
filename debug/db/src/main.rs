use std::env;
use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod db;
mod events;
mod theme;
mod ui;

use app::App;
use events::handle_events;
use ui::render_ui;

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
        terminal
            .draw(|f| render_ui(f, app))
            .map_err(|e| anyhow::anyhow!("Draw error: {}", e))?;

        // Refresh data every 500ms
        if last_update.elapsed() >= Duration::from_millis(500) {
            app.load_data(db_path)?;
            last_update = tokio::time::Instant::now();
        }

        // Handle input with timeout
        if handle_events(app)? {
            return Ok(()); // Quit signal
        }

        // Clear status message after 3 seconds
        if !app.status_message.is_empty() && last_update.elapsed() >= Duration::from_secs(3) {
            app.status_message.clear();
        }
    }
}
