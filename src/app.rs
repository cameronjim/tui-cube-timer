//! Timer state machine, key handling and `/commands`; `ui.rs` renders only from [`App`] fields refreshed here.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::scramble;
use crate::storage;
use crate::types::{Penalty, Puzzle, SaveFile, Session, Solve};

/// How long space must be held before releasing it starts the timer.
const ARM_THRESHOLD: Duration = Duration::from_millis(300);
/// After a solve, space is ignored this long (csTimer-style guard against bounced keys).
const STOP_COOLDOWN: Duration = Duration::from_millis(300);
/// Inspection length in seconds.
const INSPECTION_SECS: i64 = 15;
/// Elapsed inspection past this (ms) earns a +2.
const INSPECTION_PLUS2_MS: u128 = 15_000;
/// Elapsed inspection past this (ms) earns a DNF.
const INSPECTION_DNF_MS: u128 = 17_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Idle,
    Inspecting { started: Instant },
    Armed { since: Instant, from_inspection: bool },
    Timing { started: Instant },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Command,
}

pub struct App {
    pub save: SaveFile,
    pub state: TimerState,
    pub input_mode: InputMode,
    /// Command line being typed, including the leading `'/'`.
    pub command_buf: String,
    pub scramble: String,
    pub inspection_enabled: bool,
    pub status_msg: Option<String>,
    pub show_help: bool,
    /// 0 = newest solve visible at the top of the times list.
    pub times_scroll: usize,
    pub should_quit: bool,

    // Derived-for-rendering fields, refreshed by on_tick()/state changes.
    pub display_millis: u64,
    pub inspection_remaining: Option<i64>,
    pub pending_inspection_penalty: Penalty,
    pub data_path: PathBuf,

    // --- internal bookkeeping (not part of the ui.rs contract) ---
    /// Inspection start, kept while armed so an aborted arm restores the countdown.
    inspection_start: Option<Instant>,
    /// Key that stopped the timer: inert until its Release (Windows auto-repeat resends Press, not Repeat).
    inert_key: Option<KeyCode>,
    /// When the last solve was finalized; space is ignored for `STOP_COOLDOWN` after it.
    stopped_at: Option<Instant>,
}

impl App {
    pub fn new(save: SaveFile, data_path: PathBuf) -> App {
        let mut save = save;
        Self::sanitize(&mut save);
        let puzzle = save
            .sessions
            .iter()
            .find(|s| s.id == save.active_session_id)
            .map(|s| s.puzzle)
            .unwrap_or(Puzzle::Cube3);

        App {
            save,
            state: TimerState::Idle,
            input_mode: InputMode::Normal,
            command_buf: String::new(),
            scramble: scramble::generate(puzzle),
            inspection_enabled: false,
            status_msg: None,
            show_help: false,
            times_scroll: 0,
            should_quit: false,
            display_millis: 0,
            inspection_remaining: None,
            pending_inspection_penalty: Penalty::None,
            data_path,
            inspection_start: None,
            inert_key: None,
            stopped_at: None,
        }
    }

    /// Restore invariants: a session exists, the active id is real, `next_session_id` is free.
    fn sanitize(save: &mut SaveFile) {
        if save.sessions.is_empty() {
            save.sessions.push(Session {
                id: 1,
                name: "default".to_string(),
                puzzle: Puzzle::Cube3,
                solves: Vec::new(),
                created_at: storage::now_millis(),
            });
            save.active_session_id = 1;
        }
        let max_id = save.sessions.iter().map(|s| s.id).max().unwrap_or(0);
        if save.next_session_id <= max_id {
            save.next_session_id = max_id + 1;
        }
        if !save.sessions.iter().any(|s| s.id == save.active_session_id) {
            save.active_session_id = save.sessions[0].id;
        }
    }

    fn active_index(&self) -> usize {
        self.save
            .sessions
            .iter()
            .position(|s| s.id == self.save.active_session_id)
            .unwrap_or(0)
    }

    pub fn current_session(&self) -> &Session {
        let i = self.active_index();
        &self.save.sessions[i]
    }

    pub fn current_session_mut(&mut self) -> &mut Session {
        let i = self.active_index();
        &mut self.save.sessions[i]
    }

    /// True when Armed long enough (>= 300ms) that releasing space starts the timer.
    pub fn armed_ready(&self) -> bool {
        match self.state {
            TimerState::Armed { since, .. } => since.elapsed() >= ARM_THRESHOLD,
            _ => false,
        }
    }

    // ---------------------------------------------------------------- helpers

    fn puzzle(&self) -> Puzzle {
        self.current_session().puzzle
    }

    fn new_scramble(&mut self) {
        self.scramble = scramble::generate(self.puzzle());
        self.times_scroll = 0;
    }

    /// Persist to disk. Never panics; failures surface in `status_msg`.
    fn save_now(&mut self) {
        if let Err(e) = storage::save(&self.data_path, &self.save) {
            self.status_msg = Some(format!("save failed: {}", e));
        }
    }

    fn status<S: Into<String>>(&mut self, msg: S) {
        self.status_msg = Some(msg.into());
    }

    fn start_inspection(&mut self) {
        let now = Instant::now();
        self.state = TimerState::Inspecting { started: now };
        self.inspection_start = Some(now);
        self.pending_inspection_penalty = Penalty::None;
        self.inspection_remaining = Some(INSPECTION_SECS);
        self.display_millis = 0;
        self.status_msg = None;
    }

    fn cancel_inspection(&mut self) {
        self.state = TimerState::Idle;
        self.inspection_start = None;
        self.inspection_remaining = None;
        self.pending_inspection_penalty = Penalty::None;
        self.display_millis = 0;
    }

    fn arm(&mut self, from_inspection: bool) {
        self.state = TimerState::Armed {
            since: Instant::now(),
            from_inspection,
        };
        self.display_millis = 0;
    }

    /// Leave `Armed` without starting the timer, returning to wherever we came from.
    fn unarm(&mut self, from_inspection: bool) {
        if from_inspection {
            let started = self.inspection_start.unwrap_or_else(Instant::now);
            self.inspection_start = Some(started);
            self.state = TimerState::Inspecting { started };
        } else {
            self.state = TimerState::Idle;
            self.inspection_remaining = None;
            self.display_millis = 0;
        }
    }

    fn start_timing(&mut self) {
        self.state = TimerState::Timing {
            started: Instant::now(),
        };
        self.inspection_start = None;
        self.inspection_remaining = None;
        self.display_millis = 0;
        self.status_msg = None;
    }

    /// Record the running solve, persist, and return to Idle with a fresh scramble.
    fn finish_solve(&mut self, started: Instant) {
        let millis = started.elapsed().as_millis() as u64;
        let solve = Solve {
            millis,
            penalty: self.pending_inspection_penalty,
            scramble: self.scramble.clone(),
            timestamp: storage::now_millis(),
        };
        self.current_session_mut().solves.push(solve);

        self.pending_inspection_penalty = Penalty::None;
        self.inspection_start = None;
        self.inspection_remaining = None;
        self.display_millis = millis;
        self.state = TimerState::Idle;
        self.stopped_at = Some(Instant::now());
        self.new_scramble();
        self.save_now();
    }

    /// True while the post-solve cooldown is still swallowing space events.
    fn in_stop_cooldown(&self) -> bool {
        self.stopped_at
            .is_some_and(|when| when.elapsed() < STOP_COOLDOWN)
    }

    // ------------------------------------------------------------------- tick

    pub fn on_tick(&mut self) {
        match self.state {
            TimerState::Idle => {}
            TimerState::Inspecting { started } => {
                self.refresh_inspection(started);
            }
            TimerState::Armed {
                from_inspection, ..
            } => {
                if from_inspection {
                    if let Some(started) = self.inspection_start {
                        self.refresh_inspection(started);
                    }
                }
            }
            TimerState::Timing { started } => {
                self.display_millis = started.elapsed().as_millis() as u64;
            }
        }
    }

    fn refresh_inspection(&mut self, started: Instant) {
        let elapsed = started.elapsed().as_millis();
        self.inspection_remaining = Some(INSPECTION_SECS - (elapsed / 1000) as i64);
        self.pending_inspection_penalty = if elapsed > INSPECTION_DNF_MS {
            Penalty::Dnf
        } else if elapsed > INSPECTION_PLUS2_MS {
            Penalty::Plus2
        } else {
            Penalty::None
        };
    }

    // -------------------------------------------------------------------- key

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Repeat {
            return;
        }

        // While timing, any key press stops the timer and that key goes inert.
        if let TimerState::Timing { started } = self.state {
            if key.kind == KeyEventKind::Press {
                self.inert_key = Some(key.code);
                self.finish_solve(started);
            }
            return;
        }

        // Ctrl-C is always an escape hatch (raw mode swallows the usual SIGINT).
        if key.kind == KeyEventKind::Press
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            self.should_quit = true;
            return;
        }

        // Windows auto-repeat keeps sending Press while held, so only Release clears the block.
        if self.inert_key == Some(key.code) {
            if key.kind == KeyEventKind::Release {
                self.inert_key = None;
            }
            return;
        }

        if self.input_mode == InputMode::Command {
            if key.kind == KeyEventKind::Press {
                self.on_command_key(key);
            }
            return;
        }

        // Post-solve cooldown holds off space only; commands, scrolling and `q` keep working.
        if key.code == KeyCode::Char(' ') && self.in_stop_cooldown() {
            return;
        }

        match self.state {
            TimerState::Idle => self.on_key_idle(key),
            TimerState::Inspecting { .. } => self.on_key_inspecting(key),
            TimerState::Armed {
                from_inspection, ..
            } => self.on_key_armed(key, from_inspection),
            TimerState::Timing { .. } => {}
        }
    }

    fn on_key_idle(&mut self, key: KeyEvent) {
        match key.kind {
            KeyEventKind::Press => match key.code {
                KeyCode::Char(' ') if !self.inspection_enabled => self.arm(false),
                KeyCode::Char('/') => {
                    self.input_mode = InputMode::Command;
                    self.command_buf = "/".to_string();
                    self.status_msg = None;
                }
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('n') => {
                    self.new_scramble();
                    self.status_msg = None;
                }
                KeyCode::Char('h') | KeyCode::Char('?') => self.show_help = !self.show_help,
                KeyCode::Esc => {
                    if self.show_help {
                        self.show_help = false;
                    } else {
                        self.status_msg = None;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.times_scroll = self.times_scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = self.current_session().solves.len().saturating_sub(1);
                    self.times_scroll = (self.times_scroll + 1).min(max);
                }
                KeyCode::PageUp => {
                    self.times_scroll = self.times_scroll.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    let max = self.current_session().solves.len().saturating_sub(1);
                    self.times_scroll = (self.times_scroll + 10).min(max);
                }
                KeyCode::Home => self.times_scroll = 0,
                _ => {}
            },
            KeyEventKind::Release => {
                if key.code == KeyCode::Char(' ') && self.inspection_enabled {
                    self.start_inspection();
                }
            }
            KeyEventKind::Repeat => {}
        }
    }

    fn on_key_inspecting(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char(' ') => self.arm(true),
            KeyCode::Esc => self.cancel_inspection(),
            _ => {}
        }
    }

    fn on_key_armed(&mut self, key: KeyEvent, from_inspection: bool) {
        match key.kind {
            KeyEventKind::Release => {
                if key.code == KeyCode::Char(' ') {
                    if self.armed_ready() {
                        self.start_timing();
                    } else {
                        self.unarm(from_inspection);
                    }
                }
            }
            // Safety valve in case the Release event never arrives.
            KeyEventKind::Press => {
                if key.code == KeyCode::Esc {
                    self.unarm(from_inspection);
                }
            }
            KeyEventKind::Repeat => {}
        }
    }

    // ----------------------------------------------------------- command mode

    fn on_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.exit_command_mode(),
            KeyCode::Enter => {
                let buf = std::mem::take(&mut self.command_buf);
                self.input_mode = InputMode::Normal;
                self.execute_command(&buf);
            }
            KeyCode::Backspace => {
                self.command_buf.pop();
                if self.command_buf.is_empty() {
                    self.exit_command_mode();
                }
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::ALT)
                {
                    return;
                }
                if !c.is_control() {
                    self.command_buf.push(c);
                }
            }
            _ => {}
        }
    }

    fn exit_command_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.command_buf.clear();
    }

    /// Execute a command line (with or without its leading `'/'`).
    fn execute_command(&mut self, line: &str) {
        self.status_msg = None;
        let body = line.trim().trim_start_matches('/').trim();
        if body.is_empty() {
            return;
        }
        let (cmd, rest) = match body.find(char::is_whitespace) {
            Some(i) => (&body[..i], body[i..].trim()),
            None => (body, ""),
        };
        let cmd_lower = cmd.to_ascii_lowercase();

        if let Some(p) = Puzzle::from_name(&cmd_lower) {
            self.cmd_switch_puzzle(p);
            return;
        }

        match cmd_lower.as_str() {
            "new" => self.cmd_new_session(rest),
            "sessions" => self.cmd_list_sessions(),
            "session" => self.cmd_switch_session(rest),
            "rename" => self.cmd_rename(rest),
            "del" | "delete" => self.cmd_delete_last(),
            "dnf" => self.cmd_set_penalty(Penalty::Dnf),
            "+2" => self.cmd_set_penalty(Penalty::Plus2),
            "ok" => self.cmd_set_penalty(Penalty::None),
            "inspect" => {
                self.inspection_enabled = !self.inspection_enabled;
                let s = if self.inspection_enabled { "on" } else { "off" };
                self.status(format!("inspection: {}", s));
            }
            "help" => {
                self.show_help = !self.show_help;
            }
            "quit" | "q" => self.should_quit = true,
            _ => self.status(format!("unknown command: {}", cmd)),
        }
    }

    fn cmd_switch_puzzle(&mut self, puzzle: Puzzle) {
        // An empty session isn't committed to a puzzle yet: retype it in place.
        if self.current_session().solves.is_empty() {
            let session = self.current_session_mut();
            session.puzzle = puzzle;
            let name = session.name.clone();
            self.new_scramble();
            self.status(format!("session '{}' is now {}", name, puzzle.name()));
            self.save_now();
            return;
        }

        // A session with solves keeps its puzzle (stats must never mix): jump to the newest one instead.
        let target = self
            .save
            .sessions
            .iter()
            .filter(|s| s.puzzle == puzzle)
            .max_by_key(|s| (s.created_at, s.id))
            .map(|s| s.id);

        let id = match target {
            Some(id) => id,
            None => self.push_session("default".to_string(), puzzle),
        };
        self.save.active_session_id = id;
        self.new_scramble();
        let name = self.current_session().name.clone();
        self.status(format!("{} · session: {}", puzzle.name(), name));
        self.save_now();
    }

    /// Create a session, returning its id. Does not change the active session.
    fn push_session(&mut self, name: String, puzzle: Puzzle) -> u64 {
        let id = self.save.next_session_id;
        self.save.next_session_id = self.save.next_session_id.saturating_add(1);
        self.save.sessions.push(Session {
            id,
            name,
            puzzle,
            solves: Vec::new(),
            created_at: storage::now_millis(),
        });
        id
    }

    fn cmd_new_session(&mut self, rest: &str) {
        let puzzle = self.puzzle();
        let name = if rest.is_empty() {
            let n = self
                .save
                .sessions
                .iter()
                .filter(|s| s.puzzle == puzzle)
                .count()
                + 1;
            format!("session {}", n)
        } else {
            rest.to_string()
        };
        let id = self.push_session(name.clone(), puzzle);
        self.save.active_session_id = id;
        self.new_scramble();
        self.status(format!("new session: {} (#{})", name, id));
        self.save_now();
    }

    fn cmd_list_sessions(&mut self) {
        let list = self
            .save
            .sessions
            .iter()
            .map(|s| {
                format!(
                    "{}:{}({})[{}]",
                    s.id,
                    s.name,
                    s.puzzle.name(),
                    s.solves.len()
                )
            })
            .collect::<Vec<_>>()
            .join("  ");
        self.status(list);
    }

    fn cmd_switch_session(&mut self, rest: &str) {
        if rest.is_empty() {
            self.status("usage: /session <id>");
            return;
        }
        let id: u64 = match rest.trim().parse() {
            Ok(id) => id,
            Err(_) => {
                self.status(format!("not a session id: {}", rest));
                return;
            }
        };
        if !self.save.sessions.iter().any(|s| s.id == id) {
            self.status(format!("no session with id {}", id));
            return;
        }
        self.save.active_session_id = id;
        self.new_scramble();
        let s = self.current_session();
        let msg = format!("session: {} ({})", s.name, s.puzzle.name());
        self.status(msg);
        self.save_now();
    }

    fn cmd_rename(&mut self, rest: &str) {
        if rest.is_empty() {
            self.status("usage: /rename <name>");
            return;
        }
        let name = rest.to_string();
        self.current_session_mut().name = name.clone();
        self.status(format!("renamed session to {}", name));
        self.save_now();
    }

    fn cmd_delete_last(&mut self) {
        let popped = self.current_session_mut().solves.pop();
        match popped {
            Some(s) => {
                self.times_scroll = 0;
                let msg = format!("deleted {}", crate::types::format_solve(&s));
                self.status(msg);
                self.save_now();
            }
            None => self.status("no solves to delete"),
        }
    }

    fn cmd_set_penalty(&mut self, penalty: Penalty) {
        let shown = match self.current_session_mut().solves.last_mut() {
            Some(s) => {
                s.penalty = penalty;
                Some(crate::types::format_solve(s))
            }
            None => None,
        };
        match shown {
            Some(text) => {
                self.status(format!("last solve: {}", text));
                self.save_now();
            }
            None => self.status("no solves yet"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    // ------------------------------------------------------------- scaffolding

    const SPACE: KeyCode = KeyCode::Char(' ');

    /// A unique self-deleting temp path, so tests never touch the real save file.
    struct TempPath {
        path: PathBuf,
    }

    impl TempPath {
        fn new(tag: &str) -> TempPath {
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
    fn test_app(tag: &str) -> (App, TempPath) {
        test_app_with(tag, SaveFile::default())
    }

    fn test_app_with(tag: &str, save: SaveFile) -> (App, TempPath) {
        let guard = TempPath::new(tag);
        let app = App::new(save, guard.path.clone());
        (app, guard)
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn release(code: KeyCode) -> KeyEvent {
        KeyEvent {
            kind: KeyEventKind::Release,
            ..KeyEvent::new(code, KeyModifiers::NONE)
        }
    }

    fn repeat(code: KeyCode) -> KeyEvent {
        KeyEvent {
            kind: KeyEventKind::Repeat,
            ..KeyEvent::new(code, KeyModifiers::NONE)
        }
    }

    /// An `Instant` in the past: how tests simulate elapsed time without sleeping.
    fn ago(d: Duration) -> Instant {
        Instant::now()
            .checked_sub(d)
            .expect("monotonic clock is too young to backdate this test")
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Type `/<body>` and press Enter, exactly as a user would.
    fn run_command(app: &mut App, body: &str) {
        app.on_key(press(KeyCode::Char('/')));
        assert_eq!(app.input_mode, InputMode::Command, "'/' must enter command mode");
        for c in body.chars() {
            app.on_key(press(KeyCode::Char(c)));
        }
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.input_mode, InputMode::Normal, "Enter must leave command mode");
    }

    fn add_solve(app: &mut App, millis: u64) {
        app.current_session_mut().solves.push(Solve {
            millis,
            penalty: Penalty::None,
            scramble: "R U R' U'".to_string(),
            timestamp: 0,
        });
    }

    /// Drive Idle -> Timing, backdating the arm so the release counts as held.
    fn start_timing_now(app: &mut App) {
        app.inspection_enabled = false;
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

    // --------------------------------------------------------- idle -> arming

    #[test]
    fn inspection_is_off_by_default_and_space_press_arms_immediately() {
        let (mut app, _g) = test_app("default-insp-off");
        assert!(!app.inspection_enabled, "inspection must default to off");
        app.on_key(press(SPACE));
        assert!(
            matches!(
                app.state,
                TimerState::Armed {
                    from_inspection: false,
                    ..
                }
            ),
            "space Press must arm straight away by default, got {:?}",
            app.state
        );
    }

    #[test]
    fn space_press_is_ignored_in_idle_when_inspection_is_on() {
        let (mut app, _g) = test_app("press-insp-on");
        app.inspection_enabled = true;
        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle, "press alone must not arm");
    }

    #[test]
    fn space_release_starts_inspection_when_inspection_is_on() {
        let (mut app, _g) = test_app("release-insp-on");
        app.inspection_enabled = true;
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        assert_eq!(app.inspection_remaining, Some(15));
        assert_eq!(app.pending_inspection_penalty, Penalty::None);
    }

    #[test]
    fn space_press_arms_when_inspection_is_off() {
        let (mut app, _g) = test_app("press-insp-off");
        app.inspection_enabled = false;
        app.on_key(press(SPACE));
        assert!(matches!(
            app.state,
            TimerState::Armed {
                from_inspection: false,
                ..
            }
        ));
        assert!(!app.armed_ready(), "not held long enough yet");
    }

    #[test]
    fn releasing_too_early_returns_to_idle() {
        let (mut app, _g) = test_app("early-release");
        app.inspection_enabled = false;
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert_eq!(app.state, TimerState::Idle, "a short tap must not start the timer");
        assert_eq!(app.inspection_remaining, None);
    }

    #[test]
    fn holding_past_the_threshold_starts_the_timer() {
        let (mut app, _g) = test_app("held-release");
        app.inspection_enabled = false;
        app.on_key(press(SPACE));
        let TimerState::Armed { from_inspection, .. } = app.state else {
            panic!("expected Armed, got {:?}", app.state);
        };
        app.state = TimerState::Armed {
            since: ago(ms(350)),
            from_inspection,
        };
        assert!(app.armed_ready());
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Timing { .. }));
        assert_eq!(app.display_millis, 0);
    }

    #[test]
    fn armed_ready_tracks_the_threshold_and_is_false_off_state() {
        let (mut app, _g) = test_app("armed-ready");
        assert!(!app.armed_ready(), "Idle is never armed-ready");

        app.state = TimerState::Armed {
            since: ago(ms(10)),
            from_inspection: false,
        };
        assert!(!app.armed_ready());

        app.state = TimerState::Armed {
            since: ago(ARM_THRESHOLD + ms(50)),
            from_inspection: false,
        };
        assert!(app.armed_ready());
    }

    #[test]
    fn esc_while_armed_unarms_as_a_safety_valve() {
        let (mut app, _g) = test_app("armed-esc");
        app.inspection_enabled = false;
        app.on_key(press(SPACE));
        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.state, TimerState::Idle);
    }

    // ------------------------------------------------------------- inspection

    #[test]
    fn inspection_under_fifteen_seconds_has_no_penalty() {
        let (mut app, _g) = test_app("insp-clean");
        app.state = TimerState::Inspecting {
            started: ago(Duration::from_secs(10)),
        };
        app.on_tick();
        assert_eq!(app.pending_inspection_penalty, Penalty::None);
        assert_eq!(app.inspection_remaining, Some(5));
    }

    #[test]
    fn sixteen_seconds_of_inspection_is_a_pending_plus_two() {
        let (mut app, _g) = test_app("insp-plus2");
        app.state = TimerState::Inspecting {
            started: ago(Duration::from_secs(16)),
        };
        app.on_tick();
        assert_eq!(app.pending_inspection_penalty, Penalty::Plus2);
        assert_eq!(app.inspection_remaining, Some(-1));
    }

    #[test]
    fn eighteen_seconds_of_inspection_is_a_pending_dnf() {
        let (mut app, _g) = test_app("insp-dnf");
        app.state = TimerState::Inspecting {
            started: ago(Duration::from_secs(18)),
        };
        app.on_tick();
        assert_eq!(app.pending_inspection_penalty, Penalty::Dnf);
        assert_eq!(app.inspection_remaining, Some(-3));
    }

    #[test]
    fn esc_cancels_inspection_and_clears_the_pending_penalty() {
        let (mut app, _g) = test_app("insp-cancel");
        app.state = TimerState::Inspecting {
            started: ago(Duration::from_secs(16)),
        };
        app.inspection_start = Some(ago(Duration::from_secs(16)));
        app.on_tick();
        assert_eq!(app.pending_inspection_penalty, Penalty::Plus2);

        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.state, TimerState::Idle);
        assert_eq!(app.pending_inspection_penalty, Penalty::None);
        assert_eq!(app.inspection_remaining, None);
        assert!(app.inspection_start.is_none());
    }

    #[test]
    fn aborted_arm_from_inspection_restores_the_original_countdown() {
        let (mut app, _g) = test_app("insp-restore");
        let started = ago(ms(5_500));
        app.state = TimerState::Inspecting { started };
        app.inspection_start = Some(started);
        app.on_tick();
        assert_eq!(app.inspection_remaining, Some(10));

        app.on_key(press(SPACE));
        assert!(matches!(
            app.state,
            TimerState::Armed {
                from_inspection: true,
                ..
            }
        ));

        // Released too soon: back to inspecting, countdown *not* restarted.
        app.on_key(release(SPACE));
        match app.state {
            TimerState::Inspecting { started: s } => assert_eq!(s, started),
            other => panic!("expected Inspecting, got {:?}", other),
        }
        app.on_tick();
        assert_eq!(app.inspection_remaining, Some(10));
    }

    #[test]
    fn ticking_while_armed_from_inspection_keeps_the_countdown_running() {
        let (mut app, _g) = test_app("armed-tick");
        let started = ago(Duration::from_secs(16));
        app.inspection_start = Some(started);
        app.state = TimerState::Armed {
            since: Instant::now(),
            from_inspection: true,
        };
        app.on_tick();
        assert_eq!(app.pending_inspection_penalty, Penalty::Plus2);
    }

    #[test]
    fn a_solve_after_late_inspection_carries_the_plus_two() {
        let (mut app, _g) = test_app("carry-plus2");
        let started = ago(Duration::from_secs(16));
        app.state = TimerState::Inspecting { started };
        app.inspection_start = Some(started);
        app.on_tick();
        assert_eq!(app.pending_inspection_penalty, Penalty::Plus2);

        app.on_key(press(SPACE));
        app.state = TimerState::Armed {
            since: ago(ms(350)),
            from_inspection: true,
        };
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Timing { .. }));
        assert_eq!(
            app.pending_inspection_penalty,
            Penalty::Plus2,
            "starting the timer must not clear the inspection penalty"
        );

        app.state = TimerState::Timing {
            started: ago(ms(12_340)),
        };
        app.on_key(press(SPACE));

        let solve = app.current_session().solves.last().expect("solve recorded");
        assert_eq!(solve.penalty, Penalty::Plus2);
        assert!(
            (12_340..13_500).contains(&solve.millis),
            "unexpected raw time {}",
            solve.millis
        );
        assert_eq!(
            app.pending_inspection_penalty,
            Penalty::None,
            "penalty is consumed by the solve"
        );
    }

    #[test]
    fn a_solve_after_a_dnf_inspection_carries_the_dnf() {
        let (mut app, _g) = test_app("carry-dnf");
        let started = ago(Duration::from_secs(18));
        app.state = TimerState::Inspecting { started };
        app.inspection_start = Some(started);
        app.on_tick();
        assert_eq!(app.pending_inspection_penalty, Penalty::Dnf);

        app.on_key(press(SPACE));
        app.state = TimerState::Armed {
            since: ago(ms(400)),
            from_inspection: true,
        };
        app.on_key(release(SPACE));
        app.state = TimerState::Timing {
            started: ago(ms(8_000)),
        };
        app.on_key(press(SPACE));

        let solve = app.current_session().solves.last().expect("solve recorded");
        assert_eq!(solve.penalty, Penalty::Dnf);
        assert_eq!(solve.effective_millis(), None);
    }

    // ----------------------------------------------------------- timing/stop

    #[test]
    fn on_tick_updates_the_display_while_timing() {
        let (mut app, _g) = test_app("timing-tick");
        app.state = TimerState::Timing {
            started: ago(ms(4_200)),
        };
        app.on_tick();
        assert!(
            (4_200..5_000).contains(&app.display_millis),
            "display_millis was {}",
            app.display_millis
        );
    }

    #[test]
    fn any_key_press_stops_the_timer_and_records_a_solve() {
        let (mut app, _g) = test_app("stop-records");
        let before = app.scramble.clone();
        start_timing_now(&mut app);
        app.state = TimerState::Timing {
            started: ago(ms(9_870)),
        };

        app.on_key(press(KeyCode::Char('x')));

        assert_eq!(app.state, TimerState::Idle);
        let solves = &app.current_session().solves;
        assert_eq!(solves.len(), 1);
        assert_eq!(solves[0].penalty, Penalty::None);
        assert_eq!(solves[0].scramble, before, "the solved scramble is recorded");
        assert!(solves[0].timestamp > 0);
        assert!((9_870..10_600).contains(&solves[0].millis));
        assert_ne!(app.scramble, before, "a fresh scramble is generated");
        assert_eq!(app.times_scroll, 0);
    }

    #[test]
    fn the_stopping_key_press_does_not_also_run_its_normal_action() {
        let (mut app, _g) = test_app("stop-consumed");
        start_timing_now(&mut app);
        app.state = TimerState::Timing {
            started: ago(ms(1_000)),
        };

        // 'q' would normally quit; here it only stops the timer.
        app.on_key(press(KeyCode::Char('q')));
        assert!(!app.should_quit, "the stopping press must be consumed");
        assert_eq!(app.current_session().solves.len(), 1);
        assert_eq!(app.state, TimerState::Idle);
    }

    #[test]
    fn the_release_matching_the_stopping_press_is_swallowed() {
        let (mut app, _g) = test_app("stop-swallow");
        app.inspection_enabled = true;
        app.state = TimerState::Timing {
            started: ago(ms(2_000)),
        };

        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle);
        app.on_key(release(SPACE));
        assert_eq!(
            app.state,
            TimerState::Idle,
            "the release that ended the stopping press must not start inspection"
        );

        // After the release and the cooldown, the next space pair works normally.
        app.stopped_at = Some(ago(STOP_COOLDOWN + ms(50)));
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
    }

    #[test]
    fn holding_space_after_a_stop_is_completely_inert() {
        let (mut app, _g) = test_app("stop-autorepeat");
        app.inspection_enabled = true;
        app.state = TimerState::Timing {
            started: ago(ms(2_000)),
        };

        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle);

        // Windows auto-repeat sends Press (not Repeat) while the key is held.
        for _ in 0..8 {
            app.on_key(press(SPACE));
            assert_eq!(app.state, TimerState::Idle, "auto-repeat must not arm");
        }
        // Letting go finally is a no-op too.
        app.on_key(release(SPACE));
        assert_eq!(
            app.state,
            TimerState::Idle,
            "releasing a held stop key must not start inspection"
        );
        assert_eq!(app.current_session().solves.len(), 1, "exactly one solve");
    }

    #[test]
    fn auto_repeat_after_a_stop_does_not_arm_with_inspection_off() {
        let (mut app, _g) = test_app("stop-autorepeat-noinsp");
        app.inspection_enabled = false;
        app.state = TimerState::Timing {
            started: ago(ms(2_000)),
        };

        app.on_key(press(SPACE));
        for _ in 0..5 {
            app.on_key(press(SPACE));
        }
        assert_eq!(app.state, TimerState::Idle, "auto-repeat must not arm");
        app.on_key(release(SPACE));
        assert_eq!(app.state, TimerState::Idle);
    }

    #[test]
    fn space_is_ignored_during_the_stop_cooldown() {
        let (mut app, _g) = test_app("stop-cooldown");
        app.inspection_enabled = true;
        app.state = TimerState::Timing {
            started: ago(ms(2_000)),
        };

        app.on_key(press(SPACE));
        app.on_key(release(SPACE)); // clean release: the key is no longer inert

        // Still inside the cooldown window: a brand new tap does nothing.
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert_eq!(
            app.state,
            TimerState::Idle,
            "space must not start inspection inside the cooldown"
        );

        // Non-space keys are never blocked by the cooldown.
        app.on_key(press(KeyCode::Char('h')));
        assert!(app.show_help, "'h' still toggles help during the cooldown");
        app.on_key(press(KeyCode::Char('h')));
        let scramble = app.scramble.clone();
        app.on_key(press(KeyCode::Char('n')));
        assert_ne!(app.scramble, scramble, "'n' still re-scrambles");

        // Backdate the stop so the cooldown has expired: normal flow resumes.
        app.stopped_at = Some(ago(STOP_COOLDOWN + ms(50)));
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
    }

    #[test]
    fn finishing_a_solve_writes_it_to_disk() {
        let (mut app, _g) = test_app("stop-saves");
        start_timing_now(&mut app);
        app.state = TimerState::Timing {
            started: ago(ms(3_000)),
        };
        app.on_key(press(SPACE));

        let loaded = storage::load(&app.data_path).expect("save file written");
        assert_eq!(loaded.sessions[0].solves.len(), 1);
        assert!(app.status_msg.is_none(), "no save error expected");
    }

    #[test]
    fn repeat_key_events_are_ignored() {
        let (mut app, _g) = test_app("repeat");
        app.on_key(repeat(KeyCode::Char('q')));
        assert!(!app.should_quit);
        app.on_key(repeat(KeyCode::Char('/')));
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    // ------------------------------------------------------------- commands

    #[test]
    fn slash_enters_command_mode_and_esc_cancels() {
        let (mut app, _g) = test_app("cmd-esc");
        app.status_msg = Some("stale".to_string());
        app.on_key(press(KeyCode::Char('/')));
        assert_eq!(app.input_mode, InputMode::Command);
        assert_eq!(app.command_buf, "/");
        assert!(app.status_msg.is_none(), "entering command mode clears the status");

        app.on_key(press(KeyCode::Char('n')));
        app.on_key(press(KeyCode::Char('e')));
        assert_eq!(app.command_buf, "/ne");

        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.command_buf.is_empty());
        assert_eq!(
            app.current_session().name,
            "default",
            "a cancelled command must not run"
        );
    }

    #[test]
    fn backspace_pops_a_char_and_leaving_the_lone_slash_exits_command_mode() {
        let (mut app, _g) = test_app("cmd-backspace");
        app.on_key(press(KeyCode::Char('/')));
        app.on_key(press(KeyCode::Char('d')));
        app.on_key(press(KeyCode::Char('e')));
        assert_eq!(app.command_buf, "/de");

        app.on_key(press(KeyCode::Backspace));
        assert_eq!(app.command_buf, "/d");
        assert_eq!(app.input_mode, InputMode::Command);

        app.on_key(press(KeyCode::Backspace));
        assert_eq!(app.command_buf, "/");
        assert_eq!(app.input_mode, InputMode::Command);

        app.on_key(press(KeyCode::Backspace));
        assert_eq!(app.input_mode, InputMode::Normal, "popping '/' exits command mode");
        assert!(app.command_buf.is_empty());
    }

    #[test]
    fn normal_keys_do_nothing_while_in_command_mode() {
        let (mut app, _g) = test_app("cmd-typing");
        app.on_key(press(KeyCode::Char('/')));
        app.on_key(press(KeyCode::Char('q')));
        assert!(!app.should_quit, "'q' is just a character here");
        assert_eq!(app.command_buf, "/q");
        // Releases are ignored in command mode too.
        app.on_key(release(SPACE));
        assert_eq!(app.state, TimerState::Idle);
        assert_eq!(app.command_buf, "/q");
    }

    #[test]
    fn an_empty_command_does_nothing() {
        let (mut app, _g) = test_app("cmd-empty");
        app.on_key(press(KeyCode::Char('/')));
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn an_unknown_command_reports_itself_in_the_status() {
        let (mut app, _g) = test_app("cmd-unknown");
        run_command(&mut app, "bogus");
        assert_eq!(app.status_msg.as_deref(), Some("unknown command: bogus"));
    }

    #[test]
    fn puzzle_commands_switch_puzzle_and_session() {
        let (mut app, _g) = test_app("cmd-puzzle");
        // A session with solves is pinned to its puzzle, so /NxN jumps elsewhere.
        add_solve(&mut app, 12_000);
        let before = app.scramble.clone();
        assert_eq!(app.current_session().puzzle, Puzzle::Cube3);

        run_command(&mut app, "3x3");
        assert_eq!(app.current_session().id, 1, "already on a 3x3 session");
        assert_ne!(app.scramble, before, "a puzzle switch re-scrambles");

        run_command(&mut app, "2x2");
        assert_eq!(app.current_session().puzzle, Puzzle::Cube2);
        assert_eq!(app.current_session().name, "default");
        assert_eq!(app.save.sessions.len(), 2, "a 2x2 session was created");
        let moves = app.scramble.split_whitespace().count();
        assert!(
            (9..=11).contains(&moves),
            "2x2 scramble should be 9-11 moves, got {:?}",
            app.scramble
        );
        assert_eq!(app.status_msg.as_deref(), Some("2x2 · session: default"));

        let two_by_two_id = app.current_session().id;
        add_solve(&mut app, 3_000); // pin the 2x2 session to 2x2 as well
        run_command(&mut app, "3x3");
        assert_eq!(app.current_session().id, 1, "back to the 3x3 session");
        assert_eq!(app.save.sessions.len(), 2, "no extra session created");

        run_command(&mut app, "2x2");
        assert_eq!(
            app.current_session().id,
            two_by_two_id,
            "the existing 2x2 session is reused"
        );

        let loaded = storage::load(&app.data_path).expect("switching persists");
        assert_eq!(loaded.sessions.len(), 2);
        assert_eq!(loaded.active_session_id, two_by_two_id);
    }

    #[test]
    fn a_puzzle_command_retypes_an_empty_session_in_place() {
        let (mut app, _g) = test_app("cmd-puzzle-retype");
        let id = app.current_session().id;
        assert!(app.current_session().solves.is_empty());

        run_command(&mut app, "6x6");

        assert_eq!(app.save.sessions.len(), 1, "no new session is created");
        assert_eq!(app.current_session().id, id, "same session, new puzzle");
        assert_eq!(app.current_session().name, "default", "name is kept");
        assert_eq!(app.current_session().puzzle, Puzzle::Cube6);
        assert_eq!(
            app.status_msg.as_deref(),
            Some("session 'default' is now 6x6")
        );

        // The fresh scramble comes from the 6x6 pool.
        let moves: Vec<&str> = app.scramble.split_whitespace().collect();
        assert_eq!(moves.len(), 80, "6x6 scrambles are 80 moves");
        let pool = [
            "U", "D", "L", "R", "F", "B", "Uw", "Dw", "Lw", "Rw", "Fw", "Bw", "3Uw", "3Rw", "3Fw",
        ];
        for m in &moves {
            let base = m.trim_end_matches(['\'', '2']);
            assert!(pool.contains(&base), "{m} is not a 6x6 move");
        }

        let loaded = storage::load(&app.data_path).expect("retyping persists");
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0].id, id);
        assert_eq!(loaded.sessions[0].puzzle, Puzzle::Cube6);
    }

    #[test]
    fn a_puzzle_command_never_retypes_a_session_with_solves() {
        let (mut app, _g) = test_app("cmd-puzzle-pinned");
        add_solve(&mut app, 12_000);

        run_command(&mut app, "6x6");

        assert_eq!(app.save.sessions.len(), 2, "a 6x6 session is created");
        assert_eq!(
            app.save.sessions[0].puzzle,
            Puzzle::Cube3,
            "the session with solves keeps its puzzle"
        );
        assert_eq!(app.save.sessions[0].solves.len(), 1, "and keeps its solves");
        assert_eq!(app.save.sessions[0].name, "default");
        let six_id = app.current_session().id;
        assert_ne!(six_id, 1);
        assert_eq!(app.current_session().puzzle, Puzzle::Cube6);
        assert_eq!(app.current_session().name, "default");
        assert_eq!(app.status_msg.as_deref(), Some("6x6 · session: default"));

        // Once the 6x6 session has solves too, switching just hops between them.
        add_solve(&mut app, 180_000);
        run_command(&mut app, "3x3");
        assert_eq!(app.current_session().id, 1);
        run_command(&mut app, "6x6");
        assert_eq!(app.current_session().id, six_id);
        assert_eq!(app.save.sessions.len(), 2, "no further sessions created");
    }

    #[test]
    fn new_creates_a_session_for_the_current_puzzle() {
        let (mut app, _g) = test_app("cmd-new");
        add_solve(&mut app, 1_000);

        run_command(&mut app, "new");
        assert_eq!(app.save.sessions.len(), 2);
        assert_eq!(app.current_session().name, "session 2");
        assert_eq!(app.current_session().puzzle, Puzzle::Cube3);
        assert!(app.current_session().solves.is_empty());
        assert_ne!(app.save.active_session_id, 1);

        run_command(&mut app, "new one-handed");
        assert_eq!(app.save.sessions.len(), 3);
        assert_eq!(app.current_session().name, "one-handed");

        let loaded = storage::load(&app.data_path).expect("new session persists");
        assert_eq!(loaded.sessions.len(), 3);
    }

    #[test]
    fn session_switches_by_id_and_rejects_bad_input() {
        let (mut app, _g) = test_app("cmd-session");
        // With a solve recorded, /2x2 forks a second session instead of retyping.
        add_solve(&mut app, 9_000);
        run_command(&mut app, "2x2");
        let two = app.current_session().id;
        run_command(&mut app, "session 1");
        assert_eq!(app.save.active_session_id, 1);
        assert_eq!(app.current_session().puzzle, Puzzle::Cube3);
        assert_eq!(app.status_msg.as_deref(), Some("session: default (3x3)"));

        run_command(&mut app, &format!("session {}", two));
        assert_eq!(app.save.active_session_id, two);
        assert_eq!(app.current_session().puzzle, Puzzle::Cube2);

        run_command(&mut app, "session 999");
        assert_eq!(app.status_msg.as_deref(), Some("no session with id 999"));
        assert_eq!(app.save.active_session_id, two, "active session unchanged");

        run_command(&mut app, "session abc");
        assert_eq!(app.status_msg.as_deref(), Some("not a session id: abc"));

        run_command(&mut app, "session");
        assert_eq!(app.status_msg.as_deref(), Some("usage: /session <id>"));
    }

    #[test]
    fn sessions_lists_every_session() {
        let (mut app, _g) = test_app("cmd-sessions");
        add_solve(&mut app, 1_234);
        run_command(&mut app, "new second");
        run_command(&mut app, "sessions");
        let msg = app.status_msg.clone().expect("listing");
        assert!(msg.contains("1:default(3x3)[1]"), "got {msg}");
        assert!(msg.contains("second(3x3)[0]"), "got {msg}");
    }

    #[test]
    fn rename_renames_the_current_session() {
        let (mut app, _g) = test_app("cmd-rename");
        run_command(&mut app, "rename evening practice");
        assert_eq!(app.current_session().name, "evening practice");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("renamed session to evening practice")
        );

        run_command(&mut app, "rename");
        assert_eq!(app.status_msg.as_deref(), Some("usage: /rename <name>"));
        assert_eq!(app.current_session().name, "evening practice");

        let loaded = storage::load(&app.data_path).expect("rename persists");
        assert_eq!(loaded.sessions[0].name, "evening practice");
    }

    #[test]
    fn dnf_plus2_and_ok_mutate_the_last_solve() {
        let (mut app, _g) = test_app("cmd-penalty");
        add_solve(&mut app, 10_000);
        add_solve(&mut app, 12_000);

        run_command(&mut app, "dnf");
        assert_eq!(app.current_session().solves[1].penalty, Penalty::Dnf);
        assert_eq!(app.current_session().solves[0].penalty, Penalty::None);
        assert_eq!(app.status_msg.as_deref(), Some("last solve: DNF(12.00)"));

        run_command(&mut app, "+2");
        assert_eq!(app.current_session().solves[1].penalty, Penalty::Plus2);
        assert_eq!(app.status_msg.as_deref(), Some("last solve: 14.00+"));

        run_command(&mut app, "ok");
        assert_eq!(app.current_session().solves[1].penalty, Penalty::None);
        assert_eq!(app.status_msg.as_deref(), Some("last solve: 12.00"));

        let loaded = storage::load(&app.data_path).expect("penalties persist");
        assert_eq!(loaded.sessions[0].solves[1].penalty, Penalty::None);
    }

    #[test]
    fn penalty_commands_are_harmless_with_no_solves() {
        let (mut app, _g) = test_app("cmd-penalty-empty");
        run_command(&mut app, "dnf");
        assert_eq!(app.status_msg.as_deref(), Some("no solves yet"));
        assert!(app.current_session().solves.is_empty());
    }

    #[test]
    fn del_removes_the_last_solve() {
        let (mut app, _g) = test_app("cmd-del");
        add_solve(&mut app, 10_000);
        add_solve(&mut app, 12_345);
        app.times_scroll = 1;

        run_command(&mut app, "del");
        assert_eq!(app.current_session().solves.len(), 1);
        assert_eq!(app.current_session().solves[0].millis, 10_000);
        assert_eq!(app.times_scroll, 0);
        assert_eq!(app.status_msg.as_deref(), Some("deleted 12.34"));

        run_command(&mut app, "delete");
        assert!(app.current_session().solves.is_empty());

        run_command(&mut app, "del");
        assert_eq!(app.status_msg.as_deref(), Some("no solves to delete"));

        let loaded = storage::load(&app.data_path).expect("deletion persists");
        assert!(loaded.sessions[0].solves.is_empty());
    }

    #[test]
    fn inspect_toggles_inspection() {
        let (mut app, _g) = test_app("cmd-inspect");
        assert!(!app.inspection_enabled, "off by default");

        run_command(&mut app, "inspect");
        assert!(app.inspection_enabled);
        assert_eq!(app.status_msg.as_deref(), Some("inspection: on"));

        // ...and the timer flow follows the new setting.
        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle, "press alone must not arm");
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.state, TimerState::Idle);

        run_command(&mut app, "inspect");
        assert!(!app.inspection_enabled);
        assert_eq!(app.status_msg.as_deref(), Some("inspection: off"));

        // Back to the default flow: a press arms immediately again.
        app.on_key(press(SPACE));
        assert!(matches!(app.state, TimerState::Armed { .. }));
        app.on_key(release(SPACE));
        assert_eq!(app.state, TimerState::Idle);
    }

    #[test]
    fn help_and_quit_commands_work() {
        let (mut app, _g) = test_app("cmd-help-quit");
        run_command(&mut app, "help");
        assert!(app.show_help);
        run_command(&mut app, "help");
        assert!(!app.show_help);

        run_command(&mut app, "q");
        assert!(app.should_quit);

        let (mut app2, _g2) = test_app("cmd-quit-long");
        run_command(&mut app2, "quit");
        assert!(app2.should_quit);
    }

    #[test]
    fn commands_are_case_insensitive_and_tolerate_padding() {
        let (mut app, _g) = test_app("cmd-case");
        run_command(&mut app, "  INSPECT  ");
        assert!(app.inspection_enabled);
        run_command(&mut app, "2X2");
        assert_eq!(app.current_session().puzzle, Puzzle::Cube2);
    }

    // --------------------------------------------------------- normal-mode keys

    #[test]
    fn q_sets_should_quit() {
        let (mut app, _g) = test_app("key-q");
        app.on_key(press(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits_from_any_mode() {
        let (mut app, _g) = test_app("key-ctrl-c");
        app.on_key(press(KeyCode::Char('/')));
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn n_makes_a_new_scramble_and_h_toggles_help() {
        let (mut app, _g) = test_app("key-n-h");
        let before = app.scramble.clone();
        app.status_msg = Some("stale".to_string());
        app.on_key(press(KeyCode::Char('n')));
        assert_ne!(app.scramble, before);
        assert!(app.status_msg.is_none());

        app.on_key(press(KeyCode::Char('h')));
        assert!(app.show_help);
        app.on_key(press(KeyCode::Char('?')));
        assert!(!app.show_help);
    }

    #[test]
    fn esc_closes_help_then_clears_the_status() {
        let (mut app, _g) = test_app("key-esc");
        app.show_help = true;
        app.status_msg = Some("something".to_string());

        app.on_key(press(KeyCode::Esc));
        assert!(!app.show_help);
        assert_eq!(app.status_msg.as_deref(), Some("something"));

        app.on_key(press(KeyCode::Esc));
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn scrolling_an_empty_session_stays_clamped_at_zero() {
        let (mut app, _g) = test_app("scroll-empty");
        assert!(app.current_session().solves.is_empty());
        for key in [
            KeyCode::Down,
            KeyCode::Char('j'),
            KeyCode::PageDown,
            KeyCode::Up,
            KeyCode::Char('k'),
            KeyCode::PageUp,
            KeyCode::Home,
        ] {
            app.on_key(press(key));
            assert_eq!(app.times_scroll, 0, "{key:?} must not move past the end");
        }
    }

    #[test]
    fn scrolling_clamps_to_the_number_of_solves() {
        let (mut app, _g) = test_app("scroll-clamp");
        for i in 0..3 {
            add_solve(&mut app, 10_000 + i);
        }

        for _ in 0..6 {
            app.on_key(press(KeyCode::Down));
        }
        assert_eq!(app.times_scroll, 2, "clamped to solves.len() - 1");

        app.on_key(press(KeyCode::Up));
        assert_eq!(app.times_scroll, 1);

        app.on_key(press(KeyCode::PageDown));
        assert_eq!(app.times_scroll, 2);

        app.on_key(press(KeyCode::PageUp));
        assert_eq!(app.times_scroll, 0);

        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Home));
        assert_eq!(app.times_scroll, 0);
    }

    // ------------------------------------------------------------ constructor

    #[test]
    fn new_repairs_a_broken_save_file() {
        let broken = SaveFile {
            version: 1,
            next_session_id: 1,
            sessions: Vec::new(),
            active_session_id: 42,
        };
        let (app, _g) = test_app_with("ctor-empty", broken);
        assert_eq!(app.save.sessions.len(), 1);
        assert_eq!(app.save.active_session_id, app.save.sessions[0].id);
        assert!(app.save.next_session_id > app.save.sessions[0].id);
        assert_eq!(app.state, TimerState::Idle);
        assert!(!app.inspection_enabled, "inspection defaults to off");
        assert!(!app.scramble.is_empty());
    }

    #[test]
    fn new_picks_up_the_active_sessions_puzzle_for_the_first_scramble() {
        let save = SaveFile {
            version: 1,
            next_session_id: 3,
            sessions: vec![
                Session {
                    id: 1,
                    name: "default".to_string(),
                    puzzle: Puzzle::Cube3,
                    solves: Vec::new(),
                    created_at: 0,
                },
                Session {
                    id: 2,
                    name: "mini".to_string(),
                    puzzle: Puzzle::Cube2,
                    solves: Vec::new(),
                    created_at: 1,
                },
            ],
            active_session_id: 2,
        };
        let (app, _g) = test_app_with("ctor-puzzle", save);
        assert_eq!(app.current_session().puzzle, Puzzle::Cube2);
        let moves = app.scramble.split_whitespace().count();
        assert!((9..=11).contains(&moves), "2x2 scramble was {moves} moves");
    }

    #[test]
    fn a_save_failure_reports_instead_of_panicking() {
        // A path whose parent is a *file*, so create_dir_all/write must fail.
        let guard = TempPath::new("save-fail");
        fs::write(&guard.path, b"not a directory").expect("seed file");
        let mut app = App::new(SaveFile::default(), guard.path.join("nested.json"));

        add_solve(&mut app, 5_000);
        run_command(&mut app, "dnf");
        assert!(
            app.status_msg
                .as_deref()
                .is_some_and(|m| m.starts_with("save failed:")),
            "expected a save error, got {:?}",
            app.status_msg
        );

        // Quitting still works even though saving does not.
        app.on_key(press(KeyCode::Char('q')));
        assert!(app.should_quit);
    }
}
