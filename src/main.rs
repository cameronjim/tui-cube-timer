//! cubetimer, a speedcube timer TUI.

mod app;
mod cstimer;
mod cube;
mod scramble;
mod stats;
mod storage;
mod types;
mod ui;

use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{
    self, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::terminal::supports_keyboard_enhancement;
use ratatui::crossterm::ExecutableCommand;
use ratatui::DefaultTerminal;

use crate::app::App;

/// Poll interval for the event loop.
const TICK: Duration = Duration::from_millis(15);

fn main() {
    let path = storage::data_file_path();

    // Bail out before the TUI starts so a corrupt save file is never overwritten.
    let save = match storage::load(&path) {
        Ok(save) => save,
        Err(e) => {
            eprintln!("cubetimer: could not read your save file.");
            eprintln!("  path:  {}", path.display());
            eprintln!("  error: {}", e);
            eprintln!();
            eprintln!(
                "Refusing to start so your data is not overwritten. Fix, move or \
                 delete that file and run cubetimer again."
            );
            std::process::exit(1);
        }
    };

    let mut app = App::new(save, path);

    let mut terminal = ratatui::init();

    // Ask for key release events where the kitty protocol is supported (Windows reports them natively).
    let enhanced = matches!(supports_keyboard_enhancement(), Ok(true))
        && io::stdout()
            .execute(PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::REPORT_EVENT_TYPES,
            ))
            .is_ok();

    let result = run(&mut terminal, &mut app);

    if enhanced {
        let _ = io::stdout().execute(PopKeyboardEnhancementFlags);
    }
    ratatui::restore();

    // Final save on quit (also reports failures the TUI could no longer show).
    if let Err(e) = storage::save(&app.data_path, &app.save) {
        eprintln!("cubetimer: final save failed: {}", e);
    }

    if let Err(e) = result {
        eprintln!("cubetimer: {}", e);
        std::process::exit(1);
    }
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.should_quit {
        app.on_tick();
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                // Auto-repeat is not a real press/release: ignore it.
                if key.kind != KeyEventKind::Repeat {
                    app.on_key(key);
                }
            }
        }
    }
    Ok(())
}
