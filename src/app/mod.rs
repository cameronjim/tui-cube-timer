//! Timer state machine and key handling; `ui` renders only from [`App`] fields refreshed here.
//!
//! The four clusters that answer their own question live beside this file: [`commands`] runs
//! the `/command` line, [`selection`] owns the times cursor and the two list overlays,
//! [`progress`] owns the trend and the personal-best celebration, and [`repair`] makes a
//! parsed save file internally consistent.

mod commands;
mod progress;
mod repair;
mod selection;
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
    /// Whether the trend graph is open over the frame.
    ///
    /// Non-modal like [`App::show_help`] and mutually exclusive with it and the sessions picker.
    pub show_trend: bool,
    /// Open sessions overlay, holding the cursor's index into `save.sessions`.
    ///
    /// Mutually exclusive with [`App::show_help`] and [`App::show_trend`]; the solve detail wins
    /// over all three.
    pub sessions_overlay: Option<usize>,
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
    /// Effective times of the last [`TREND_LEN`](progress::TREND_LEN) solves of the active
    /// session, oldest first.
    ///
    /// DNFs are dropped, so this is shorter than the window it covers whenever one is in range.
    pub trend: Vec<u64>,
    /// The personal-best celebration currently on screen, cleared after
    /// [`PB_BANNER`](progress::PB_BANNER).
    pub pb_banner: Option<String>,

    // --- internal bookkeeping (not part of the ui contract) ---
    /// Inspection start, kept while armed so an aborted arm restores the countdown.
    inspection_start: Option<Instant>,
    /// Key that stopped the timer: inert until its Release (Windows auto-repeat resends Press, not Repeat).
    inert_key: Option<KeyCode>,
    /// When the last solve was finalized; space is ignored for `STOP_COOLDOWN` after it.
    stopped_at: Option<Instant>,
    /// When the current [`App::pb_banner`] went up; `on_tick` retires it five seconds later.
    banner_since: Option<Instant>,
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
            show_trend: false,
            sessions_overlay: None,
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
            trend: Vec::new(),
            pb_banner: None,
            inspection_start: None,
            inert_key: None,
            stopped_at: None,
            banner_since: None,
        };
        app.refresh_derived();
        app
    }

    /// Recompute the statistics and the trend `ui` renders from.
    ///
    /// They walk every solve of the puzzle, so the 15 ms draw loop must never call them.
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
        let trend = progress::trend_of(&self.current_session().solves);
        self.stats = stats;
        self.pbs = pbs;
        self.trend = trend;
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
        self.inspection_stage = 0;
        self.display_millis = 0;
        self.status_msg = None;
    }

    fn cancel_inspection(&mut self) {
        self.state = TimerState::Idle;
        self.inspection_start = None;
        self.inspection_remaining = None;
        self.pending_inspection_penalty = Penalty::None;
        self.inspection_stage = 0;
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
        self.clear_pb_banner();
        self.inspection_start = None;
        self.inspection_remaining = None;
        self.inspection_stage = 0;
        self.display_millis = 0;
        self.status_msg = None;
    }

    /// Record the running solve, persist, and return to Idle with a fresh scramble.
    fn finish_solve(&mut self, started: Instant) {
        let millis = started.elapsed().as_millis() as u64;
        // The records to beat, read before this solve joins them.
        let (prev_single, prev_ao5) = (self.pbs.single, self.pbs.ao5);
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
        self.note_pb(prev_single, prev_ao5);
        self.save_now();
    }

    /// True while the post-solve cooldown is still swallowing space events.
    fn in_stop_cooldown(&self) -> bool {
        self.stopped_at
            .is_some_and(|when| when.elapsed() < STOP_COOLDOWN)
    }

    // ------------------------------------------------------------------- tick

    pub fn on_tick(&mut self) {
        self.expire_pb_banner();
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

        // The judge calls are silent: `ui` reads the stage for the countdown's colour and caption.
        self.inspection_stage = if elapsed >= INSPECTION_CALL_12_MS {
            2
        } else if elapsed >= INSPECTION_CALL_8_MS {
            1
        } else {
            0
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

        // The solve overlay is modal: nothing behind it may act, Ctrl-C above excepted.
        if self.solve_detail.is_some() {
            if key.kind == KeyEventKind::Press {
                self.on_key_solve_detail(key);
            }
            return;
        }

        // The sessions overlay is modal too, and the solve overlay outranks it.
        if self.sessions_overlay.is_some() {
            if key.kind == KeyEventKind::Press {
                self.on_key_sessions(key);
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
                    if !self.close_popups() {
                        self.status_msg = None;
                    }
                }
                // The times cursor and the detail overlay are selection state, not timer state.
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::Enter
                | KeyCode::Char('k')
                | KeyCode::Char('j') => self.on_key_times(key),
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
    fn the_stage_advances_at_eight_and_twelve_seconds() {
        let (mut app, _g) = test_app("insp-stage");
        app.save.settings.inspection = true;
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        assert_eq!(app.inspection_stage, 0, "inspection starts before the first call");

        set_inspection_started(&mut app, ago(ms(7_900)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 0, "still under eight seconds");

        set_inspection_started(&mut app, ago(ms(8_100)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 1, "the eight second call");

        set_inspection_started(&mut app, ago(ms(11_500)));
        app.on_tick();
        app.on_tick();
        assert_eq!(app.inspection_stage, 1, "later ticks stay in the same stage");

        set_inspection_started(&mut app, ago(ms(12_100)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 2, "the twelve second call");

        set_inspection_started(&mut app, ago(ms(14_000)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 2, "there is no third call");
    }

    #[test]
    fn the_stage_keeps_advancing_while_armed_from_inspection() {
        let (mut app, _g) = test_app("insp-stage-armed");
        app.inspection_start = Some(ago(ms(8_200)));
        app.state = TimerState::Armed {
            since: Instant::now(),
            from_inspection: true,
        };
        app.on_tick();
        assert_eq!(app.inspection_stage, 1, "the clock is still running while armed");
    }

    #[test]
    fn cancelling_inspection_resets_the_stage() {
        let (mut app, _g) = test_app("insp-stage-reset");
        app.save.settings.inspection = true;
        set_inspection_started(&mut app, ago(ms(12_500)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 2);

        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.state, TimerState::Idle);
        assert_eq!(app.inspection_stage, 0);

        // A fresh inspection starts from stage 0 and climbs again.
        app.on_key(press(SPACE));
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        assert_eq!(app.inspection_stage, 0);
        set_inspection_started(&mut app, ago(ms(8_500)));
        app.on_tick();
        assert_eq!(app.inspection_stage, 1);
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
    fn esc_closes_the_trend_graph_before_it_clears_the_status() {
        let (mut app, _g) = test_app("key-esc-trend");
        app.show_trend = true;
        app.status_msg = Some("something".to_string());

        app.on_key(press(KeyCode::Esc));
        assert!(!app.show_trend);
        assert_eq!(app.status_msg.as_deref(), Some("something"));

        app.on_key(press(KeyCode::Esc));
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn h_opens_the_help_over_an_open_trend_graph() {
        let (mut app, _g) = test_app("key-h-trend");
        app.show_trend = true;
        app.on_key(press(KeyCode::Char('h')));
        assert!(app.show_help, "'h' still reaches the help");
        assert!(!app.show_trend, "and the graph gives way to it");
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
