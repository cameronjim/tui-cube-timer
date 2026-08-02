//! Selection state: the times-list cursor and the four overlays, modal and not.
//!
//! None of this is timer state, so it lives beside the state machine in [`super`] rather than
//! inside it. Every entry point here is reached from a key the state machine did not claim.
//! The mutual exclusion between the three overlays that can be opened outright is enforced
//! here, where the state changes, rather than in the renderer.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::App;

/// How far PageUp and PageDown move the times cursor.
const TIMES_PAGE: usize = 10;
/// How far PageUp and PageDown move the sessions cursor.
const SESSIONS_PAGE: usize = 10;

impl App {
    // ------------------------------------------------- the non-modal overlays

    /// Toggle the help. The overlays are alternatives, so opening one closes the others.
    pub(super) fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.show_trend = false;
            self.sessions_overlay = None;
        }
    }

    /// Toggle the trend graph, on the same terms as the help it displaces.
    pub(super) fn toggle_trend(&mut self) {
        self.show_trend = !self.show_trend;
        if self.show_trend {
            self.show_help = false;
            self.sessions_overlay = None;
        }
    }

    /// Close whichever non-modal overlay is up, reporting whether there was one.
    ///
    /// Esc uses the answer to decide whether it has anything left to do: with no popup open
    /// it goes on to clear the status line instead.
    pub(super) fn close_popups(&mut self) -> bool {
        let was_open = self.show_help || self.show_trend;
        self.show_help = false;
        self.show_trend = false;
        was_open
    }

    // --------------------------------------------------------- the times list

    /// The Idle-mode keys that belong to the times list rather than to the timer.
    pub(super) fn on_key_times(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_newer(1),
            KeyCode::Down | KeyCode::Char('j') => self.select_older(1),
            KeyCode::PageUp => self.select_newer(TIMES_PAGE),
            KeyCode::PageDown => self.select_older(TIMES_PAGE),
            KeyCode::Home => self.times_selected = 0,
            KeyCode::Enter => self.open_solve_detail(),
            _ => {}
        }
    }

    /// Move the cursor toward newer solves; 0 is the newest and it stops there.
    fn select_newer(&mut self, by: usize) {
        self.times_selected = self.times_selected.saturating_sub(by);
    }

    /// Move the cursor toward older solves, clamped to the oldest one in the session.
    fn select_older(&mut self, by: usize) {
        let oldest = self.current_session().solves.len().saturating_sub(1);
        self.times_selected = self.times_selected.saturating_add(by).min(oldest);
    }

    // ---------------------------------------------------- solve detail overlay

    /// Open the detail overlay on the selected solve. An empty session has nothing to show.
    fn open_solve_detail(&mut self) {
        let count = self.current_session().solves.len();
        if count == 0 {
            return;
        }
        self.solve_detail = Some(self.times_selected.min(count - 1));
    }

    pub(super) fn on_key_solve_detail(&mut self, key: KeyEvent) {
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

    // ------------------------------------------------------ sessions overlay

    /// Open the sessions overlay with the cursor on the session you are in.
    pub(super) fn open_sessions_overlay(&mut self) {
        self.sessions_overlay = Some(self.active_index());
        self.show_help = false;
        self.show_trend = false;
    }

    /// Drive the modal sessions list: move the cursor, switch to a session, or close.
    pub(super) fn on_key_sessions(&mut self, key: KeyEvent) {
        let Some(cursor) = self.sessions_overlay else {
            return;
        };
        let last = self.save.sessions.len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.sessions_overlay = Some(cursor.saturating_sub(1))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.sessions_overlay = Some(cursor.saturating_add(1).min(last))
            }
            KeyCode::PageUp => self.sessions_overlay = Some(cursor.saturating_sub(SESSIONS_PAGE)),
            KeyCode::PageDown => {
                self.sessions_overlay = Some(cursor.saturating_add(SESSIONS_PAGE).min(last))
            }
            KeyCode::Home => self.sessions_overlay = Some(0),
            KeyCode::Enter => {
                let target = self.save.sessions.get(cursor).map(|s| s.id);
                self.sessions_overlay = None;
                // Choosing the session you are already in keeps the scramble in front of you.
                if let Some(id) = target.filter(|id| *id != self.save.active_session_id) {
                    self.switch_to_session(id);
                }
            }
            KeyCode::Esc => self.sessions_overlay = None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::super::{InputMode, TimerState};
    use super::*;
    use crate::storage;
    use crate::types::Puzzle;
    use ratatui::crossterm::event::{KeyEvent, KeyModifiers};

    // ------------------------------------------------------ sessions overlay

    /// Add `n` user sessions, then go back to the 3x3 default so index 0 is the active row.
    fn with_user_sessions(app: &mut App, n: usize) {
        for i in 0..n {
            run_command(app, &format!("new session {}", i));
        }
        run_command(app, "session 1");
        app.status_msg = None;
    }

    #[test]
    fn sessions_opens_the_overlay_on_the_active_session() {
        let (mut app, _g) = test_app("sessions-open");
        run_command(&mut app, "sessions");
        assert_eq!(app.sessions_overlay, Some(0), "a fresh file starts on 3x3");
        app.on_key(press(KeyCode::Esc));

        // 3x3 leads the default order, so 5x5's permanent session is the fourth row.
        run_command(&mut app, "5x5");
        run_command(&mut app, "sessions");
        assert_eq!(
            app.sessions_overlay,
            Some(3),
            "the cursor follows the active session, not the top of the list"
        );
        app.on_key(press(KeyCode::Esc));

        run_command(&mut app, "new evening");
        run_command(&mut app, "sessions");
        assert_eq!(
            app.sessions_overlay,
            Some(Puzzle::DEFAULT_ORDER.len()),
            "a session of your own sits after the twelve defaults"
        );
        app.on_key(press(KeyCode::Esc));

        // An active id that matches nothing falls back to the first row rather than vanishing.
        app.save.active_session_id = 9_999;
        run_command(&mut app, "sessions");
        assert_eq!(app.sessions_overlay, Some(0));
    }

    #[test]
    fn the_sessions_cursor_clamps_at_both_ends() {
        let (mut app, _g) = test_app("sessions-cursor");
        with_user_sessions(&mut app, 3);
        let last = Puzzle::DEFAULT_ORDER.len() + 2;
        run_command(&mut app, "sessions");
        assert_eq!(app.sessions_overlay, Some(0));

        app.on_key(press(KeyCode::Down));
        assert_eq!(app.sessions_overlay, Some(1));
        app.on_key(press(KeyCode::Char('j')));
        assert_eq!(app.sessions_overlay, Some(2));
        app.on_key(press(KeyCode::Up));
        assert_eq!(app.sessions_overlay, Some(1));
        app.on_key(press(KeyCode::Char('k')));
        assert_eq!(app.sessions_overlay, Some(0));
        app.on_key(press(KeyCode::Char('k')));
        assert_eq!(app.sessions_overlay, Some(0), "the first row is the top");

        app.on_key(press(KeyCode::PageDown));
        assert_eq!(app.sessions_overlay, Some(10), "PageDown jumps ten rows");
        app.on_key(press(KeyCode::PageDown));
        assert_eq!(app.sessions_overlay, Some(last), "and clamps to the last session");
        app.on_key(press(KeyCode::Down));
        assert_eq!(app.sessions_overlay, Some(last), "so does one row at a time");
        app.on_key(press(KeyCode::PageUp));
        assert_eq!(app.sessions_overlay, Some(last - 10));
        app.on_key(press(KeyCode::PageUp));
        assert_eq!(app.sessions_overlay, Some(0), "PageUp clamps at the first row");

        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Home));
        assert_eq!(app.sessions_overlay, Some(0), "Home returns to the first row");
    }

    #[test]
    fn enter_switches_to_the_session_under_the_cursor_and_closes() {
        let (mut app, _g) = test_app("sessions-enter");
        run_command(&mut app, "new evening");
        let evening = app.current_session().id;
        add_solve(&mut app, 12_000);
        run_command(&mut app, "session 1");
        add_solve(&mut app, 8_000);
        run_command(&mut app, "ok");
        let before = app.scramble.clone();
        app.times_selected = 1;

        run_command(&mut app, "sessions");
        assert_eq!(app.sessions_overlay, Some(0));
        app.on_key(press(KeyCode::PageDown));
        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Down));
        assert_eq!(app.sessions_overlay, Some(Puzzle::DEFAULT_ORDER.len()));

        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.sessions_overlay, None, "choosing a session closes the overlay");
        assert_eq!(app.save.active_session_id, evening);
        assert_eq!(app.status_msg.as_deref(), Some("session: evening (3x3)"));
        assert_ne!(app.scramble, before, "a switch re-scrambles");
        assert_eq!(app.times_selected, 0, "and brings the times cursor home");
        assert_eq!(app.stats.count, 1, "the statistics follow the new session");

        let loaded = storage::load(&app.data_path).expect("the switch persists");
        assert_eq!(loaded.active_session_id, evening);
    }

    #[test]
    fn enter_on_the_session_you_are_in_only_closes_the_overlay() {
        let (mut app, _g) = test_app("sessions-enter-self");
        with_user_sessions(&mut app, 1);
        let before = app.scramble.clone();

        run_command(&mut app, "sessions");
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.sessions_overlay, None);
        assert_eq!(app.save.active_session_id, 1, "still the same session");
        assert_eq!(app.scramble, before, "the scramble in front of you survives");
        assert!(app.status_msg.is_none(), "and nothing is announced");
    }

    #[test]
    fn esc_closes_the_sessions_overlay_without_switching() {
        let (mut app, _g) = test_app("sessions-esc");
        with_user_sessions(&mut app, 2);
        let before = app.scramble.clone();

        run_command(&mut app, "sessions");
        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Down));
        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.sessions_overlay, None);
        assert_eq!(app.save.active_session_id, 1, "Esc chooses nothing");
        assert_eq!(app.scramble, before);
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn the_sessions_overlay_swallows_every_key_but_its_own() {
        let (mut app, _g) = test_app("sessions-modal");
        add_solve(&mut app, 10_000);
        run_command(&mut app, "sessions");
        assert_eq!(app.sessions_overlay, Some(0));

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

        app.on_key(press(KeyCode::Char('h')));
        assert!(!app.show_help, "'h' must not open the help behind the overlay");
        assert_eq!(app.sessions_overlay, Some(0), "and the overlay is still open");

        // Ctrl-C is checked before the overlay, so it is still the escape hatch.
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn the_solve_detail_overlay_outranks_the_sessions_overlay() {
        let (mut app, _g) = test_app("sessions-detail");
        add_solve(&mut app, 10_000);
        run_command(&mut app, "sessions");
        app.solve_detail = Some(0);

        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.solve_detail, None, "the detail overlay takes the key");
        assert_eq!(app.sessions_overlay, Some(0), "the list underneath is untouched");

        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.sessions_overlay, None, "and closes on the next Esc");
    }

    #[test]
    fn help_and_the_sessions_overlay_are_never_open_at_once() {
        let (mut app, _g) = test_app("sessions-help");
        app.on_key(press(KeyCode::Char('h')));
        assert!(app.show_help);

        run_command(&mut app, "sessions");
        assert_eq!(app.sessions_overlay, Some(0), "/sessions opens the list");
        assert!(!app.show_help, "and closes the help");

        // The list is modal, so the help can only come back once it is closed.
        app.on_key(press(KeyCode::Esc));
        app.on_key(press(KeyCode::Char('h')));
        assert!(app.show_help);
        assert_eq!(app.sessions_overlay, None);
        app.on_key(press(KeyCode::Char('?')));
        assert!(!app.show_help, "'?' still closes the help");
    }

    #[test]
    fn the_trend_graph_and_the_sessions_overlay_are_never_open_at_once() {
        let (mut app, _g) = test_app("overlay-trend-exclusive");
        app.show_trend = true;
        app.open_sessions_overlay();
        assert_eq!(app.sessions_overlay, Some(0), "the listing opens");
        assert!(!app.show_trend, "and the graph closes behind it");
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
}
