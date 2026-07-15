//! Inspection: the fifteen second countdown, the two judge calls, and the penalty it earns.
//!
//! This is the one part of the timer with a vocabulary of its own, so the five thresholds and
//! the handlers behind them live beside the state machine in [`super`] rather than inside it.
//! The parent calls in at three points: the space release that begins inspection, the tick that
//! advances it, and the keys the `Inspecting` state answers. Nothing above this file needs to
//! know what fifteen seconds costs.

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use super::{App, TimerState};
use crate::types::Penalty;

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

impl App {
    pub(super) fn start_inspection(&mut self) {
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

    pub(super) fn refresh_inspection(&mut self, started: Instant) {
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

    pub(super) fn on_key_inspecting(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char(' ') => self.arm(true),
            KeyCode::Esc => self.cancel_inspection(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use std::time::Duration;

    /// Backdate the running inspection so the next tick sees `started` as its beginning.
    fn set_inspection_started(app: &mut App, started: Instant) {
        app.state = TimerState::Inspecting { started };
        app.inspection_start = Some(started);
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
}
