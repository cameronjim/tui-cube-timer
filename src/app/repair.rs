//! Structural repair of an already-parsed save file.
//!
//! One question, asked of a hand-edited or truncated file before anything else reads it: is
//! this file internally consistent. Reading an older format is `storage::load`'s job, and by
//! the time any of this runs the file is already at the current version. Nothing here touches
//! the timer.

use std::collections::HashSet;

use crate::types::{Puzzle, SaveFile, Session, FIRST_USER_ID};

/// Restore invariants: ids are unique, each reserved id holds its own puzzle, the twelve
/// defaults exist and sort first, the active id is real, `next_session_id` is free.
pub(super) fn sanitize(save: &mut SaveFile) {
    free_next_id(save);
    dedupe_ids(save);
    evict_misfiled_defaults(save);

    for puzzle in Puzzle::DEFAULT_ORDER {
        if !save
            .sessions
            .iter()
            .any(|s| s.id == puzzle.default_session_id())
        {
            save.sessions.push(Session::default_for(puzzle));
        }
    }
    save.sessions.sort_by_key(|s| s.id);

    free_next_id(save);
    if !save.sessions.iter().any(|s| s.id == save.active_session_id) {
        save.active_session_id = Puzzle::Cube3.default_session_id();
    }
}

/// Raise `next_session_id` clear of every id in use and of the whole reserved range.
fn free_next_id(save: &mut SaveFile) {
    let floor = save
        .sessions
        .iter()
        .map(|s| s.id)
        .max()
        .unwrap_or(0)
        .max(FIRST_USER_ID - 1);
    if save.next_session_id <= floor {
        save.next_session_id = floor.saturating_add(1);
    }
}

/// Hand out the next free user id. Call [`free_next_id`] first on untrusted input.
pub(super) fn take_id(save: &mut SaveFile) -> u64 {
    let id = save.next_session_id;
    save.next_session_id = save.next_session_id.saturating_add(1);
    id
}

/// Give a fresh user id to every session after the first that claims an id already taken.
///
/// Duplicate ids would make `/session <id>`, `/delsession` and the active-session lookup
/// resolve to whichever copy came first, silently orphaning the rest.
fn dedupe_ids(save: &mut SaveFile) {
    let mut seen: HashSet<u64> = HashSet::new();
    for i in 0..save.sessions.len() {
        let id = save.sessions[i].id;
        if seen.insert(id) {
            continue;
        }
        let fresh = take_id(save);
        save.sessions[i].id = fresh;
        seen.insert(fresh);
    }
}

/// Move any session holding a reserved id that is not its own puzzle's out to a user id.
///
/// The default it displaced is recreated by [`sanitize`], so `/megaminx` always lands on a
/// Megaminx session rather than on whatever an edited file parked at id 9. The session keeps
/// its solves, and the active id follows it.
fn evict_misfiled_defaults(save: &mut SaveFile) {
    for i in 0..save.sessions.len() {
        let session = &save.sessions[i];
        if session.id >= FIRST_USER_ID || session.id == session.puzzle.default_session_id() {
            continue;
        }
        let old = session.id;
        let fresh = take_id(save);
        save.sessions[i].id = fresh;
        if save.active_session_id == old {
            save.active_session_id = fresh;
        }
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::super::testkit::test_app_with;
    use super::super::TimerState;
    use crate::types::{Penalty, Puzzle, SaveFile, Session, Solve, FIRST_USER_ID};

    #[test]
    fn new_repairs_a_broken_save_file() {
        let broken = SaveFile {
            next_session_id: 1,
            sessions: Vec::new(),
            active_session_id: 42,
            ..SaveFile::default()
        };
        let (app, _g) = test_app_with("ctor-empty", broken);
        assert_eq!(app.save.sessions.len(), Puzzle::DEFAULT_ORDER.len());
        assert_eq!(app.save.active_session_id, 1);
        assert_eq!(app.save.next_session_id, FIRST_USER_ID);
        assert_eq!(app.state, TimerState::Idle);
        assert!(!app.save.settings.inspection, "inspection defaults to off");
        assert!(!app.scramble.is_empty());
    }

    #[test]
    fn new_restores_the_twelve_defaults_and_keeps_user_sessions() {
        let save = SaveFile {
            next_session_id: 2,
            sessions: vec![
                Session {
                    id: FIRST_USER_ID,
                    name: "evening".to_string(),
                    puzzle: Puzzle::Cube4,
                    solves: Vec::new(),
                    created_at: 1,
                },
                Session {
                    id: 1,
                    name: "default".to_string(),
                    puzzle: Puzzle::Cube3,
                    solves: vec![Solve {
                        millis: 9_000,
                        penalty: Penalty::None,
                        scramble: "R U".to_string(),
                        timestamp: 1,
                    }],
                    created_at: 0,
                },
            ],
            active_session_id: 99,
            ..SaveFile::default()
        };
        let (app, _g) = test_app_with("ctor-defaults", save);

        let ids: Vec<u64> = app.save.sessions.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13],
            "defaults exist and sort first"
        );
        assert_eq!(app.save.sessions[0].solves.len(), 1, "existing solves survive");
        assert_eq!(app.save.sessions[3].puzzle, Puzzle::Cube5, "id 4 is the 5x5 default");
        assert_eq!(app.save.sessions[6].puzzle, Puzzle::Pyraminx, "id 7 is Pyraminx");
        assert_eq!(app.save.sessions[11].puzzle, Puzzle::Oh, "id 12 is one-handed");
        assert_eq!(app.save.sessions[12].name, "evening");
        assert_eq!(app.save.next_session_id, 14, "past every id in use");
        assert_eq!(app.save.active_session_id, 1, "a dangling active id falls back to 3x3");
    }

    #[test]
    fn new_renumbers_sessions_that_share_an_id() {
        // Three sessions all claiming the first user id: only the first may keep it.
        let dup = |name: &str, puzzle: Puzzle, millis: u64| Session {
            id: FIRST_USER_ID,
            name: name.to_string(),
            puzzle,
            solves: vec![Solve {
                millis,
                penalty: Penalty::None,
                scramble: "R U".to_string(),
                timestamp: 1,
            }],
            created_at: 1,
        };
        let save = SaveFile {
            next_session_id: FIRST_USER_ID,
            sessions: vec![
                dup("first", Puzzle::Cube3, 9_000),
                dup("second", Puzzle::Cube2, 3_000),
                dup("third", Puzzle::Skewb, 7_000),
            ],
            active_session_id: FIRST_USER_ID,
            ..SaveFile::default()
        };
        let (app, _g) = test_app_with("ctor-dupes", save);

        let ids: Vec<u64> = app.save.sessions.iter().map(|s| s.id).collect();
        assert_eq!(ids, (1..=15).collect::<Vec<u64>>(), "every id is distinct");
        let named: Vec<(u64, &str)> = app
            .save
            .sessions
            .iter()
            .filter(|s| s.id >= FIRST_USER_ID)
            .map(|s| (s.id, s.name.as_str()))
            .collect();
        assert_eq!(
            named,
            vec![(13, "first"), (14, "second"), (15, "third")],
            "the first occurrence keeps the id, the rest move up in order"
        );
        for session in app.save.sessions.iter().filter(|s| s.id >= FIRST_USER_ID) {
            assert_eq!(session.solves.len(), 1, "renumbering keeps the solves");
        }
        assert_eq!(app.save.next_session_id, 16, "past every id now in use");
        assert_eq!(
            app.save.active_session_id, FIRST_USER_ID,
            "the session that kept the id stays active"
        );
    }

    #[test]
    fn new_evicts_a_session_squatting_on_another_puzzles_reserved_id() {
        // Id 9 is Megaminx's, but this file parked a 4x4 session there.
        let save = SaveFile {
            next_session_id: FIRST_USER_ID,
            sessions: vec![Session {
                id: 9,
                name: "misfiled".to_string(),
                puzzle: Puzzle::Cube4,
                solves: vec![Solve {
                    millis: 7_000,
                    penalty: Penalty::None,
                    scramble: "R U".to_string(),
                    timestamp: 1,
                }],
                created_at: 5,
            }],
            active_session_id: 9,
            ..SaveFile::default()
        };
        let (app, _g) = test_app_with("ctor-squatter", save);

        let nine = app
            .save
            .sessions
            .iter()
            .find(|s| s.id == 9)
            .expect("id 9 exists again");
        assert_eq!(nine.puzzle, Puzzle::Megaminx, "the proper default is recreated");
        assert_eq!(nine.name, "default");
        assert!(nine.solves.is_empty());

        let moved = app
            .save
            .sessions
            .iter()
            .find(|s| s.name == "misfiled")
            .expect("the squatter survives");
        assert_eq!(moved.id, FIRST_USER_ID, "it moves to a fresh user id");
        assert_eq!(moved.puzzle, Puzzle::Cube4, "with its puzzle and solves intact");
        assert_eq!(moved.solves.len(), 1);
        assert_eq!(
            app.save.active_session_id,
            FIRST_USER_ID,
            "the active id follows the session it pointed at"
        );
        assert_eq!(app.current_session().puzzle, Puzzle::Cube4);
    }

    #[test]
    fn new_evicts_a_session_holding_an_id_no_puzzle_reserves() {
        // Zero is not a legal id, so the session is renumbered out of the reserved range.
        let save = SaveFile {
            next_session_id: 1,
            sessions: vec![Session {
                id: 0,
                name: "zero".to_string(),
                puzzle: Puzzle::Clock,
                solves: Vec::new(),
                created_at: 1,
            }],
            active_session_id: 0,
            ..SaveFile::default()
        };
        let (app, _g) = test_app_with("ctor-zero", save);
        assert!(
            app.save.sessions.iter().all(|s| s.id > 0),
            "no session keeps id 0"
        );
        let moved = app
            .save
            .sessions
            .iter()
            .find(|s| s.name == "zero")
            .expect("the session survives");
        assert!(moved.id >= FIRST_USER_ID);
        assert_eq!(app.save.active_session_id, moved.id);
    }
}
