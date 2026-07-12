//! Progress feedback: the trend the sparkline plots and the personal-best celebration.
//!
//! Neither is timer state, so both live beside the state machine in [`super`] rather than
//! inside it. The trend is refilled wherever the statistics are, by `refresh_derived`; the
//! banner is raised by the solve that beat a record and retired by the tick that outlives it.

use std::time::{Duration, Instant};

use super::App;
use crate::stats::PersonalBests;
use crate::types::{format_millis, Solve};

/// How many recent solves [`App::trend`] carries.
pub(super) const TREND_LEN: usize = 50;
/// How long a personal-best celebration stays on screen.
pub(super) const PB_BANNER: Duration = Duration::from_secs(5);

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
/// A first-ever best has no record to beat, so it never celebrates.
fn improved(prev: Option<u64>, now: Option<u64>) -> Option<u64> {
    match (prev, now) {
        (Some(p), Some(n)) if n < p => Some(n),
        _ => None,
    }
}

/// The celebration naming every personal best the finished solve beat, if it beat one.
fn pb_banner_text(
    prev_single: Option<u64>,
    prev_ao5: Option<u64>,
    pbs: &PersonalBests,
) -> Option<String> {
    let single =
        improved(prev_single, pbs.single).map(|t| format!("single: {}", format_millis(t)));
    let ao5 = improved(prev_ao5, pbs.ao5).map(|t| format!("ao5: {}", format_millis(t)));
    let parts: Vec<String> = [single, ao5].into_iter().flatten().collect();
    if parts.is_empty() {
        None
    } else {
        Some(format!("new pb {}", parts.join(", ")))
    }
}

impl App {
    /// Celebrate the solve just recorded if it beat either record it could, given the two
    /// values those records held before it joined them.
    pub(super) fn note_pb(&mut self, prev_single: Option<u64>, prev_ao5: Option<u64>) {
        if let Some(text) = pb_banner_text(prev_single, prev_ao5, &self.pbs) {
            self.pb_banner = Some(text);
            self.banner_since = Some(Instant::now());
        }
    }

    pub(super) fn clear_pb_banner(&mut self) {
        self.pb_banner = None;
        self.banner_since = None;
    }

    /// Take down a celebration that has had its [`PB_BANNER`] on screen.
    pub(super) fn expire_pb_banner(&mut self) {
        if self.banner_since.is_some_and(|up| up.elapsed() >= PB_BANNER) {
            self.clear_pb_banner();
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

    // ------------------------------------------------------------- pb banner

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
        let (mut app, _g) = test_app("pb-single");
        perform_solve_of(&mut app, 20_000);
        assert!(app.pb_banner.is_none(), "the first solve set the record, it beat none");

        perform_solve_of(&mut app, 12_340);
        let recorded = newest_millis(&app);
        assert!(
            (12_340..12_900).contains(&recorded),
            "unexpected raw time {}",
            recorded
        );
        assert_eq!(app.pbs.single, Some(recorded));
        let want = format!("new pb single: {}", format_millis(recorded));
        assert_eq!(app.pb_banner.as_deref(), Some(want.as_str()));
    }

    #[test]
    fn beating_an_existing_ao5_record_raises_the_banner() {
        let (mut app, _g) = test_app("pb-ao5");
        // The 30s solves at both ends are trimmed away, so this ao5 is the mean of 10, 10 and 30.
        for t in [30_000, 10_000, 10_000, 10_000, 30_000] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");
        let before = app.pbs.ao5.expect("five solves make an ao5 record");
        assert_eq!(app.pbs.single, Some(10_000));

        // A sixth 10s solve cannot beat the single, but its window trims both 30s away.
        perform_solve_of(&mut app, 10_000);
        assert_eq!(app.pbs.single, Some(10_000), "the single record stands");
        let ao5 = app.pbs.ao5.expect("still an ao5 record");
        assert!(ao5 < before, "the ao5 improved from {} to {}", before, ao5);
        let want = format!("new pb ao5: {}", format_millis(ao5));
        assert_eq!(app.pb_banner.as_deref(), Some(want.as_str()));
    }

    #[test]
    fn beating_both_records_names_both_in_one_banner() {
        let (mut app, _g) = test_app("pb-both");
        for t in [30_000, 11_000, 11_000, 11_000, 30_000] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");

        perform_solve_of(&mut app, 10_000);
        let single = app.pbs.single.expect("a single record");
        let ao5 = app.pbs.ao5.expect("an ao5 record");
        assert!(single < 11_000, "the single improved");
        let want = format!(
            "new pb single: {}, ao5: {}",
            format_millis(single),
            format_millis(ao5)
        );
        assert_eq!(app.pb_banner.as_deref(), Some(want.as_str()));
    }

    #[test]
    fn a_first_ever_record_is_not_a_celebration() {
        let (mut app, _g) = test_app("pb-first");
        perform_solve_of(&mut app, 9_000);
        assert!(app.pbs.single.is_some(), "the record is set");
        assert!(app.pb_banner.is_none(), "there was no single record to beat");

        for t in [11_000, 12_000, 13_000] {
            add_solve(&mut app, t);
        }
        run_command(&mut app, "ok");
        assert_eq!(app.pbs.ao5, None, "four solves are not an ao5");

        // The fifth solve is slower than everything, so only the first-ever ao5 is new.
        perform_solve_of(&mut app, 20_000);
        assert!(app.pbs.ao5.is_some(), "the fifth solve makes the first ao5");
        assert!(app.pb_banner.is_none(), "and no ao5 record was beaten either");
    }

    #[test]
    fn a_slower_solve_leaves_the_banner_down() {
        let (mut app, _g) = test_app("pb-slower");
        perform_solve_of(&mut app, 10_000);
        perform_solve_of(&mut app, 10_500);
        assert!(app.pb_banner.is_none());
    }

    #[test]
    fn matching_a_record_exactly_is_not_beating_it() {
        let pbs = PersonalBests {
            single: Some(10_000),
            ao5: Some(12_000),
            ..PersonalBests::default()
        };
        assert_eq!(pb_banner_text(Some(10_000), Some(12_000), &pbs), None, "equal is not better");
        assert_eq!(pb_banner_text(None, None, &pbs), None, "a first record never celebrates");
        assert_eq!(
            pb_banner_text(Some(10_001), None, &pbs).as_deref(),
            Some("new pb single: 10.00")
        );
        assert_eq!(
            pb_banner_text(None, Some(12_001), &pbs).as_deref(),
            Some("new pb ao5: 12.00")
        );
        assert_eq!(
            pb_banner_text(Some(10_001), Some(12_001), &pbs).as_deref(),
            Some("new pb single: 10.00, ao5: 12.00")
        );
    }

    #[test]
    fn the_banner_comes_down_five_seconds_later() {
        let (mut app, _g) = test_app("pb-expire");
        perform_solve_of(&mut app, 20_000);
        perform_solve_of(&mut app, 10_000);
        assert!(app.pb_banner.is_some(), "the record was beaten");

        app.on_tick();
        assert!(app.pb_banner.is_some(), "a fresh banner survives a tick");

        app.banner_since = Some(ago(PB_BANNER + ms(1)));
        app.on_tick();
        assert!(app.pb_banner.is_none(), "five seconds is the whole of it");
        assert!(app.banner_since.is_none());
    }

    #[test]
    fn the_banner_survives_arming_and_clears_when_the_next_solve_starts() {
        let (mut app, _g) = test_app("pb-next-solve");
        perform_solve_of(&mut app, 20_000);
        perform_solve_of(&mut app, 10_000);
        assert!(app.pb_banner.is_some(), "the record was beaten");

        app.save.settings.inspection = false;
        app.on_key(press(SPACE));
        assert!(matches!(app.state, TimerState::Armed { .. }));
        assert!(app.pb_banner.is_some(), "arming alone must not take the banner down");

        app.state = TimerState::Armed {
            since: ago(ms(350)),
            from_inspection: false,
        };
        app.on_key(release(SPACE));
        assert!(matches!(app.state, TimerState::Timing { .. }));
        assert!(app.pb_banner.is_none(), "the next solve takes it down");
        assert!(app.banner_since.is_none());
    }

    #[test]
    fn a_record_on_another_puzzle_never_raises_this_puzzles_banner() {
        let (mut app, _g) = test_app("pb-per-puzzle");
        add_solve(&mut app, 60_000);
        run_command(&mut app, "ok");
        assert_eq!(app.pbs.single, Some(60_000));

        run_command(&mut app, "2x2");
        assert_eq!(app.pbs.single, None, "2x2 keeps its own records");
        perform_solve_of(&mut app, 20_000);
        assert!(
            app.pb_banner.is_none(),
            "faster than the 3x3 record is not a 2x2 record beaten"
        );

        perform_solve_of(&mut app, 10_000);
        let want = format!("new pb single: {}", format_millis(newest_millis(&app)));
        assert_eq!(
            app.pb_banner.as_deref(),
            Some(want.as_str()),
            "its own record it does celebrate"
        );

        run_command(&mut app, "3x3");
        assert_eq!(app.pbs.single, Some(60_000), "the 3x3 record is untouched");
    }
}
