//! Progress feedback: the trend the sparkline plots and the session-best celebration.
//!
//! Neither is timer state, so both live beside the state machine in [`super`] rather than
//! inside it. The trend is refilled wherever the statistics are, by `refresh_derived`; the
//! banner is raised by the solve that beat a record and retired by the tick that outlives it.
//! Both read the active session alone, so a record belongs to the session that set it.

use std::time::{Duration, Instant};

use super::App;
use crate::stats::SessionBests;
use crate::types::{format_millis, Solve};

/// How many recent solves [`App::trend`] carries.
pub(super) const TREND_LEN: usize = 50;
/// How long a session-best celebration stays on screen.
pub(super) const BEST_BANNER: Duration = Duration::from_secs(5);

/// The tail of `solves` the trend covers, as effective times with the DNFs left out.
pub(super) fn trend_of(solves: &[Solve]) -> Vec<u64> {
    let start = solves.len().saturating_sub(TREND_LEN);
    solves[start..]
        .iter()
        .filter_map(|s| s.effective_millis())
        .collect()
}

/// The improved value when `now` strictly beats an existing `prev`, else None.
///
/// The first best of a session has no record to beat, so it never celebrates.
fn improved(prev: Option<u64>, now: Option<u64>) -> Option<u64> {
    match (prev, now) {
        (Some(p), Some(n)) if n < p => Some(n),
        _ => None,
    }
}

/// The celebration naming every session best the finished solve beat, if it beat one.
fn best_banner_text(
    prev_single: Option<u64>,
    prev_ao5: Option<u64>,
    bests: &SessionBests,
) -> Option<String> {
    let single =
        improved(prev_single, bests.single).map(|t| format!("single: {}", format_millis(t)));
    let ao5 = improved(prev_ao5, bests.ao5).map(|t| format!("ao5: {}", format_millis(t)));
    let parts: Vec<String> = [single, ao5].into_iter().flatten().collect();
    if parts.is_empty() {
        None
    } else {
        Some(format!("new best {}", parts.join(", ")))
    }
}

impl App {
    /// Celebrate the solve just recorded if it beat either record it could, given the two
    /// values those records held before it joined them.
    pub(super) fn note_best(&mut self, prev_single: Option<u64>, prev_ao5: Option<u64>) {
        if let Some(text) = best_banner_text(prev_single, prev_ao5, &self.bests) {
            self.best_banner = Some(text);
            self.banner_since = Some(Instant::now());
        }
    }

    pub(super) fn clear_best_banner(&mut self) {
        self.best_banner = None;
        self.banner_since = None;
    }

    /// Take down a celebration that has had its [`BEST_BANNER`] on screen.
    pub(super) fn expire_best_banner(&mut self) {
        if self.banner_since.is_some_and(|up| up.elapsed() >= BEST_BANNER) {
            self.clear_best_banner();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::super::TimerState;
    use super::*;

    // ------------------------------------------------------------- trend cache

    #[test]
    fn the_trend_holds_the_last_fifty_solves_oldest_first() {
        let (mut app, _g) = test_app("trend-window");
        assert!(app.trend.is_empty(), "no solves, no trend");

        for n in 1..=55u64 {
            add_solve(&mut app, n * 1_000);
        }
        run_command(&mut app, "ok");
        assert_eq!(app.trend.len(), TREND_LEN, "the window is capped");
        assert_eq!(app.trend.first(), Some(&6_000), "it starts at the sixth solve");
        assert_eq!(app.trend.last(), Some(&55_000), "and ends at the newest");
        assert!(app.trend.windows(2).all(|w| w[0] < w[1]), "oldest first");
    }

    #[test]
    fn the_trend_carries_effective_times_and_drops_dnfs() {
        let (mut app, _g) = test_app("trend-penalty");
        for t in [10_000, 11_000, 12_000] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");
        assert_eq!(app.trend, vec![10_000, 11_000, 12_000]);

        run_command(&mut app, "+2");
        assert_eq!(app.trend, vec![10_000, 11_000, 14_000], "a +2 counts as served");

        run_command(&mut app, "dnf");
        assert_eq!(app.trend, vec![10_000, 11_000], "a DNF has no time to plot");
    }

    #[test]
    fn the_trend_follows_a_solve_a_deletion_and_a_session_switch() {
        let (mut app, _g) = test_app("trend-refresh");
        perform_solve_of(&mut app, 7_000);
        let recorded = app.current_session().solves[0].millis;
        assert_eq!(app.trend, vec![recorded], "finishing a solve refreshes the trend");

        add_solve(&mut app, 8_000);
        run_command(&mut app, "ok");
        assert_eq!(app.trend, vec![recorded, 8_000]);

        run_command(&mut app, "del");
        assert_eq!(app.trend, vec![recorded], "the deleted solve leaves the trend");

        run_command(&mut app, "2x2");
        assert!(app.trend.is_empty(), "the 2x2 default has no solves");
        run_command(&mut app, "3x3");
        assert_eq!(app.trend, vec![recorded], "switching back restores it");
    }

    // ----------------------------------------------------------- best banner

    /// The raw time of the newest solve, which is a hair over the one a test asked for.
    fn newest_millis(app: &App) -> u64 {
        app.current_session()
            .solves
            .last()
            .expect("a solve was recorded")
            .millis
    }

    #[test]
    fn beating_an_existing_single_record_raises_the_banner() {
        let (mut app, _g) = test_app("best-single");
        perform_solve_of(&mut app, 20_000);
        assert!(
            app.best_banner.is_none(),
            "the first solve set the record, it beat none"
        );

        perform_solve_of(&mut app, 12_340);
        let recorded = newest_millis(&app);
        assert!(
            (12_340..12_900).contains(&recorded),
            "unexpected raw time {}",
            recorded
        );
        assert_eq!(app.bests.single, Some(recorded));
        let want = format!("new best single: {}", format_millis(recorded));
        assert_eq!(app.best_banner.as_deref(), Some(want.as_str()));
    }

    #[test]
    fn beating_an_existing_ao5_record_raises_the_banner() {
        let (mut app, _g) = test_app("best-ao5");
        // The 30s solves at both ends are trimmed away, so this ao5 is the mean of 10, 10 and 30.
        for t in [30_000, 10_000, 10_000, 10_000, 30_000] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");
        let before = app.bests.ao5.expect("five solves make an ao5 record");
        assert_eq!(app.bests.single, Some(10_000));

        // A sixth 10s solve cannot beat the single, but its window trims both 30s away.
        perform_solve_of(&mut app, 10_000);
        assert_eq!(app.bests.single, Some(10_000), "the single record stands");
        let ao5 = app.bests.ao5.expect("still an ao5 record");
        assert!(ao5 < before, "the ao5 improved from {} to {}", before, ao5);
        let want = format!("new best ao5: {}", format_millis(ao5));
        assert_eq!(app.best_banner.as_deref(), Some(want.as_str()));
    }

    #[test]
    fn beating_both_records_names_both_in_one_banner() {
        let (mut app, _g) = test_app("best-both");
        for t in [30_000, 11_000, 11_000, 11_000, 30_000] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");

        perform_solve_of(&mut app, 10_000);
        let single = app.bests.single.expect("a single record");
        let ao5 = app.bests.ao5.expect("an ao5 record");
        assert!(single < 11_000, "the single improved");
        let want = format!(
            "new best single: {}, ao5: {}",
            format_millis(single),
            format_millis(ao5)
        );
        assert_eq!(app.best_banner.as_deref(), Some(want.as_str()));
    }

    #[test]
    fn the_first_record_of_a_session_is_not_a_celebration() {
        let (mut app, _g) = test_app("best-first");
        perform_solve_of(&mut app, 9_000);
        assert!(app.bests.single.is_some(), "the record is set");
        assert!(
            app.best_banner.is_none(),
            "there was no single record to beat"
        );

        for t in [11_000, 12_000, 13_000] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");
        assert_eq!(app.bests.ao5, None, "four solves are not an ao5");

        // The fifth solve is slower than everything, so only the session's first ao5 is new.
        perform_solve_of(&mut app, 20_000);
        assert!(app.bests.ao5.is_some(), "the fifth solve makes the first ao5");
        assert!(
            app.best_banner.is_none(),
            "and no ao5 record was beaten either"
        );
    }

    #[test]
    fn a_slower_solve_leaves_the_banner_down() {
        let (mut app, _g) = test_app("best-slower");
        perform_solve_of(&mut app, 10_000);
        perform_solve_of(&mut app, 10_500);
        assert!(app.best_banner.is_none());
    }

    #[test]
    fn matching_a_record_exactly_is_not_beating_it() {
        let bests = SessionBests {
            single: Some(10_000),
            ao5: Some(12_000),
            ..SessionBests::default()
        };
        assert_eq!(
            best_banner_text(Some(10_000), Some(12_000), &bests),
            None,
            "equal is not better"
        );
        assert_eq!(
            best_banner_text(None, None, &bests),
            None,
            "a first record never celebrates"
        );
        assert_eq!(
            best_banner_text(Some(10_001), None, &bests).as_deref(),
            Some("new best single: 10.00")
        );
        assert_eq!(
            best_banner_text(None, Some(12_001), &bests).as_deref(),
            Some("new best ao5: 12.00")
        );
        assert_eq!(
            best_banner_text(Some(10_001), Some(12_001), &bests).as_deref(),
            Some("new best single: 10.00, ao5: 12.00")
        );
    }

    #[test]
    fn the_banner_comes_down_five_seconds_later() {
        let (mut app, _g) = test_app("best-expire");
        perform_solve_of(&mut app, 20_000);
        perform_solve_of(&mut app, 10_000);
        assert!(app.best_banner.is_some(), "the record was beaten");

        app.on_tick();
        assert!(app.best_banner.is_some(), "a fresh banner survives a tick");

        app.banner_since = Some(ago(BEST_BANNER + ms(1)));
        app.on_tick();
        assert!(app.best_banner.is_none(), "five seconds is the whole of it");
        assert!(app.banner_since.is_none());
    }

    #[test]
    fn the_banner_survives_arming_and_clears_when_the_next_solve_starts() {
        let (mut app, _g) = test_app("best-next-solve");
        perform_solve_of(&mut app, 20_000);
        perform_solve_of(&mut app, 10_000);
        assert!(app.best_banner.is_some(), "the record was beaten");

        app.save.settings.inspection = false;
        app.on_key(press(SPACE));
        assert!(matches!(app.state, TimerState::Armed { .. }));
        assert!(
            app.best_banner.is_some(),
            "arming alone must not take the banner down"
        );

        app.state = TimerState::Armed {
            since: ago(ms(350)),
            from_inspection: false,
        };
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Timing { .. }));
        assert!(app.best_banner.is_none(), "the next solve takes it down");
        assert!(app.banner_since.is_none());
    }

    #[test]
    fn a_record_in_another_session_does_not_leak() {
        let (mut app, _g) = test_app("best-per-session");
        // A fast 3x3 session, the shape a csTimer import leaves behind.
        for t in [5_000, 5_100, 5_200, 5_300, 5_400] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");
        assert_eq!(app.bests.single, Some(5_000));
        let imported_ao5 = app.bests.ao5.expect("five solves make an ao5");

        // A second session on the same puzzle starts from nothing at all.
        run_command(&mut app, "new evening");
        assert_eq!(
            app.bests.single, None,
            "another session's times are not this session's bests"
        );
        assert_eq!(app.bests.ao5, None);

        perform_solve_of(&mut app, 20_000);
        assert!(
            app.best_banner.is_none(),
            "the first solve of the session had no record to beat"
        );
        assert_eq!(app.bests.single, Some(newest_millis(&app)));

        // Slower than the other session's 5.00, and still this session's best single.
        perform_solve_of(&mut app, 15_000);
        let want = format!("new best single: {}", format_millis(newest_millis(&app)));
        assert_eq!(app.best_banner.as_deref(), Some(want.as_str()));

        run_command(&mut app, "session 1");
        assert_eq!(app.bests.single, Some(5_000), "and the first session keeps its own");
        assert_eq!(app.bests.ao5, Some(imported_ao5));
    }
}
