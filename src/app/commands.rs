//! Command mode: the `/command` line, the parser that dispatches it, and one `cmd_*` handler
//! per command.
//!
//! Every handler here mutates [`App`] exactly as the key handlers do, and funnels its writes
//! through `save_now`. The timer state machine stays in [`super`].

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::repair;
use super::{App, InputMode};
use crate::storage;
use crate::types::{Penalty, Puzzle, Session};

impl App {
    /// The one entry point the state machine calls; everything below it stays in this file.
    pub(super) fn on_command_key(&mut self, key: KeyEvent) {
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
            "delsession" => self.cmd_delete_session(rest),
            "del" | "delete" => self.cmd_delete_solve(rest),
            "dnf" => self.cmd_set_penalty(Penalty::Dnf),
            "+2" => self.cmd_set_penalty(Penalty::Plus2),
            "ok" => self.cmd_set_penalty(Penalty::None),
            "inspect" => self.cmd_toggle_inspection(),
            "hidetime" => self.cmd_toggle_hide_time(),
            "help" => self.toggle_help(),
            "quit" | "q" => self.should_quit = true,
            _ => self.status(format!("unknown command: {}", cmd)),
        }
    }

    fn cmd_switch_puzzle(&mut self, puzzle: Puzzle) {
        // An empty session someone created themselves isn't committed to a puzzle yet: retype it in place.
        let current = self.current_session();
        if !current.is_default() && current.solves.is_empty() {
            let session = self.current_session_mut();
            session.puzzle = puzzle;
            let name = session.name.clone();
            self.new_scramble();
            self.refresh_derived();
            self.status(format!("session '{}' is now {}", name, puzzle.name()));
            self.save_now();
            return;
        }

        // Otherwise navigate, never retype: the puzzle's permanent default is always the destination.
        self.save.active_session_id = puzzle.default_session_id();
        self.new_scramble();
        self.refresh_derived();
        let name = self.current_session().name.clone();
        self.status(format!("{} · session: {}", puzzle.name(), name));
        self.save_now();
    }

    /// Create a session, returning its id. Does not change the active session.
    fn push_session(&mut self, name: String, puzzle: Puzzle) -> u64 {
        let id = repair::take_id(&mut self.save);
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
        self.refresh_derived();
        self.status(format!("new session: {} (#{})", name, id));
        self.save_now();
    }

    /// Open the sessions overlay. The listing outgrew the one-line status bar at twelve
    /// permanent defaults, so it is a popup and `ui` does the formatting.
    fn cmd_list_sessions(&mut self) {
        self.show_sessions = true;
        self.show_help = false;
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
        self.refresh_derived();
        let s = self.current_session();
        let msg = format!("session: {} ({})", s.name, s.puzzle.name());
        self.status(msg);
        self.save_now();
    }

    fn cmd_rename(&mut self, rest: &str) {
        if self.current_session().is_default() {
            self.status("default sessions cannot be renamed");
            return;
        }
        if rest.is_empty() {
            self.status("usage: /rename <name>");
            return;
        }
        let name = rest.to_string();
        self.current_session_mut().name = name.clone();
        self.status(format!("renamed session to {}", name));
        self.save_now();
    }

    /// Delete a whole session and its solves. No argument means the one you are in.
    fn cmd_delete_session(&mut self, rest: &str) {
        let id = if rest.is_empty() {
            self.save.active_session_id
        } else {
            match rest.trim().parse::<u64>() {
                Ok(id) => id,
                Err(_) => {
                    self.status(format!("not a session id: {}", rest));
                    return;
                }
            }
        };
        let Some(index) = self.save.sessions.iter().position(|s| s.id == id) else {
            self.status(format!("no session with id {}", id));
            return;
        };
        if self.save.sessions[index].is_default() {
            self.status("default sessions cannot be deleted");
            return;
        }

        let removed = self.save.sessions.remove(index);
        if self.save.active_session_id == removed.id {
            self.save.active_session_id = removed.puzzle.default_session_id();
            self.new_scramble();
        }
        self.refresh_derived();
        self.status(format!("deleted session: {} (#{})", removed.name, removed.id));
        self.save_now();
    }

    /// Delete one solve. No argument means the newest; an argument is the number the
    /// times list shows, where the oldest solve is 1 and the newest is the solve count.
    fn cmd_delete_solve(&mut self, rest: &str) {
        let count = self.current_session().solves.len();
        if count == 0 {
            self.status("no solves to delete");
            return;
        }
        let index = if rest.is_empty() {
            count - 1
        } else {
            match rest.trim().parse::<usize>() {
                Ok(n) if (1..=count).contains(&n) => n - 1,
                Ok(n) => {
                    self.status(format!("no solve numbered {}", n));
                    return;
                }
                Err(_) => {
                    self.status(format!("not a solve number: {}", rest));
                    return;
                }
            }
        };

        let removed = self.current_session_mut().solves.remove(index);
        // Every index from the newest has just shifted, so neither the cursor nor an
        // overlay opened on one of them still means what it did.
        self.times_selected = 0;
        self.solve_detail = None;
        self.refresh_derived();
        let msg = format!("deleted {}", crate::types::format_solve(&removed));
        self.status(msg);
        self.save_now();
    }

    fn cmd_toggle_inspection(&mut self) {
        self.save.settings.inspection = !self.save.settings.inspection;
        let s = if self.save.settings.inspection {
            "on"
        } else {
            "off"
        };
        self.status(format!("inspection: {}", s));
        self.save_now();
    }

    fn cmd_toggle_hide_time(&mut self) {
        self.save.settings.hide_time = !self.save.settings.hide_time;
        let s = if self.save.settings.hide_time {
            "on"
        } else {
            "off"
        };
        self.status(format!("timer hidden while solving: {}", s));
        self.save_now();
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
                self.refresh_derived();
                self.status(format!("last solve: {}", text));
                self.save_now();
            }
            None => self.status("no solves yet"),
        }
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::super::TimerState;
    use super::*;
    use crate::types::FIRST_USER_ID;

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
    fn puzzle_commands_navigate_to_the_puzzles_default_session() {
        let (mut app, _g) = test_app("cmd-puzzle");
        let before = app.scramble.clone();
        assert_eq!(app.current_session().id, 1, "a fresh file starts on 3x3");

        run_command(&mut app, "3x3");
        assert_eq!(app.current_session().id, 1, "already on the 3x3 default");
        assert_ne!(app.scramble, before, "a puzzle switch re-scrambles");

        run_command(&mut app, "2x2");
        assert_eq!(app.current_session().id, Puzzle::Cube2.default_session_id());
        assert_eq!(app.current_session().puzzle, Puzzle::Cube2);
        assert_eq!(app.current_session().name, "default");
        assert_eq!(
            app.save.sessions.len(),
            Puzzle::DEFAULT_ORDER.len(),
            "navigation never creates a session"
        );
        let moves = app.scramble.split_whitespace().count();
        assert!(
            (9..=11).contains(&moves),
            "2x2 scramble should be 9-11 moves, got {:?}",
            app.scramble
        );
        assert_eq!(app.status_msg.as_deref(), Some("2x2 · session: default"));

        run_command(&mut app, "7x7");
        assert_eq!(app.current_session().id, Puzzle::Cube7.default_session_id());

        let loaded = storage::load(&app.data_path).expect("switching persists");
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len());
        assert_eq!(loaded.active_session_id, Puzzle::Cube7.default_session_id());
    }

    #[test]
    fn a_puzzle_command_goes_to_the_default_not_the_newest_session() {
        let (mut app, _g) = test_app("cmd-puzzle-default-wins");
        run_command(&mut app, "2x2");
        run_command(&mut app, "new mini");
        let mini = app.current_session().id;
        assert_eq!(mini, FIRST_USER_ID, "user sessions start above the defaults");
        add_solve(&mut app, 3_000);

        // Leaving a user session that has solves lands on the target puzzle's default.
        run_command(&mut app, "3x3");
        assert_eq!(app.current_session().id, 1);
        let kept = app
            .save
            .sessions
            .iter()
            .find(|s| s.id == mini)
            .expect("mini");
        assert_eq!(kept.solves.len(), 1, "the session it left keeps its solves");
        assert_eq!(kept.puzzle, Puzzle::Cube2, "and its puzzle");

        run_command(&mut app, "2x2");
        assert_eq!(
            app.current_session().id,
            Puzzle::Cube2.default_session_id(),
            "the 2x2 default wins over the newer 2x2 session"
        );
    }

    #[test]
    fn a_puzzle_command_retypes_an_empty_user_session_in_place() {
        let (mut app, _g) = test_app("cmd-puzzle-retype");
        run_command(&mut app, "new evening");
        let id = app.current_session().id;
        let count = app.save.sessions.len();
        assert!(app.current_session().solves.is_empty());

        run_command(&mut app, "6x6");

        assert_eq!(app.save.sessions.len(), count, "no new session is created");
        assert_eq!(app.current_session().id, id, "same session, new puzzle");
        assert_eq!(app.current_session().name, "evening", "name is kept");
        assert_eq!(app.current_session().puzzle, Puzzle::Cube6);
        assert_eq!(
            app.status_msg.as_deref(),
            Some("session 'evening' is now 6x6")
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
        let session = loaded
            .sessions
            .iter()
            .find(|s| s.id == id)
            .expect("the retyped session survives");
        assert_eq!(session.puzzle, Puzzle::Cube6);
    }

    #[test]
    fn the_five_non_cube_events_navigate_to_their_own_default_sessions() {
        let (mut app, _g) = test_app("cmd-wca-events");
        for puzzle in [
            Puzzle::Pyraminx,
            Puzzle::Skewb,
            Puzzle::Megaminx,
            Puzzle::Square1,
            Puzzle::Clock,
        ] {
            run_command(&mut app, puzzle.name());
            assert_eq!(app.current_session().puzzle, puzzle);
            assert_eq!(
                app.current_session().id,
                puzzle.default_session_id(),
                "/{} must land on its default",
                puzzle.name()
            );
            assert_eq!(app.current_session().name, "default");
            assert_eq!(
                app.status_msg.as_deref(),
                Some(format!("{} · session: default", puzzle.name()).as_str())
            );
            assert!(!app.scramble.is_empty());
        }
        assert_eq!(
            app.save.sessions.len(),
            Puzzle::DEFAULT_ORDER.len(),
            "navigation never creates a session"
        );

        let loaded = storage::load(&app.data_path).expect("switching persists");
        assert_eq!(loaded.active_session_id, Puzzle::Clock.default_session_id());
    }

    #[test]
    fn the_newer_events_sit_on_the_reserved_ids_seven_through_twelve() {
        let (mut app, _g) = test_app("cmd-wca-ids");
        run_command(&mut app, "pyra");
        assert_eq!(app.current_session().id, 7);
        run_command(&mut app, "skewb");
        assert_eq!(app.current_session().id, 8);
        run_command(&mut app, "megaminx");
        assert_eq!(app.current_session().id, 9);
        run_command(&mut app, "sq1");
        assert_eq!(app.current_session().id, 10);
        run_command(&mut app, "clock");
        assert_eq!(app.current_session().id, 11);
        run_command(&mut app, "oh");
        assert_eq!(app.current_session().id, 12);
        assert_eq!(app.current_session().puzzle, Puzzle::Oh);
    }

    #[test]
    fn every_alias_reaches_the_same_default_session() {
        let (mut app, _g) = test_app("cmd-wca-aliases");
        for (alias, puzzle) in [
            ("pyra", Puzzle::Pyraminx),
            ("PYRAMINX", Puzzle::Pyraminx),
            ("Skewb", Puzzle::Skewb),
            ("mega", Puzzle::Megaminx),
            ("MEGAMINX", Puzzle::Megaminx),
            ("square1", Puzzle::Square1),
            ("square-1", Puzzle::Square1),
            ("SQ1", Puzzle::Square1),
            ("Clock", Puzzle::Clock),
        ] {
            run_command(&mut app, alias);
            assert_eq!(
                app.current_session().id,
                puzzle.default_session_id(),
                "/{alias} should be {}",
                puzzle.name()
            );
        }

        run_command(&mut app, "pyraminx2");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("unknown command: pyraminx2")
        );
    }

    #[test]
    fn an_empty_user_session_retypes_onto_a_new_event() {
        let (mut app, _g) = test_app("cmd-retype-skewb");
        run_command(&mut app, "new evening");
        let id = app.current_session().id;
        let count = app.save.sessions.len();

        run_command(&mut app, "skewb");

        assert_eq!(app.save.sessions.len(), count, "no new session is created");
        assert_eq!(app.current_session().id, id, "same session, new puzzle");
        assert_eq!(app.current_session().puzzle, Puzzle::Skewb);
        assert_eq!(
            app.status_msg.as_deref(),
            Some("session 'evening' is now skewb")
        );
        assert!(!app.scramble.is_empty(), "retyping re-scrambles");

        let loaded = storage::load(&app.data_path).expect("retyping persists");
        let session = loaded
            .sessions
            .iter()
            .find(|s| s.id == id)
            .expect("the retyped session survives");
        assert_eq!(session.puzzle, Puzzle::Skewb);
    }

    #[test]
    fn a_puzzle_command_never_retypes_a_default_session() {
        let (mut app, _g) = test_app("cmd-puzzle-default-pinned");
        assert!(app.current_session().solves.is_empty());

        // Empty or not, a default session is navigated away from, never retyped.
        run_command(&mut app, "6x6");
        assert_eq!(app.save.sessions[0].puzzle, Puzzle::Cube3);
        assert_eq!(app.current_session().id, Puzzle::Cube6.default_session_id());
        assert_eq!(app.status_msg.as_deref(), Some("6x6 · session: default"));

        run_command(&mut app, "3x3");
        add_solve(&mut app, 12_000);
        run_command(&mut app, "6x6");
        assert_eq!(app.save.sessions[0].puzzle, Puzzle::Cube3, "puzzle kept");
        assert_eq!(app.save.sessions[0].solves.len(), 1, "solves kept");
        assert_eq!(app.current_session().id, Puzzle::Cube6.default_session_id());
        assert_eq!(
            app.save.sessions.len(),
            Puzzle::DEFAULT_ORDER.len(),
            "no session was created along the way"
        );
    }

    #[test]
    fn delsession_refuses_the_default_sessions() {
        let (mut app, _g) = test_app("cmd-delsession-default");
        run_command(&mut app, "delsession");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("default sessions cannot be deleted")
        );

        run_command(&mut app, "delsession 6");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("default sessions cannot be deleted")
        );
        assert_eq!(app.save.sessions.len(), Puzzle::DEFAULT_ORDER.len());
    }

    #[test]
    fn delsession_rejects_unknown_and_malformed_ids() {
        let (mut app, _g) = test_app("cmd-delsession-bad");
        run_command(&mut app, "delsession 99");
        assert_eq!(app.status_msg.as_deref(), Some("no session with id 99"));

        run_command(&mut app, "delsession abc");
        assert_eq!(app.status_msg.as_deref(), Some("not a session id: abc"));
        assert_eq!(app.save.sessions.len(), Puzzle::DEFAULT_ORDER.len());
    }

    #[test]
    fn delsession_removes_a_user_session_and_its_solves() {
        let (mut app, _g) = test_app("cmd-delsession");
        run_command(&mut app, "new evening");
        let id = app.current_session().id;
        add_solve(&mut app, 12_000);
        run_command(&mut app, "session 1");

        run_command(&mut app, &format!("delsession {}", id));
        assert_eq!(
            app.status_msg.as_deref(),
            Some(format!("deleted session: evening (#{})", id).as_str())
        );
        assert_eq!(app.save.sessions.len(), Puzzle::DEFAULT_ORDER.len());
        assert_eq!(app.save.active_session_id, 1, "the active session is untouched");

        let loaded = storage::load(&app.data_path).expect("deletion persists");
        assert!(!loaded.sessions.iter().any(|s| s.id == id));
    }

    #[test]
    fn deleting_the_active_session_falls_back_to_the_puzzle_default() {
        let (mut app, _g) = test_app("cmd-delsession-active");
        run_command(&mut app, "5x5");
        run_command(&mut app, "new evening");
        add_solve(&mut app, 60_000);
        let before = app.scramble.clone();

        run_command(&mut app, "delsession");

        assert_eq!(
            app.save.active_session_id,
            Puzzle::Cube5.default_session_id()
        );
        assert_eq!(app.current_session().puzzle, Puzzle::Cube5);
        assert!(app.current_session().solves.is_empty());
        assert_ne!(app.scramble, before, "landing somewhere else re-scrambles");
        assert_eq!(app.save.sessions.len(), Puzzle::DEFAULT_ORDER.len());

        let loaded = storage::load(&app.data_path).expect("deletion persists");
        assert_eq!(loaded.active_session_id, Puzzle::Cube5.default_session_id());
    }

    #[test]
    fn new_creates_a_session_for_the_current_puzzle() {
        let (mut app, _g) = test_app("cmd-new");
        add_solve(&mut app, 1_000);

        run_command(&mut app, "new");
        assert_eq!(app.save.sessions.len(), Puzzle::DEFAULT_ORDER.len() + 1);
        assert_eq!(
            app.current_session().id,
            FIRST_USER_ID,
            "user ids start above the defaults"
        );
        assert_eq!(app.current_session().name, "session 2");
        assert_eq!(app.current_session().puzzle, Puzzle::Cube3);
        assert!(app.current_session().solves.is_empty());
        assert!(!app.current_session().is_default());

        run_command(&mut app, "new one-handed");
        assert_eq!(app.save.sessions.len(), Puzzle::DEFAULT_ORDER.len() + 2);
        assert_eq!(app.current_session().name, "one-handed");
        assert_eq!(app.current_session().id, FIRST_USER_ID + 1);

        let loaded = storage::load(&app.data_path).expect("new session persists");
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len() + 2);
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
    fn sessions_opens_the_overlay_instead_of_filling_the_status_line() {
        let (mut app, _g) = test_app("cmd-sessions");
        add_solve(&mut app, 1_234);
        run_command(&mut app, "new second");

        run_command(&mut app, "sessions");
        assert!(app.show_sessions, "the listing is a popup now");
        assert!(
            app.status_msg.is_none(),
            "twelve defaults do not fit on one line, so nothing goes there: {:?}",
            app.status_msg
        );
        assert_eq!(
            app.save.sessions.len(),
            Puzzle::DEFAULT_ORDER.len() + 1,
            "listing them creates or removes nothing"
        );
    }

    #[test]
    fn the_sessions_and_help_overlays_are_alternatives() {
        let (mut app, _g) = test_app("cmd-sessions-help");
        run_command(&mut app, "help");
        assert!(app.show_help);

        run_command(&mut app, "sessions");
        assert!(app.show_sessions, "/sessions opens the listing");
        assert!(!app.show_help, "and closes the help");

        run_command(&mut app, "help");
        assert!(app.show_help, "/help opens the help");
        assert!(!app.show_sessions, "and closes the listing");

        // Toggling the help back off leaves the listing closed rather than restoring it.
        run_command(&mut app, "help");
        assert!(!app.show_help);
        assert!(!app.show_sessions);
    }

    #[test]
    fn rename_renames_the_current_session() {
        let (mut app, _g) = test_app("cmd-rename");
        run_command(&mut app, "new");
        let id = app.current_session().id;

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
        let session = loaded
            .sessions
            .iter()
            .find(|s| s.id == id)
            .expect("session");
        assert_eq!(session.name, "evening practice");
    }

    #[test]
    fn rename_refuses_a_default_session() {
        let (mut app, _g) = test_app("cmd-rename-default");
        run_command(&mut app, "rename evening practice");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("default sessions cannot be renamed")
        );
        assert_eq!(app.current_session().name, "default");

        // The refusal comes before the usage line: the name is not the problem.
        run_command(&mut app, "rename");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("default sessions cannot be renamed")
        );
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
        app.times_selected = 1;

        run_command(&mut app, "del");
        assert_eq!(app.current_session().solves.len(), 1);
        assert_eq!(app.current_session().solves[0].millis, 10_000);
        assert_eq!(app.times_selected, 0);
        assert_eq!(app.status_msg.as_deref(), Some("deleted 12.34"));

        run_command(&mut app, "delete");
        assert!(app.current_session().solves.is_empty());

        run_command(&mut app, "del");
        assert_eq!(app.status_msg.as_deref(), Some("no solves to delete"));

        let loaded = storage::load(&app.data_path).expect("deletion persists");
        assert!(loaded.sessions[0].solves.is_empty());
    }

    #[test]
    fn del_with_a_number_removes_that_line_of_the_times_list() {
        let (mut app, _g) = test_app("cmd-del-n");
        for i in 1..=3 {
            add_solve(&mut app, i * 10_000);
        }
        app.times_selected = 2;

        // The list numbers the oldest solve 1 and the newest 3.
        run_command(&mut app, "del 1");
        assert_eq!(app.status_msg.as_deref(), Some("deleted 10.00"));
        let left: Vec<u64> = app
            .current_session()
            .solves
            .iter()
            .map(|s| s.millis)
            .collect();
        assert_eq!(left, vec![20_000, 30_000], "the oldest solve is the one gone");
        assert_eq!(app.times_selected, 0, "the cursor comes home");

        run_command(&mut app, "del 2");
        assert_eq!(
            app.status_msg.as_deref(),
            Some("deleted 30.00"),
            "2 is now the newest of the two left"
        );
        let left: Vec<u64> = app
            .current_session()
            .solves
            .iter()
            .map(|s| s.millis)
            .collect();
        assert_eq!(left, vec![20_000]);

        let loaded = storage::load(&app.data_path).expect("numbered deletion persists");
        assert_eq!(loaded.sessions[0].solves.len(), 1);
        assert_eq!(loaded.sessions[0].solves[0].millis, 20_000);
    }

    #[test]
    fn del_rejects_a_number_that_is_not_in_the_list() {
        let (mut app, _g) = test_app("cmd-del-bad");
        add_solve(&mut app, 10_000);
        add_solve(&mut app, 20_000);
        app.times_selected = 1;

        for (line, msg) in [
            ("del 0", "no solve numbered 0"),
            ("del 3", "no solve numbered 3"),
            ("del 99", "no solve numbered 99"),
            ("del abc", "not a solve number: abc"),
            ("del -1", "not a solve number: -1"),
        ] {
            run_command(&mut app, line);
            assert_eq!(app.status_msg.as_deref(), Some(msg), "/{line}");
            assert_eq!(
                app.current_session().solves.len(),
                2,
                "/{line} must delete nothing"
            );
        }
        assert_eq!(app.times_selected, 1, "a refused delete leaves the cursor alone");
    }

    #[test]
    fn inspect_toggles_inspection() {
        let (mut app, _g) = test_app("cmd-inspect");
        assert!(!app.save.settings.inspection, "off by default");

        run_command(&mut app, "inspect");
        assert!(app.save.settings.inspection);
        assert_eq!(app.status_msg.as_deref(), Some("inspection: on"));

        // ...and the timer flow follows the new setting.
        app.on_key(press(SPACE));
        assert_eq!(app.state, TimerState::Idle, "press alone must not arm");
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Inspecting { .. }));
        app.on_key(press(KeyCode::Esc));
        assert_eq!(app.state, TimerState::Idle);

        run_command(&mut app, "inspect");
        assert!(!app.save.settings.inspection);
        assert_eq!(app.status_msg.as_deref(), Some("inspection: off"));

        // Back to the default flow: a press arms immediately again.
        app.on_key(press(SPACE));
        assert!(matches!(app.state, TimerState::Armed { .. }));
        app.on_key(release(SPACE));
        assert_eq!(app.state, TimerState::Idle);
    }

    #[test]
    fn hidetime_toggles_and_names_the_setting() {
        let (mut app, _g) = test_app("cmd-hidetime");
        assert!(!app.save.settings.hide_time, "off by default");

        run_command(&mut app, "hidetime");
        assert!(app.save.settings.hide_time);
        assert_eq!(
            app.status_msg.as_deref(),
            Some("timer hidden while solving: on")
        );

        run_command(&mut app, "hidetime");
        assert!(!app.save.settings.hide_time);
        assert_eq!(
            app.status_msg.as_deref(),
            Some("timer hidden while solving: off")
        );
    }

    #[test]
    fn both_toggles_survive_a_save_and_load_round_trip() {
        let (mut app, _g) = test_app("cmd-settings-persist");
        run_command(&mut app, "inspect");
        run_command(&mut app, "hidetime");

        let loaded = storage::load(&app.data_path).expect("settings persist");
        assert!(loaded.settings.inspection, "/inspect reached the file");
        assert!(loaded.settings.hide_time, "/hidetime reached the file");

        // A second run takes the settings from the file rather than hardcoding them.
        let reopened = App::new(loaded, app.data_path.clone());
        assert!(reopened.save.settings.inspection);
        assert!(reopened.save.settings.hide_time);

        run_command(&mut app, "inspect");
        run_command(&mut app, "hidetime");
        let loaded = storage::load(&app.data_path).expect("turning them back off persists");
        assert!(!loaded.settings.inspection);
        assert!(!loaded.settings.hide_time);
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
        assert!(app.save.settings.inspection);
        run_command(&mut app, "2X2");
        assert_eq!(app.current_session().puzzle, Puzzle::Cube2);
    }
}
