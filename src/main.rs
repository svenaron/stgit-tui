mod app;
mod keys;
#[allow(dead_code)]
mod stgit;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, prelude::*};
use std::io;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = app::App::new()?;

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if let Event::Key(key) = event::read()? {
            // Handle edit specially — need to leave TUI for $EDITOR
            if key.code == KeyCode::Char('e') && key.modifiers == KeyModifiers::NONE {
                if let Some(i) = app.current_patch_index() {
                    let name = app.state.patches[i].name.clone();
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    terminal.show_cursor()?;

                    let result = stgit::stg_edit(&name);

                    enable_raw_mode()?;
                    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                    terminal.hide_cursor()?;
                    terminal.clear()?;

                    app.run_op(result);
                    continue;
                }
            }

            app.handle_key(key);
            if app.should_quit {
                break;
            }
        }
    }

    Ok(())
}
