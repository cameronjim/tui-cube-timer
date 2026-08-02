//! Test scaffolding shared by the three files of `app`.
//!
//! The helpers are short on purpose: they appear dozens of times per file.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::{App, InputMode, TimerState};
use crate::storage;
use crate::types::{Penalty, SaveFile, Solve};

pub(super) const SPACE: KeyCode = KeyCode::Char(' ');

/// A unique self-deleting temp path, so tests never touch the real save file.
pub(super) struct TempPath {
    pub(super) path: PathBuf,
}

impl TempPath {
    pub(super) fn new(tag: &str) -> TempPath {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "cubetimer-apptest-{}-{}-{}-{}.json",
            tag,
            std::process::id(),
            n,
            storage::now_millis()
        );
        TempPath {
            path: std::env::temp_dir().join(name),
        }
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let _ = fs::remove_file(PathBuf::from(tmp));
    }
}

/// An app writing to a temp path; keep the guard alive for the whole test.
pub(super) fn test_app(tag: &str) -> (App, TempPath) {
    test_app_with(tag, SaveFile::default())
}

pub(super) fn test_app_with(tag: &str, save: SaveFile) -> (App, TempPath) {
    let guard = TempPath::new(tag);
    let app = App::new(save, guard.path.clone());
    (app, guard)
}

pub(super) fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(super) fn release(code: KeyCode) -> KeyEvent {
    KeyEvent {
        kind: KeyEventKind::Release,
        ..KeyEvent::new(code, KeyModifiers::NONE)
    }
}

pub(super) fn repeat(code: KeyCode) -> KeyEvent {
    KeyEvent {
        kind: KeyEventKind::Repeat,
        ..KeyEvent::new(code, KeyModifiers::NONE)
    }
}

/// An `Instant` in the past: how tests simulate elapsed time without sleeping.
pub(super) fn ago(d: Duration) -> Instant {
    Instant::now()
        .checked_sub(d)
        .expect("monotonic clock is too young to backdate this test")
}

pub(super) fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Type `/<body>` and press Enter, exactly as a user would.
pub(super) fn run_command(app: &mut App, body: &str) {
    app.on_key(press(KeyCode::Char('/')));
    assert_eq!(app.input_mode, InputMode::Command, "'/' must enter command mode");
    for c in body.chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Enter));
    assert_eq!(app.input_mode, InputMode::Normal, "Enter must leave command mode");
}

pub(super) fn add_solve(app: &mut App, millis: u64) {
    add_solve_with(app, millis, "R U R' U'");
}

/// Seed a solve carrying a chosen scramble, so a test can tell recalled ones apart.
pub(super) fn add_solve_with(app: &mut App, millis: u64, scramble: &str) {
    app.current_session_mut().solves.push(Solve {
        millis,
        penalty: Penalty::None,
        scramble: scramble.to_string(),
        timestamp: 0,
    });
}

/// Drive Idle -> Timing, backdating the arm so the release counts as held.
pub(super) fn start_timing_now(app: &mut App) {
    app.save.settings.inspection = false;
    app.on_key(press(SPACE));
    match app.state {
        TimerState::Armed { from_inspection, .. } => {
            app.state = TimerState::Armed {
                since: ago(ms(350)),
                from_inspection,
            };
        }
        other => panic!("expected Armed, got {:?}", other),
    }
    app.on_key(release(SPACE));
    assert!(matches!(app.state, TimerState::Timing { .. }));
}
