//! Pure statistics helpers: WCA trimmed averages, session summaries, personal bests.

use crate::types::{Session, Solve};

use std::cmp::Ordering;

/// Outcome of an average computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AvgResult {
    /// A valid average, in milliseconds (truncated).
    Time(u64),
    /// Too many DNFs in the window for a valid average.
    Dnf,
    /// Fewer solves available than the average requires. The default: no solves is not enough.
    #[default]
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

/// Solves trimmed from each end of an aoN: `ceil(n / 20)` (1 for ao5/ao12, 5 for ao100).
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
    // n <= 2 * trim leaves nothing to average.
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
    // Safe: DNFs sort last and number at most `trim`, so they all land in the trimmed tail.
    let sum: u128 = kept.iter().map(|t| t.unwrap_or(0) as u128).sum();
    AvgResult::Time((sum / kept.len() as u128) as u64)
}

/// WCA trimmed average of the last `n` solves: DNF sorts worst, mean truncated to ms.
pub fn average_of(n: usize, solves: &[Solve]) -> AvgResult {
    if n == 0 || solves.len() < n {
        return AvgResult::NotEnough;
    }
    average_window(&solves[solves.len() - n..])
}

/// Best rolling aoN in the session; `None` if never enough solves or every window DNF'd.
pub fn best_average_of(n: usize, solves: &[Solve]) -> Option<u64> {
    // The length guard is also the cheap early-out: `average_window` sorts every window, so
    // the loop below costs O((L - n + 1) * n log n) and dominates a cache refresh at n = 1000.
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

/// Plain untrimmed mean over exactly this window: one DNF poisons the whole result.
fn mean_window(window: &[Solve]) -> AvgResult {
    if window.is_empty() {
        return AvgResult::NotEnough;
    }
    let mut sum: u128 = 0;
    for solve in window {
        match solve.effective_millis() {
            Some(ms) => sum += ms as u128,
            None => return AvgResult::Dnf,
        }
    }
    AvgResult::Time((sum / window.len() as u128) as u64)
}

/// Untrimmed mean of the last `n` solves, the "moN" of cubing: nothing is trimmed, so any
/// DNF in the window makes the whole mean a DNF rather than being absorbed as the worst.
pub fn mean_of_last(n: usize, solves: &[Solve]) -> AvgResult {
    if n == 0 || solves.len() < n {
        return AvgResult::NotEnough;
    }
    mean_window(&solves[solves.len() - n..])
}

/// Best rolling moN in the session; windows holding a DNF are skipped, not counted as slow.
pub fn best_mean_of(n: usize, solves: &[Solve]) -> Option<u64> {
    if n == 0 || solves.len() < n {
        return None;
    }
    let mut best: Option<u64> = None;
    for window in solves.windows(n) {
        if let AvgResult::Time(t) = mean_window(window) {
            best = Some(match best {
                Some(b) if b <= t => b,
                _ => t,
            });
        }
    }
    best
}

/// Summary statistics for one session's solve list. The default is an empty session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// Untrimmed mean of the last three solves.
    pub mo3: AvgResult,
    pub ao5: AvgResult,
    pub ao12: AvgResult,
    pub ao100: AvgResult,
    pub ao1000: AvgResult,
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
        mo3: mean_of_last(3, solves),
        ao5: average_of(5, solves),
        ao12: average_of(12, solves),
        ao100: average_of(100, solves),
        ao1000: average_of(1000, solves),
    }
}

/// All-time personal bests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersonalBests {
    pub single: Option<u64>,
    /// Best untrimmed mean of three anywhere in a session.
    pub mo3: Option<u64>,
    pub ao5: Option<u64>,
    pub ao12: Option<u64>,
    pub ao100: Option<u64>,
    pub ao1000: Option<u64>,
}

/// Keep the smaller of `slot` and `candidate`.
fn keep_min(slot: &mut Option<u64>, candidate: Option<u64>) {
    if let Some(c) = candidate {
        if slot.is_none_or(|cur| c < cur) {
            *slot = Some(c);
        }
    }
}

/// All-time PBs across the given sessions; rolling windows never span a session boundary.
pub fn personal_bests(sessions: &[&Session]) -> PersonalBests {
    let mut pb = PersonalBests::default();
    for session in sessions {
        let single = session
            .solves
            .iter()
            .filter_map(|s| s.effective_millis())
            .min();
        keep_min(&mut pb.single, single);
        keep_min(&mut pb.mo3, best_mean_of(3, &session.solves));
        keep_min(&mut pb.ao5, best_average_of(5, &session.solves));
        keep_min(&mut pb.ao12, best_average_of(12, &session.solves));
        keep_min(&mut pb.ao100, best_average_of(100, &session.solves));
        keep_min(&mut pb.ao1000, best_average_of(1000, &session.solves));
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
        // ao1000 trims 50 from each end, leaving 900 solves in the mean.
        assert_eq!(trim_count(1000), 50);
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
        // Effective 9/11/12/13/20 -> trim 9.000 and 20.000 -> mean(11,12,13) = 12.000
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
        // Kept = 1000, 1001, 1001 -> 3002/3 = 1000.67, truncated to 1000.
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
        assert_eq!(st.mo3, AvgResult::NotEnough);
        assert_eq!(st.ao5, AvgResult::NotEnough);
        assert_eq!(st.ao12, AvgResult::NotEnough);
        assert_eq!(st.ao100, AvgResult::NotEnough);
        assert_eq!(st.ao1000, AvgResult::NotEnough);
    }

    #[test]
    fn the_default_stats_describe_an_empty_session() {
        // `App` starts from the default before its first refresh, so the two must agree.
        let d = SessionStats::default();
        let empty = session_stats(&[]);
        assert_eq!(d.count, empty.count);
        assert_eq!(d.valid_count, empty.valid_count);
        assert_eq!(d.best, empty.best);
        assert_eq!(d.worst, empty.worst);
        assert_eq!(d.mean, empty.mean);
        assert_eq!(d.mo3, empty.mo3);
        assert_eq!(d.ao5, empty.ao5);
        assert_eq!(d.ao12, empty.ao12);
        assert_eq!(d.ao100, empty.ao100);
        assert_eq!(d.ao1000, empty.ao1000);
        assert_eq!(d, empty);
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
        // ao5 over the last five: trim DNF and 11.000 -> mean(12.500, 13.000, 14.000).
        assert_eq!(
            st.ao5,
            AvgResult::Time((12_500 + 13_000 + 14_000) / 3)
        );
        // mo3 over the last three, untrimmed: 12.500, 13.000, 11.000.
        assert_eq!(st.mo3, AvgResult::Time((12_500 + 13_000 + 11_000) / 3));
        assert_eq!(st.ao12, AvgResult::NotEnough);
        assert_eq!(st.ao100, AvgResult::NotEnough);
        assert_eq!(st.ao1000, AvgResult::NotEnough);
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
        assert_eq!(pb, PersonalBests::default());
        assert_eq!(pb.single, None);
        assert_eq!(pb.mo3, None);
        assert_eq!(pb.ao5, None);
        assert_eq!(pb.ao12, None);
        assert_eq!(pb.ao100, None);
        assert_eq!(pb.ao1000, None);

        let empty = session(1, &[]);
        let pb = personal_bests(&[&empty]);
        assert_eq!(pb.single, None);
        assert_eq!(pb.mo3, None);
        assert_eq!(pb.ao5, None);
    }

    #[test]
    fn personal_bests_across_sessions() {
        // Session 1 holds the PB single; session 2 holds the PB ao5.
        let a = session(1, &[5_000, 30_000, 30_000, 30_000, 30_000, 30_000]);
        let b = session(2, &[9_000, 10_000, 11_000, 12_000, 13_000]);
        let pb = personal_bests(&[&a, &b]);
        assert_eq!(pb.single, Some(5_000));
        // a's best ao5 window is 30.000; b's is mean(10,11,12) = 11.000.
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

    // ---- mo3, the untrimmed mean of the last n ---------------------------

    #[test]
    fn mo3_clean() {
        let v = solves(&[10_000, 11_000, 12_000]);
        assert_eq!(mean_of_last(3, &v), AvgResult::Time(11_000));
    }

    #[test]
    fn mo3_counts_the_plus2() {
        // Effective 10.000, 13.000, 12.000.
        let v = vec![s(10_000), plus2(11_000), s(12_000)];
        assert_eq!(
            mean_of_last(3, &v),
            AvgResult::Time((10_000 + 13_000 + 12_000) / 3)
        );
    }

    #[test]
    fn mo3_any_dnf_is_dnf() {
        // Nothing is trimmed, so a DNF cannot be absorbed the way an ao5 absorbs it.
        let mut v = solves(&[10_000, 11_000, 12_000]);
        v[0] = dnf(1_000);
        assert_eq!(mean_of_last(3, &v), AvgResult::Dnf);

        let mut v = solves(&[10_000, 11_000, 12_000]);
        v[1] = dnf(1_000);
        assert_eq!(mean_of_last(3, &v), AvgResult::Dnf);

        let mut v = solves(&[10_000, 11_000, 12_000]);
        v[2] = dnf(99_000);
        assert_eq!(mean_of_last(3, &v), AvgResult::Dnf);
        // The same three solves make a perfectly good ao3, which trims the DNF away.
        assert_eq!(average_of(3, &v), AvgResult::Time(11_000));
    }

    #[test]
    fn mo3_two_solves_is_not_enough() {
        assert_eq!(
            mean_of_last(3, &solves(&[10_000, 11_000])),
            AvgResult::NotEnough
        );
        assert_eq!(mean_of_last(3, &[]), AvgResult::NotEnough);
        assert_eq!(mean_of_last(0, &solves(&[1, 2, 3])), AvgResult::NotEnough);
    }

    #[test]
    fn mo3_does_not_trim() {
        // ao3 trims the 10 and the 40 and reports the survivor; mo3 keeps all three.
        let v = solves(&[10_000, 13_000, 40_000]);
        assert_eq!(average_of(3, &v), AvgResult::Time(13_000));
        assert_eq!(mean_of_last(3, &v), AvgResult::Time(21_000));
    }

    #[test]
    fn mean_of_last_uses_the_tail_and_truncates() {
        let mut v = solves(&[1_000, 1_000, 1_000]);
        v.extend(solves(&[10_000, 11_000, 11_002]));
        // 32.002 / 3 = 10667.33, truncated to 10667.
        assert_eq!(mean_of_last(3, &v), AvgResult::Time(10_667));
    }

    // ---- rolling best mean -----------------------------------------------

    #[test]
    fn best_mean_of_picks_a_non_final_window() {
        let v = solves(&[10_000, 10_000, 10_000, 30_000, 30_000, 30_000]);
        assert_eq!(best_mean_of(3, &v), Some(10_000));
        assert_eq!(mean_of_last(3, &v), AvgResult::Time(30_000));
    }

    #[test]
    fn best_mean_of_skips_dnf_windows() {
        // Only the final window is DNF-free, even though earlier windows hold fast raw times.
        let v = vec![
            dnf(1_000),
            dnf(1_000),
            dnf(1_000),
            dnf(1_000),
            s(10_000),
            s(11_000),
            s(12_000),
        ];
        assert_eq!(best_mean_of(3, &v), Some(11_000));
    }

    #[test]
    fn best_mean_of_none_when_no_clean_window() {
        assert_eq!(best_mean_of(3, &[dnf(1), dnf(2), dnf(3), s(4)]), None);
        assert_eq!(best_mean_of(3, &[]), None);
        assert_eq!(best_mean_of(3, &solves(&[1, 2])), None);
        assert_eq!(best_mean_of(0, &solves(&[1, 2, 3])), None);
    }

    // ---- ao1000 ----------------------------------------------------------

    #[test]
    fn ao1000_needs_a_thousand_solves() {
        let times: Vec<u64> = (1..=999).map(|i| i * 1_000).collect();
        let st = session_stats(&solves(&times));
        assert_eq!(st.count, 999);
        assert_eq!(st.ao1000, AvgResult::NotEnough);
    }

    #[test]
    fn ao1000_at_exactly_a_thousand_trims_fifty_each_end() {
        let times: Vec<u64> = (1..=1_000).map(|i| i * 1_000).collect();
        let v = solves(&times);
        // trim_count(1000) is 50, so the mean runs over 51.000 through 950.000.
        let expected: u64 = (51..=950).map(|i| i * 1_000).sum::<u64>() / 900;
        assert_eq!(average_of(1000, &v), AvgResult::Time(expected));
        assert_eq!(session_stats(&v).ao1000, AvgResult::Time(expected));
    }

    // ---- personal bests for the new fields -------------------------------

    #[test]
    fn personal_bests_mo3_across_sessions() {
        // Session 2 holds the best mean of three.
        let a = session(1, &[20_000, 20_000, 20_000]);
        let b = session(2, &[9_000, 10_000, 11_000, 30_000]);
        let pb = personal_bests(&[&a, &b]);
        assert_eq!(pb.single, Some(9_000));
        assert_eq!(pb.mo3, Some(10_000));
        assert_eq!(pb.ao1000, None);
    }

    #[test]
    fn personal_bests_mo3_windows_do_not_span_sessions() {
        // Two solves each; only a combined slice would reach three.
        let a = session(1, &[10_000, 10_000]);
        let b = session(2, &[10_000, 10_000]);
        let pb = personal_bests(&[&a, &b]);
        assert_eq!(pb.single, Some(10_000));
        assert_eq!(pb.mo3, None);
    }

    #[test]
    fn personal_bests_ao1000_across_sessions() {
        let slow: Vec<u64> = (1..=1_000).map(|i| i * 1_000).collect();
        let fast: Vec<u64> = (1..=1_000).map(|i| i * 100).collect();
        let a = session(1, &slow);
        let b = session(2, &fast);
        let pb = personal_bests(&[&a, &b]);
        assert_eq!(pb.single, Some(100));
        assert_eq!(pb.mo3, Some((100 + 200 + 300) / 3));
        let expected: u64 = (51..=950).map(|i| i * 100).sum::<u64>() / 900;
        assert_eq!(pb.ao1000, Some(expected));
    }
}
