//! Timer state machine and key handling; `ui` renders only from [`App`] fields refreshed here.
//!
//! The two clusters that answer their own question live beside this file: [`commands`] runs
//! the `/command` line, and [`repair`] makes a parsed save file internally consistent.

mod commands;
mod repair;
#[cfg(test)]
mod testkit;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::scramble;
use crate::stats::{self, PersonalBests, SessionStats};
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
/// First WCA judge call: the "8 seconds" warning.
const INSPECTION_CALL_8_MS: u128 = 8_000;
/// Second WCA judge call: the "12 seconds" warning.
const INSPECTION_CALL_12_MS: u128 = 12_000;
/// How far PageUp and PageDown move the times cursor.
const TIMES_PAGE: usize = 10;

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
    pub status_msg: Option<String>,
    pub show_help: bool,
    /// Open sessions overlay. Mutually exclusive with [`App::show_help`]; the solve detail wins over both.
    pub show_sessions: bool,
    /// Times-list cursor, counted from the newest solve: 0 is the newest.
    pub times_selected: usize,
    /// Open solve-detail overlay, holding the same index-from-newest as [`App::times_selected`].
    pub solve_detail: Option<usize>,
    pub should_quit: bool,

    // Derived-for-rendering fields, refreshed by on_tick()/state changes.
    pub display_millis: u64,
    pub inspection_remaining: Option<i64>,
    pub pending_inspection_penalty: Penalty,
    /// Judge-call stage of the running inspection: 0 under 8s, 1 from 8s, 2 from 12s.
    pub inspection_stage: u8,
    pub data_path: PathBuf,
    /// Statistics for the active session, cached by [`App::refresh_derived`].
    pub stats: SessionStats,
    /// All-time bests across every session of the active puzzle, cached by [`App::refresh_derived`].
    pub pbs: PersonalBests,

    // --- internal bookkeeping (not part of the ui contract) ---
    /// Inspection start, kept while armed so an aborted arm restores the countdown.
    inspection_start: Option<Instant>,
    /// A judge call is waiting to be sounded; `main.rs` drains it with [`App::take_bell`].
    bell_pending: bool,
    /// Key that stopped the timer: inert until its Release (Windows auto-repeat resends Press, not Repeat).
    inert_key: Option<KeyCode>,
    /// When the last solve was finalized; space is ignored for `STOP_COOLDOWN` after it.
    stopped_at: Option<Instant>,
}

impl App {
    pub fn new(save: SaveFile, data_path: PathBuf) -> App {
        let mut save = save;
        repair::sanitize(&mut save);
        let puzzle = save
            .sessions
            .iter()
            .find(|s| s.id == save.active_session_id)
            .map(|s| s.puzzle)
            .unwrap_or(Puzzle::Cube3);

        let mut app = App {
            save,
            state: TimerState::Idle,
            input_mode: InputMode::Normal,
            command_buf: String::new(),
            scramble: scramble::generate(puzzle),
            status_msg: None,
            show_help: false,
            show_sessions: false,
            times_selected: 0,
            solve_detail: None,
            should_quit: false,
            display_millis: 0,
            inspection_remaining: None,
            pending_inspection_penalty: Penalty::None,
            inspection_stage: 0,
            data_path,
            stats: SessionStats::default(),
            pbs: PersonalBests::default(),
            inspection_start: None,
            bell_pending: false,
            inert_key: None,
            stopped_at: None,
        };
        app.refresh_derived();
        app
    }

    /// Recompute the statistics `ui` renders from.
    ///
    /// Both walk every solve of the puzzle, so the 15 ms draw loop must never call them.
    /// Every path that changes the solve list, a penalty, the session list or the active
    /// session calls this instead; `/rename` is the one mutation that changes neither.
    fn refresh_derived(&mut self) {
        let puzzle = self.current_session().puzzle;
        let stats = stats::session_stats(&self.current_session().solves);
        let of_puzzle: Vec<&Session> = self
            .save
            .sessions
            .iter()
            .filter(|s| s.puzzle == puzzle)
            .collect();
        let pbs = stats::personal_bests(&of_puzzle);
        self.stats = stats;
        self.pbs = pbs;
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
        self.times_selected = 0;
    }

    /// Take the queued judge-call bell, if any. `main.rs` sounds it; nothing else may.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell_pending)
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

    /// Toggle the help overlay. The two popups are alternatives, so opening one closes the other.
    fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.show_sessions = false;
        }
    }

    fn start_inspection(&mut self) {
        let now = Instant::now();
        self.state = TimerState::Inspecting { started: now };
        self.inspection_start = Some(now);
        self.pending_inspection_penalty = Penalty::None;
        self.inspection_remaining = Some(INSPECTION_SECS);
        self.inspection_stage = 0;
        self.bell_pending = false;
        self.display_millis = 0;
        self.status_msg = None;
    }

    fn cancel_inspection(&mut self) {
        self.state = TimerState::Idle;
        self.inspection_start = None;
        self.inspection_remaining = None;
        self.pending_inspection_penalty = Penalty::None;
        self.inspection_stage = 0;
        self.bell_pending = false;
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
        self.inspection_stage = 0;
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
        self.inspection_stage = 0;
        self.display_millis = millis;
        self.state = TimerState::Idle;
        self.stopped_at = Some(Instant::now());
        self.new_scramble();
        self.refresh_derived();
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

        // The stage doubles as the record of which calls have already sounded, so each
        // threshold rings once per inspection however many ticks land on top of it.
        let stage = if elapsed >= INSPECTION_CALL_12_MS {
            2
        } else if elapsed >= INSPECTION_CALL_8_MS {
            1
        } else {
            0
        };
        if stage > self.inspection_stage {
            self.inspection_stage = stage;
            self.bell_pending = true;
        }
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

        // The solve overlay is modal: nothing behind it may act, Ctrl-C above excepted.
        if self.solve_detail.is_some() {
            if key.kind == KeyEventKind::Press {
                self.on_key_solve_detail(key);
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
                KeyCode::Char(' ') if !self.save.settings.inspection => self.arm(false),
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
                KeyCode::Char('h') | KeyCode::Char('?') => self.toggle_help(),
                KeyCode::Esc => {
                    if self.show_help || self.show_sessions {
                        self.show_help = false;
                        self.show_sessions = false;
                    } else {
                        self.status_msg = None;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => self.select_newer(1),
                KeyCode::Down | KeyCode::Char('j') => self.select_older(1),
                KeyCode::PageUp => self.select_newer(TIMES_PAGE),
                KeyCode::PageDown => self.select_older(TIMES_PAGE),
                KeyCode::Home => self.times_selected = 0,
                KeyCode::Enter => self.open_solve_detail(),
                _ => {}
            },
            KeyEventKind::Release => {
                if key.code == KeyCode::Char(' ') && self.save.settings.inspection {
                    self.start_inspection();
                }
            }
            KeyEventKind::Repeat => {}
        }
    }

    // --------------------------------------------------------- the times list

    /// Move the cursor toward newer solves; 0 is the newest and it stops there.
    fn select_newer(&mut self, by: usize) {
        self.times_selected = self.times_selected.saturating_sub(by);
    }

    /// Move the cursor toward older solves, clamped to the oldest one in the session.
    fn select_older(&mut self, by: usize) {
        let oldest = self.current_session().solves.len().saturating_sub(1);
        self.times_selected = self.times_selected.saturating_add(by).min(oldest);
    }

    /// Open the detail overlay on the selected solve. An empty session has nothing to show.
    fn open_solve_detail(&mut self) {
        let count = self.current_session().solves.len();
        if count == 0 {
            return;
        }
        self.solve_detail = Some(self.times_selected.min(count - 1));
    }

    fn on_key_solve_detail(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('r') => self.recall_scramble(),
            KeyCode::Esc | KeyCode::Enter => self.solve_detail = None,
            _ => {}
        }
    }

    /// Put the open solve's scramble back on screen for another attempt, and close the overlay.
    fn recall_scramble(&mut self) {
        let Some(index) = self.solve_detail else {
            return;
        };
        let count = self.current_session().solves.len();
        let Some(position) = count.checked_sub(index + 1) else {
            self.solve_detail = None;
            return;
        };
        self.scramble = self.current_session().solves[position].scramble.clone();
        self.solve_detail = None;
        // The overlay is numbered like the times list, where the newest solve is `count`.
        self.status(format!("scramble loaded from solve {}", count - index));
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
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use std::fs;

    /// Backdate the running inspection so the next tick sees `started` as its beginning.
    fn set_inspection_started(app: &mut App, started: Instant) {
        app.state = TimerState::Inspecting { started };
        app.inspection_start = Some(started);
    }

    // --------------------------------------------------------- idle -> arming

    #[test]
    fn inspection_is_off_by_default_and_space_press_arms_immediately() {
        let (mut app, _g) = test_app("default-insp-off");
        assert!(!app.save.settings.inspection, "inspection must default to off");
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
        app.save.settings.inspection = true;
        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle, "press alone must not arm");
    }

    #[test]
    fn space_release_starts_inspection_when_inspection_is_on() {
        let (mut app, _g) = test_app("release-insp-on");
        app.save.settings.inspection = true;
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        assert_eq!(app.inspection_remaining, Some(15));
        assert_eq!(app.pending_inspection_penalty, Penalty::None);
    }

    #[test]
    fn space_press_arms_when_inspection_is_off() {
        let (mut app, _g) = test_app("press-insp-off");
        app.save.settings.inspection = false;
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
        app.save.settings.inspection = false;
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert_eq!(app.state, TimerState::Idle, "a short tap must not start the timer");
        assert_eq!(app.inspection_remaining, None);
    }

    #[test]
    fn holding_past_the_threshold_starts_the_timer() {
        let (mut app, _g) = test_app("held-release");
        app.save.settings.inspection = false;
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
        app.save.settings.inspection = false;
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

    // ---------------------------------------------------------- judge calls

    #[test]
    fn each_judge_call_rings_exactly_once() {
        let (mut app, _g) = test_app("insp-bell");
        app.save.settings.inspection = true;
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        assert_eq!(app.inspection_stage, 0);
        assert!(!app.take_bell(), "inspection starts silent");

        set_inspection_started(&mut app, ago(ms(7_900)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 0, "still under eight seconds");
        assert!(!app.take_bell());

        set_inspection_started(&mut app, ago(ms(8_100)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 1);
        assert!(app.take_bell(), "the eight second call rings");
        assert!(!app.take_bell(), "take_bell clears what it returns");

        set_inspection_started(&mut app, ago(ms(11_500)));
        app.on_tick();
        app.on_tick();
        assert_eq!(app.inspection_stage, 1);
        assert!(!app.take_bell(), "later ticks in the same stage stay silent");

        set_inspection_started(&mut app, ago(ms(12_100)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 2);
        assert!(app.take_bell(), "the twelve second call rings");

        set_inspection_started(&mut app, ago(ms(14_000)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 2, "there is no third call");
        assert!(!app.take_bell());
    }

    #[test]
    fn the_calls_keep_coming_while_armed_from_inspection() {
        let (mut app, _g) = test_app("insp-bell-armed");
        app.inspection_start = Some(ago(ms(8_200)));
        app.state = TimerState::Armed {
            since: Instant::now(),
            from_inspection: true,
        };
        app.on_tick();
        assert_eq!(app.inspection_stage, 1);
        assert!(app.take_bell(), "the clock is still running while armed");
    }

    #[test]
    fn cancelling_inspection_resets_the_stage_and_the_pending_bell() {
        let (mut app, _g) = test_app("insp-bell-reset");
        app.save.settings.inspection = true;
        set_inspection_started(&mut app, ago(ms(12_500)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 2);

        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.state, TimerState::Idle);
        assert_eq!(app.inspection_stage, 0);
        assert!(!app.take_bell(), "a cancelled inspection leaves nothing queued");

        // A fresh inspection starts from stage 0 and calls again.
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        assert_eq!(app.inspection_stage, 0);
        set_inspection_started(&mut app, ago(ms(8_500)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 1);
        assert!(app.take_bell());
    }

    #[test]
    fn starting_the_timer_clears_the_inspection_stage() {
        let (mut app, _g) = test_app("insp-stage-clear");
        app.save.settings.inspection = true;
        set_inspection_started(&mut app, ago(ms(9_000)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 1);

        app.on_key(press(SPACE));
        app.state = TimerState::Armed {
            since: ago(ms(350)),
            from_inspection: true,
        };
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Timing { .. }));
        assert_eq!(app.inspection_stage, 0, "the calls belong to inspection only");

        app.state = TimerState::Timing {
            started: ago(ms(3_000)),
        };
        app.on_key(press(SPACE));
        assert_eq!(app.inspection_stage, 0);
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
        assert_eq!(app.times_selected, 0);
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
        app.save.settings.inspection = true;
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
        app.save.settings.inspection = true;
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
        app.save.settings.inspection = false;
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
        app.save.settings.inspection = true;
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
    fn esc_closes_the_sessions_overlay_and_help_replaces_it() {
        let (mut app, _g) = test_app("key-sessions");
        app.show_sessions = true;
        app.status_msg = Some("something".to_string());

        // The two popups are alternatives, so 'h' takes the screen from the listing.
        app.on_key(press(KeyCode::Char('h')));
        assert!(app.show_help, "'h' still opens the help");
        assert!(!app.show_sessions, "and closes the sessions overlay");
        app.on_key(press(KeyCode::Char('?')));
        assert!(!app.show_help, "'?' still closes it");
        assert!(!app.show_sessions);

        app.show_sessions = true;
        app.on_key(press(KeyCode::Esc));
        assert!(!app.show_sessions, "Esc closes the sessions overlay");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("something"),
            "and stops there, exactly as it does for the help"
        );
        app.on_key(press(KeyCode::Esc));
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn the_sessions_overlay_is_not_modal() {
        // It behaves like the help overlay, which the keys behind it ignore entirely.
        let (mut app, _g) = test_app("key-sessions-nonmodal");
        app.show_sessions = true;
        let before = app.scramble.clone();

        app.on_key(press(KeyCode::Char('n')));
        assert_ne!(app.scramble, before, "'n' still re-scrambles");
        assert!(app.show_sessions, "and the overlay stays open");

        app.on_key(press(SPACE));
        assert!(matches!(app.state, TimerState::Armed { .. }), "space still arms");
    }

    // ------------------------------------------------------- the times cursor

    #[test]
    fn the_cursor_stays_at_zero_in_an_empty_session() {
        let (mut app, _g) = test_app("select-empty");
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
            assert_eq!(app.times_selected, 0, "{key:?} must not move in an empty session");
        }
    }

    #[test]
    fn one_solve_leaves_the_cursor_nowhere_to_go() {
        let (mut app, _g) = test_app("select-single");
        add_solve(&mut app, 10_000);
        for key in [KeyCode::Down, KeyCode::Char('j'), KeyCode::PageDown] {
            app.on_key(press(key));
            assert_eq!(app.times_selected, 0, "{key:?} has no older solve to reach");
        }
        app.on_key(press(KeyCode::Up));
        assert_eq!(app.times_selected, 0);
    }

    #[test]
    fn the_cursor_moves_toward_older_solves_and_back_again() {
        let (mut app, _g) = test_app("select-move");
        for i in 0..15 {
            add_solve(&mut app, 10_000 + i);
        }

        app.on_key(press(KeyCode::Down));
        assert_eq!(app.times_selected, 1, "Down moves toward older solves");
        app.on_key(press(KeyCode::Char('j')));
        assert_eq!(app.times_selected, 2);
        app.on_key(press(KeyCode::Up));
        assert_eq!(app.times_selected, 1, "Up moves back toward the newest");
        app.on_key(press(KeyCode::Char('k')));
        assert_eq!(app.times_selected, 0);
        app.on_key(press(KeyCode::Char('k')));
        assert_eq!(app.times_selected, 0, "0 is the newest and the cursor stops there");

        app.on_key(press(KeyCode::PageDown));
        assert_eq!(app.times_selected, 10, "PageDown jumps ten toward the oldest");
        app.on_key(press(KeyCode::PageDown));
        assert_eq!(app.times_selected, 14, "and clamps to the oldest solve");
        app.on_key(press(KeyCode::PageUp));
        assert_eq!(app.times_selected, 4);
        app.on_key(press(KeyCode::PageUp));
        assert_eq!(app.times_selected, 0, "PageUp clamps at the newest");

        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Home));
        assert_eq!(app.times_selected, 0, "Home returns to the newest");
    }

    #[test]
    fn a_new_solve_resets_the_cursor() {
        let (mut app, _g) = test_app("select-reset-solve");
        for i in 0..4 {
            add_solve(&mut app, 10_000 + i);
        }
        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Down));
        assert_eq!(app.times_selected, 2);

        perform_solve(&mut app);
        assert_eq!(app.times_selected, 0, "a recorded solve brings the cursor home");
    }

    #[test]
    fn every_session_change_resets_the_cursor() {
        let (mut app, _g) = test_app("select-reset-session");
        for i in 0..4 {
            add_solve(&mut app, 10_000 + i);
        }

        app.times_selected = 3;
        run_command(&mut app, "2x2");
        assert_eq!(app.times_selected, 0, "a puzzle switch resets the cursor");

        app.times_selected = 2;
        run_command(&mut app, "session 1");
        assert_eq!(app.times_selected, 0, "a session switch resets the cursor");

        app.times_selected = 2;
        run_command(&mut app, "new evening");
        assert_eq!(app.times_selected, 0, "a new session resets the cursor");

        app.times_selected = 2;
        run_command(&mut app, "skewb");
        assert_eq!(app.times_selected, 0, "retyping in place resets the cursor");

        app.times_selected = 2;
        run_command(&mut app, "delsession");
        assert_eq!(app.times_selected, 0, "deleting the active session resets the cursor");
    }

    // ---------------------------------------------------- solve detail overlay

    #[test]
    fn enter_opens_the_overlay_on_the_selected_solve() {
        let (mut app, _g) = test_app("detail-open");
        for i in 1..=3 {
            add_solve(&mut app, 10_000 + i);
        }
        app.on_key(press(KeyCode::Down));
        assert_eq!(app.times_selected, 1);

        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, Some(1), "the overlay opens on the cursor");
        assert_eq!(app.state, TimerState::Idle, "opening it disturbs no timer state");
        assert_eq!(app.display_millis, 0);
    }

    #[test]
    fn enter_opens_nothing_in_an_empty_session() {
        let (mut app, _g) = test_app("detail-empty");
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, None);
    }

    #[test]
    fn the_overlay_does_not_open_mid_solve() {
        let (mut app, _g) = test_app("detail-mid-solve");
        add_solve(&mut app, 10_000);
        app.save.settings.inspection = true;

        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, None, "Enter is inert during inspection");
        assert!(matches!(app.state, TimerState::Inspecting { .. }));

        app.on_key(press(SPACE));
        assert!(matches!(app.state, TimerState::Armed { .. }));
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, None, "Enter is inert while armed");
        assert!(matches!(app.state, TimerState::Armed { .. }));

        app.state = TimerState::Timing {
            started: ago(ms(5_000)),
        };
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, None, "Enter is inert while timing");
        assert_eq!(app.state, TimerState::Idle, "it stops the timer instead");
        assert_eq!(app.current_session().solves.len(), 2);
    }

    #[test]
    fn the_overlay_swallows_every_key_but_its_own() {
        let (mut app, _g) = test_app("detail-swallow");
        add_solve(&mut app, 10_000);
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, Some(0));

        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle, "space must not arm behind the overlay");
        app.on_key(release(SPACE));
        assert_eq!(app.state, TimerState::Idle);

        app.on_key(press(KeyCode::Char('q')));
        assert!(!app.should_quit, "q must not quit behind the overlay");

        app.on_key(press(KeyCode::Char('/')));
        assert_eq!(app.input_mode, InputMode::Normal, "'/' must not open command mode");

        let scramble = app.scramble.clone();
        app.on_key(press(KeyCode::Char('n')));
        assert_eq!(app.scramble, scramble, "'n' must not re-scramble");

        app.on_key(press(KeyCode::Down));
        assert_eq!(app.times_selected, 0, "the cursor does not move behind the overlay");
        assert_eq!(app.solve_detail, Some(0), "and the overlay is still open");

        // Ctrl-C is checked before the overlay, so it is still the escape hatch.
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn esc_and_enter_close_the_overlay() {
        let (mut app, _g) = test_app("detail-close");
        add_solve(&mut app, 10_000);

        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, Some(0));
        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.solve_detail, None, "Esc closes it");

        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, Some(0));
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, None, "Enter closes it too");

        // Closing with Esc leaves the help overlay and the status alone.
        assert!(!app.show_help);
    }

    #[test]
    fn r_recalls_the_scramble_of_the_open_solve() {
        let (mut app, _g) = test_app("detail-recall");
        for i in 1..=3 {
            add_solve_with(&mut app, 10_000 + i, &format!("SCRAMBLE {}", i));
        }

        // One step older than the newest: index 1, which the list numbers 2 of 3.
        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.solve_detail, Some(1));

        app.on_key(press(KeyCode::Char('r')));
        assert_eq!(app.scramble, "SCRAMBLE 2");
        assert_eq!(app.solve_detail, None, "recalling closes the overlay");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("scramble loaded from solve 2")
        );

        // The next solve is recorded against the recalled scramble.
        start_timing_now(&mut app);
        app.state = TimerState::Timing {
            started: ago(ms(4_000)),
        };
        app.on_key(press(KeyCode::Char('x')));
        let last = app.current_session().solves.last().expect("solve recorded");
        assert_eq!(last.scramble, "SCRAMBLE 2");
        assert_eq!(app.times_selected, 0);
        assert_ne!(app.scramble, "SCRAMBLE 2", "and a fresh scramble follows it");
    }

    #[test]
    fn the_newest_solve_recalls_under_its_own_number() {
        let (mut app, _g) = test_app("detail-recall-newest");
        for i in 1..=3 {
            add_solve_with(&mut app, 10_000 + i, &format!("SCRAMBLE {}", i));
        }
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('r')));
        assert_eq!(app.scramble, "SCRAMBLE 3");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("scramble loaded from solve 3"),
            "the newest solve is numbered by the solve count"
        );
    }

    // ------------------------------------------------------------ constructor

    #[test]
    fn new_picks_up_the_active_sessions_puzzle_for_the_first_scramble() {
        let save = SaveFile {
            active_session_id: Puzzle::Cube2.default_session_id(),
            ..SaveFile::default()
        };
        let (app, _g) = test_app_with("ctor-puzzle", save);
        assert_eq!(app.current_session().puzzle, Puzzle::Cube2);
        let moves = app.scramble.split_whitespace().count();
        assert!((9..=11).contains(&moves), "2x2 scramble was {moves} moves");
    }

    #[test]
    fn a_loaded_file_decides_whether_inspection_is_on() {
        let mut save = SaveFile::default();
        save.settings.inspection = true;
        let (mut app, _g) = test_app_with("ctor-inspection-on", save);
        assert!(app.save.settings.inspection);

        // The timer flow follows it without any command being run.
        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle, "press alone must not arm");
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
    }

    // ------------------------------------------------------- derived statistics

    /// Assert the cached statistics still equal a fresh computation over the same data.
    fn assert_cache_is_fresh(app: &App, when: &str) {
        let fresh = stats::session_stats(&app.current_session().solves);
        assert_eq!(app.stats, fresh, "app.stats went stale {}", when);

        let puzzle = app.current_session().puzzle;
        let of_puzzle: Vec<&Session> = app
            .save
            .sessions
            .iter()
            .filter(|s| s.puzzle == puzzle)
            .collect();
        assert_eq!(
            app.pbs,
            stats::personal_bests(&of_puzzle),
            "app.pbs went stale {}",
            when
        );
    }

    /// Record a solve through the real path, then clear the guards a user clears by waiting.
    fn perform_solve(app: &mut App) {
        start_timing_now(app);
        app.on_key(press(KeyCode::Char('x')));
        app.on_key(release(KeyCode::Char('x')));
        app.stopped_at = None;
    }

    #[test]
    fn the_stats_cache_starts_fresh_and_follows_every_solve() {
        let (mut app, _g) = test_app("cache-solve");
        assert_cache_is_fresh(&app, "on a brand new app");
        assert_eq!(app.stats.count, 0);

        for n in 1..=6 {
            perform_solve(&mut app);
            assert_eq!(app.stats.count, n, "the cache counts every solve");
            assert_cache_is_fresh(&app, "after finishing a solve");
        }
        assert!(
            matches!(app.stats.ao5, crate::stats::AvgResult::Time(_)),
            "six solves is enough for an ao5, got {:?}",
            app.stats.ao5
        );
        assert!(app.pbs.single.is_some(), "a solve sets a PB single");
    }

    #[test]
    fn the_stats_cache_follows_a_penalty_change() {
        let (mut app, _g) = test_app("cache-penalty");
        add_solve(&mut app, 10_000);
        add_solve(&mut app, 20_000);
        run_command(&mut app, "ok"); // Any penalty command re-derives the cache.
        assert_cache_is_fresh(&app, "after seeding solves");
        assert_eq!(app.stats.worst, Some(20_000));

        run_command(&mut app, "+2");
        assert_eq!(app.stats.worst, Some(22_000), "+2 lands in the cache");
        assert_cache_is_fresh(&app, "after /+2");

        run_command(&mut app, "dnf");
        assert_eq!(app.stats.valid_count, 1, "a DNF drops out of the valid count");
        assert_eq!(app.stats.worst, Some(10_000));
        assert_cache_is_fresh(&app, "after /dnf");

        run_command(&mut app, "ok");
        assert_eq!(app.stats.worst, Some(20_000));
        assert_cache_is_fresh(&app, "after /ok");
    }

    #[test]
    fn the_stats_cache_follows_a_deletion() {
        let (mut app, _g) = test_app("cache-delete");
        for ms in [10_000, 11_000, 30_000] {
            add_solve(&mut app, ms);
        }
        run_command(&mut app, "del");
        assert_eq!(app.stats.count, 2, "the deleted solve leaves the cache");
        assert_eq!(app.stats.worst, Some(11_000));
        assert_cache_is_fresh(&app, "after /del");

        // A second session of the same puzzle holds the PB, so deleting it must move the PB.
        run_command(&mut app, "new fast");
        for ms in [1_000, 1_100, 1_200, 1_300, 1_400] {
            add_solve(&mut app, ms);
        }
        run_command(&mut app, "del");
        assert_eq!(app.pbs.single, Some(1_000));
        assert_cache_is_fresh(&app, "in the second session");

        run_command(&mut app, "delsession");
        assert_eq!(app.pbs.single, Some(10_000), "the fast session's PB is gone");
        assert_cache_is_fresh(&app, "after /delsession");
    }

    #[test]
    fn the_stats_cache_follows_a_session_switch() {
        let (mut app, _g) = test_app("cache-switch");
        add_solve(&mut app, 10_000);
        run_command(&mut app, "ok");
        assert_eq!(app.stats.count, 1);

        run_command(&mut app, "new evening");
        assert_eq!(app.stats.count, 0, "a new session starts with no stats");
        assert_cache_is_fresh(&app, "after /new");

        run_command(&mut app, "session 1");
        assert_eq!(app.stats.count, 1, "switching back restores the 3x3 stats");
        assert_cache_is_fresh(&app, "after /session 1");

        run_command(&mut app, "2x2");
        assert_eq!(app.stats.count, 0, "2x2 has its own empty default");
        assert_eq!(app.pbs.single, None, "and its own PBs");
        assert_cache_is_fresh(&app, "after /2x2");

        run_command(&mut app, "3x3");
        assert_eq!(app.stats.count, 1);
        assert_cache_is_fresh(&app, "after /3x3");
    }

    #[test]
    fn the_stats_cache_follows_retyping_an_empty_session() {
        let (mut app, _g) = test_app("cache-retype");
        add_solve(&mut app, 10_000);
        run_command(&mut app, "ok");

        // An empty user session retypes in place, which changes which sessions the PBs span.
        run_command(&mut app, "new scratch");
        assert_cache_is_fresh(&app, "in the new 3x3 session");
        assert_eq!(app.pbs.single, Some(10_000), "3x3 PBs still include the default");

        run_command(&mut app, "megaminx");
        assert_eq!(app.current_session().puzzle, Puzzle::Megaminx, "retyped in place");
        assert_eq!(app.pbs.single, None, "Megaminx has no times yet");
        assert_cache_is_fresh(&app, "after retyping onto Megaminx");
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
