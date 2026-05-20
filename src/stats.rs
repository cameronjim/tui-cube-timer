//! Pure statistics helpers: WCA trimmed averages, session summaries, personal bests.
//!
//! Nothing in here touches I/O or global state — every function is a pure function of
//! its arguments, which keeps it trivially testable.

use crate::types::{Session, Solve};

use std::cmp::Ordering;

/// Outcome of an average computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvgResult {
    /// A valid average, in milliseconds (truncated).
    Time(u64),
    /// Too many DNFs in the window for a valid average.
    Dnf,
    /// Fewer solves available than the average requires.
    NotEnough,
}

impl AvgResult {
    /// `"12.34"` / `"DNF"` / `"-"`.
    pub fn display(&self) -> String {
        match self {
            AvgResult::Time(ms) => crate::types::format_millis(*ms),
            AvgResult::Dnf => "DNF".to_string(),
            AvgResult::NotEnough => "-".to_string(),
        }
    }
}

/// How many solves are trimmed from *each* end of an aoN: `ceil(n / 20)`.
///
/// 1 for ao5 / ao12, 2 for ao21..ao40, 5 for ao100.
fn trim_count(n: usize) -> usize {
    n.div_ceil(20)
}

/// Order two effective times with DNF (`None`) sorting as the worst possible result.
fn cmp_effective(a: &Option<u64>, b: &Option<u64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// WCA trimmed average over exactly this window of solves.
fn average_window(window: &[Solve]) -> AvgResult {
    let n = window.len();
    let trim = trim_count(n);
    // Degenerate windows (n == 0, and n <= 2*trim, i.e. n <= 2) leave nothing to average.
    if n == 0 || n <= 2 * trim {
        return AvgResult::NotEnough;
    }

    let mut times: Vec<Option<u64>> = window.iter().map(|s| s.effective_millis()).collect();
    let dnfs = times.iter().filter(|t| t.is_none()).count();
    if dnfs > trim {
        return AvgResult::Dnf;
    }

    times.sort_by(cmp_effective);
    let kept = &times[trim..n - trim];
    // Safe: DNFs sort last and there are at most `trim` of them, so they all land in the
    // trimmed tail.
    let sum: u128 = kept.iter().map(|t| t.unwrap_or(0) as u128).sum();
    AvgResult::Time((sum / kept.len() as u128) as u64)
}

/// WCA trimmed average of the LAST `n` solves (the most recent `n` in the slice's tail).
///
/// Trims `ceil(n/20)` best and worst. DNF sorts as worst; more DNFs than the trim count
/// yields [`AvgResult::Dnf`]. Fewer than `n` solves yields [`AvgResult::NotEnough`].
/// The result is the mean of the remaining solves, truncated to whole milliseconds.
pub fn average_of(n: usize, solves: &[Solve]) -> AvgResult {
    if n == 0 || solves.len() < n {
        return AvgResult::NotEnough;
    }
    average_window(&solves[solves.len() - n..])
}

/// Best rolling aoN anywhere in the session.
///
/// Returns `None` if there were never enough solves, or if every window was a DNF average.
pub fn best_average_of(n: usize, solves: &[Solve]) -> Option<u64> {
    if n == 0 || solves.len() < n {
        return None;
    }
    let mut best: Option<u64> = None;
    for window in solves.windows(n) {
        if let AvgResult::Time(t) = average_window(window) {
            best = Some(match best {
                Some(b) if b <= t => b,
                _ => t,
            });
        }
    }
    best
}

/// Summary statistics for one session's solve list.
#[derive(Debug, Clone)]
pub struct SessionStats {
    /// Total solves, including DNFs.
    pub count: usize,
    /// Solves that are not DNF.
    pub valid_count: usize,
    /// Best single (effective ms).
    pub best: Option<u64>,
    /// Worst single (effective ms), ignoring DNFs.
    pub worst: Option<u64>,
    /// Plain mean of non-DNF effective times, truncated.
    pub mean: Option<u64>,
    pub ao5: AvgResult,
    pub ao12: AvgResult,
    pub ao100: AvgResult,
}

pub fn session_stats(solves: &[Solve]) -> SessionStats {
    let effective: Vec<u64> = solves.iter().filter_map(|s| s.effective_millis()).collect();
    let valid_count = effective.len();
    let best = effective.iter().copied().min();
    let worst = effective.iter().copied().max();
    let mean = if valid_count == 0 {
        None
    } else {
        let sum: u128 = effective.iter().map(|&t| t as u128).sum();
        Some((sum / valid_count as u128) as u64)
    };

    SessionStats {
        count: solves.len(),
        valid_count,
        best,
        worst,
        mean,
        ao5: average_of(5, solves),
        ao12: average_of(12, solves),
        ao100: average_of(100, solves),
    }
}

/// All-time personal bests.
#[derive(Debug, Clone, Default)]
pub struct PersonalBests {
    pub single: Option<u64>,
    pub ao5: Option<u64>,
    pub ao12: Option<u64>,
    pub ao100: Option<u64>,
}

/// Keep the smaller of `slot` and `candidate`.
fn keep_min(slot: &mut Option<u64>, candidate: Option<u64>) {
    if let Some(c) = candidate {
        if slot.is_none_or(|cur| c < cur) {
            *slot = Some(c);
        }
    }
}

/// All-time PBs across the given sessions (the caller filters to a single puzzle).
///
/// Rolling averages are computed per session; windows never span a session boundary.
pub fn personal_bests(sessions: &[&Session]) -> PersonalBests {
    let mut pb = PersonalBests::default();
    for session in sessions {
        let single = session
            .solves
            .iter()
            .filter_map(|s| s.effective_millis())
            .min();
        keep_min(&mut pb.single, single);
        keep_min(&mut pb.ao5, best_average_of(5, &session.solves));
        keep_min(&mut pb.ao12, best_average_of(12, &session.solves));
        keep_min(&mut pb.ao100, best_average_of(100, &session.solves));
    }
    pb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Penalty, Puzzle};

    /// A clean solve of `ms` milliseconds.
    fn s(ms: u64) -> Solve {
        Solve {
            millis: ms,
            penalty: Penalty::None,
            scramble: String::new(),
            timestamp: 0,
        }
    }

    /// A solve of raw `ms` with a +2 penalty.
    fn plus2(ms: u64) -> Solve {
        Solve {
            penalty: Penalty::Plus2,
            ..s(ms)
        }
    }

    /// A DNF solve with raw time `ms`.
    fn dnf(ms: u64) -> Solve {
        Solve {
            penalty: Penalty::Dnf,
            ..s(ms)
        }
    }

    fn solves(times: &[u64]) -> Vec<Solve> {
        times.iter().map(|&t| s(t)).collect()
    }

    fn session(id: u64, times: &[u64]) -> Session {
        Session {
            id,
            name: format!("s{}", id),
            puzzle: Puzzle::Cube3,
            solves: solves(times),
            created_at: 0,
        }
    }

    // ---- trim counts -----------------------------------------------------

    #[test]
    fn trim_is_ceil_n_over_20() {
        assert_eq!(trim_count(1), 1);
        assert_eq!(trim_count(5), 1);
        assert_eq!(trim_count(12), 1);
        assert_eq!(trim_count(20), 1);
        assert_eq!(trim_count(21), 2);
        assert_eq!(trim_count(40), 2);
        assert_eq!(trim_count(41), 3);
        assert_eq!(trim_count(100), 5);
        assert_eq!(trim_count(101), 6);
    }

    #[test]
    fn ao20_trims_one_ao21_trims_two() {
        // 20 solves: 1000..20000. Trim 1 each end -> mean of 2000..19000.
        let times: Vec<u64> = (1..=20).map(|i| i * 1000).collect();
        let v = solves(&times);
        let expected: u64 = (2..=19).map(|i| i * 1000).sum::<u64>() / 18;
        assert_eq!(average_of(20, &v), AvgResult::Time(expected));

        // 21 solves: trim 2 each end -> mean of 3000..19000.
        let times: Vec<u64> = (1..=21).map(|i| i * 1000).collect();
        let v = solves(&times);
        let expected: u64 = (3..=19).map(|i| i * 1000).sum::<u64>() / 17;
        assert_eq!(average_of(21, &v), AvgResult::Time(expected));
    }

    #[test]
    fn ao100_trims_five_each_end() {
        // 100 solves 1000..100000; trim 5 best and 5 worst -> mean of 6000..95000.
        let times: Vec<u64> = (1..=100).map(|i| i * 1000).collect();
        let v = solves(&times);
        let expected: u64 = (6..=95).map(|i| i * 1000).sum::<u64>() / 90;
        assert_eq!(average_of(100, &v), AvgResult::Time(expected));
    }

    // ---- ao5 -------------------------------------------------------------

    #[test]
    fn ao5_no_dnf() {
        // trim 10.000 and 14.000 -> mean(11,12,13) = 12.000
        let v = solves(&[10_000, 14_000, 12_000, 11_000, 13_000]);
        assert_eq!(average_of(5, &v), AvgResult::Time(12_000));
    }

    #[test]
    fn ao5_one_dnf_is_trimmed_as_worst() {
        // DNF is the worst -> trimmed; best (10.000) trimmed -> mean(11,12,13).
        let v = vec![s(10_000), s(11_000), dnf(9_000), s(12_000), s(13_000)];
        assert_eq!(average_of(5, &v), AvgResult::Time(12_000));
    }

    #[test]
    fn ao5_two_dnfs_is_dnf() {
        let v = vec![s(10_000), dnf(11_000), dnf(9_000), s(12_000), s(13_000)];
        assert_eq!(average_of(5, &v), AvgResult::Dnf);
    }

    #[test]
    fn ao5_all_dnf_is_dnf() {
        let v = vec![
            dnf(10_000),
            dnf(11_000),
            dnf(9_000),
            dnf(12_000),
            dnf(13_000),
        ];
        assert_eq!(average_of(5, &v), AvgResult::Dnf);
    }

    #[test]
    fn ao12_one_dnf_allowed_two_not() {
        let mut v = solves(&[
            10_000, 11_000, 12_000, 13_000, 14_000, 15_000, 16_000, 17_000, 18_000, 19_000,
            20_000, 21_000,
        ]);
        v[3] = dnf(13_000);
        // Trim 1 each end: DNF (worst) and 10.000 (best) removed.
        let expected: u64 = (11_000
            + 12_000
            + 14_000
            + 15_000
            + 16_000
            + 17_000
            + 18_000
            + 19_000
            + 20_000
            + 21_000)
            / 10;
        assert_eq!(average_of(12, &v), AvgResult::Time(expected));

        v[7] = dnf(17_000);
        assert_eq!(average_of(12, &v), AvgResult::Dnf);
    }

    // ---- +2 --------------------------------------------------------------

    #[test]
    fn plus2_adds_two_seconds_before_averaging() {
        // Raw 10.000 with +2 -> 12.000, which is now the middle value.
        let v = vec![
            s(11_000),
            plus2(10_000),
            s(13_000),
            s(9_000),
            s(20_000),
        ];
        // Effective: 11.000, 12.000, 13.000, 9.000, 20.000
        // trim 9.000 and 20.000 -> mean(11,12,13) = 12.000
        assert_eq!(average_of(5, &v), AvgResult::Time(12_000));
    }

    #[test]
    fn plus2_can_become_the_trimmed_worst() {
        let v = vec![
            s(10_000),
            s(11_000),
            s(12_000),
            s(13_000),
            plus2(12_500), // -> 14.500, worst
        ];
        // trim 10.000 and 14.500 -> mean(11,12,13) = 12.000
        assert_eq!(average_of(5, &v), AvgResult::Time(12_000));
    }

    #[test]
    fn plus2_counts_in_single_and_mean() {
        let v = vec![s(10_000), plus2(9_000)]; // effective 10.000 and 11.000
        let st = session_stats(&v);
        assert_eq!(st.best, Some(10_000));
        assert_eq!(st.worst, Some(11_000));
        assert_eq!(st.mean, Some(10_500));
    }

    // ---- tail semantics / truncation ------------------------------------

    #[test]
    fn average_of_uses_the_last_n_solves() {
        // First five are fast, last five are slow; ao5 must see only the tail.
        let mut v = solves(&[1_000, 1_000, 1_000, 1_000, 1_000]);
        v.extend(solves(&[10_000, 11_000, 12_000, 13_000, 14_000]));
        assert_eq!(average_of(5, &v), AvgResult::Time(12_000));
    }

    #[test]
    fn mean_is_truncated_not_rounded() {
        // kept = 1000, 1001, 1002 -> sum 3003 / 3 = 1001 exactly; shift by 1ms to force
        // a non-integer mean: kept = 1000, 1001, 1001 -> 3002/3 = 1000.67 -> 1000.
        let v = solves(&[1, 1_000, 1_001, 1_001, 99_999]);
        assert_eq!(average_of(5, &v), AvgResult::Time(1000));
    }

    // ---- not enough / empty ---------------------------------------------

    #[test]
    fn not_enough_solves() {
        assert_eq!(average_of(5, &[]), AvgResult::NotEnough);
        assert_eq!(average_of(5, &solves(&[1, 2, 3, 4])), AvgResult::NotEnough);
        assert_eq!(
            average_of(12, &solves(&[1, 2, 3, 4, 5])),
            AvgResult::NotEnough
        );
        assert_eq!(average_of(0, &solves(&[1, 2, 3])), AvgResult::NotEnough);
    }

    #[test]
    fn degenerate_window_sizes_are_not_enough() {
        // n <= 2 leaves nothing after trimming one from each end.
        assert_eq!(average_of(1, &solves(&[1_000])), AvgResult::NotEnough);
        assert_eq!(average_of(2, &solves(&[1_000, 2_000])), AvgResult::NotEnough);
        // n == 3 is fine: trim 1 each end, one solve remains.
        assert_eq!(
            average_of(3, &solves(&[1_000, 2_000, 3_000])),
            AvgResult::Time(2_000)
        );
    }

    #[test]
    fn exactly_n_solves_is_enough() {
        assert_eq!(
            average_of(5, &solves(&[10_000, 11_000, 12_000, 13_000, 14_000])),
            AvgResult::Time(12_000)
        );
    }

    // ---- display ---------------------------------------------------------

    #[test]
    fn display_formats() {
        assert_eq!(AvgResult::Time(12_345).display(), "12.34");
        assert_eq!(AvgResult::Time(62_990).display(), "1:02.99");
        assert_eq!(AvgResult::Dnf.display(), "DNF");
        assert_eq!(AvgResult::NotEnough.display(), "-");
    }

    // ---- rolling best ----------------------------------------------------

    #[test]
    fn best_average_of_finds_the_best_window() {
        // Windows of 5 over: 20,20,20,20,20, 10,10,10,10,10
        let v = solves(&[
            20_000, 20_000, 20_000, 20_000, 20_000, 10_000, 10_000, 10_000, 10_000, 10_000,
        ]);
        assert_eq!(best_average_of(5, &v), Some(10_000));
    }

    #[test]
    fn best_average_of_is_not_just_the_last_window() {
        // Fast block early, slow block late.
        let v = solves(&[
            10_000, 10_000, 10_000, 10_000, 10_000, 30_000, 30_000, 30_000, 30_000, 30_000,
        ]);
        assert_eq!(best_average_of(5, &v), Some(10_000));
        assert_eq!(average_of(5, &v), AvgResult::Time(30_000));
    }

    #[test]
    fn best_average_of_skips_dnf_windows() {
        // Only the final window has <= 1 DNF.
        let v = vec![
            dnf(1_000),
            dnf(1_000),
            dnf(1_000),
            dnf(1_000),
            s(10_000),
            s(11_000),
            s(12_000),
            s(13_000),
            s(14_000),
        ];
        assert_eq!(best_average_of(5, &v), Some(12_000));
    }

    #[test]
    fn best_average_of_none_when_all_windows_dnf() {
        let v = vec![dnf(1), dnf(2), dnf(3), dnf(4), dnf(5), dnf(6)];
        assert_eq!(best_average_of(5, &v), None);
    }

    #[test]
    fn best_average_of_none_when_not_enough() {
        assert_eq!(best_average_of(5, &[]), None);
        assert_eq!(best_average_of(5, &solves(&[1, 2, 3, 4])), None);
        assert_eq!(best_average_of(0, &solves(&[1, 2, 3, 4])), None);
    }

    // ---- session_stats ---------------------------------------------------

    #[test]
    fn session_stats_empty() {
        let st = session_stats(&[]);
        assert_eq!(st.count, 0);
        assert_eq!(st.valid_count, 0);
        assert_eq!(st.best, None);
        assert_eq!(st.worst, None);
        assert_eq!(st.mean, None);
        assert_eq!(st.ao5, AvgResult::NotEnough);
        assert_eq!(st.ao12, AvgResult::NotEnough);
        assert_eq!(st.ao100, AvgResult::NotEnough);
    }

    #[test]
    fn session_stats_all_dnf() {
        let v = vec![dnf(10_000), dnf(11_000), dnf(12_000)];
        let st = session_stats(&v);
        assert_eq!(st.count, 3);
        assert_eq!(st.valid_count, 0);
        assert_eq!(st.best, None);
        assert_eq!(st.worst, None);
        assert_eq!(st.mean, None);
        assert_eq!(st.ao5, AvgResult::NotEnough); // only 3 solves
    }

    #[test]
    fn session_stats_typical() {
        let v = vec![
            s(10_000),
            s(14_000),
            dnf(30_000),
            plus2(10_500), // 12.500
            s(13_000),
            s(11_000),
        ];
        let st = session_stats(&v);
        assert_eq!(st.count, 6);
        assert_eq!(st.valid_count, 5);
        assert_eq!(st.best, Some(10_000));
        assert_eq!(st.worst, Some(14_000));
        assert_eq!(st.mean, Some((10_000 + 14_000 + 12_500 + 13_000 + 11_000) / 5));
        // ao5 over the last five: 14.000, DNF, 12.500, 13.000, 11.000
        // trim DNF and 11.000 -> mean(12.500, 13.000, 14.000) = 13.166
        assert_eq!(
            st.ao5,
            AvgResult::Time((12_500 + 13_000 + 14_000) / 3)
        );
        assert_eq!(st.ao12, AvgResult::NotEnough);
        assert_eq!(st.ao100, AvgResult::NotEnough);
    }

    #[test]
    fn session_stats_ao5_dnf_when_recent_window_has_two_dnfs() {
        let v = vec![
            s(10_000),
            s(11_000),
            s(12_000),
            dnf(13_000),
            dnf(14_000),
            s(15_000),
            s(16_000),
        ];
        let st = session_stats(&v);
        assert_eq!(st.ao5, AvgResult::Dnf);
        assert_eq!(st.best, Some(10_000));
        assert_eq!(st.valid_count, 5);
    }

    // ---- personal bests --------------------------------------------------

    #[test]
    fn personal_bests_empty_inputs() {
        let pb = personal_bests(&[]);
        assert_eq!(pb.single, None);
        assert_eq!(pb.ao5, None);
        assert_eq!(pb.ao12, None);
        assert_eq!(pb.ao100, None);

        let empty = session(1, &[]);
        let pb = personal_bests(&[&empty]);
        assert_eq!(pb.single, None);
        assert_eq!(pb.ao5, None);
    }

    #[test]
    fn personal_bests_across_sessions() {
        // Session 1 holds the PB single; session 2 holds the PB ao5.
        let a = session(1, &[5_000, 30_000, 30_000, 30_000, 30_000, 30_000]);
        let b = session(2, &[9_000, 10_000, 11_000, 12_000, 13_000]);
        let pb = personal_bests(&[&a, &b]);
        assert_eq!(pb.single, Some(5_000));
        // a: best ao5 window -> trim 5.000/30.000 -> 30.000 ; also all-30 window -> 30.000
        // b: trim 9.000/13.000 -> mean(10,11,12) = 11.000
        assert_eq!(pb.ao5, Some(11_000));
        assert_eq!(pb.ao12, None);
        assert_eq!(pb.ao100, None);
    }

    #[test]
    fn personal_bests_windows_do_not_span_sessions() {
        // Each session alone is too short for an ao5; combined they would be long enough.
        let a = session(1, &[10_000, 10_000, 10_000]);
        let b = session(2, &[10_000, 10_000, 10_000]);
        let pb = personal_bests(&[&a, &b]);
        assert_eq!(pb.single, Some(10_000));
        assert_eq!(pb.ao5, None);
    }

    #[test]
    fn personal_bests_ao12_and_ao100() {
        let times12: Vec<u64> = (1..=12).map(|i| i * 1_000).collect();
        let times100: Vec<u64> = (1..=100).map(|i| i * 1_000).collect();
        let a = session(1, &times12);
        let b = session(2, &times100);
        let pb = personal_bests(&[&a, &b]);
        assert_eq!(pb.single, Some(1_000));
        // b's best ao12 window is its first 12 solves == a's ao12.
        let expected_ao12: u64 = (2..=11).map(|i| i * 1_000).sum::<u64>() / 10;
        assert_eq!(pb.ao12, Some(expected_ao12));
        let expected_ao100: u64 = (6..=95).map(|i| i * 1_000).sum::<u64>() / 90;
        assert_eq!(pb.ao100, Some(expected_ao100));
    }

    #[test]
    fn personal_bests_ignores_dnf_singles() {
        let mut a = session(1, &[20_000]);
        a.solves.push(dnf(1_000)); // fastest raw time, but a DNF
        let pb = personal_bests(&[&a]);
        assert_eq!(pb.single, Some(20_000));
    }
}
