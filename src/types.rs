//! Core shared types. Every module builds on these; keep this file dependency-light.

use serde::{Deserialize, Serialize};

/// Save-file format this build writes. `storage::load` migrates anything older and refuses anything newer.
pub const SAVE_VERSION: u32 = 4;
/// First id a user-created session can take: ids 1 through 12 are the permanent per-puzzle defaults.
pub const FIRST_USER_ID: u64 = 13;

/// The puzzle events the timer supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Puzzle {
    #[serde(rename = "2x2")]
    Cube2,
    #[serde(rename = "3x3")]
    Cube3,
    #[serde(rename = "4x4")]
    Cube4,
    #[serde(rename = "5x5")]
    Cube5,
    #[serde(rename = "6x6")]
    Cube6,
    #[serde(rename = "7x7")]
    Cube7,
    #[serde(rename = "pyraminx")]
    Pyraminx,
    #[serde(rename = "skewb")]
    Skewb,
    #[serde(rename = "megaminx")]
    Megaminx,
    #[serde(rename = "sq1")]
    Square1,
    #[serde(rename = "clock")]
    Clock,
    /// 3x3 One-Handed: the same puzzle and the same scrambles as 3x3, timed as its own event.
    #[serde(rename = "oh")]
    Oh,
}

impl Puzzle {
    pub const ALL: [Puzzle; 12] = [
        Puzzle::Cube2,
        Puzzle::Cube3,
        Puzzle::Cube4,
        Puzzle::Cube5,
        Puzzle::Cube6,
        Puzzle::Cube7,
        Puzzle::Pyraminx,
        Puzzle::Skewb,
        Puzzle::Megaminx,
        Puzzle::Square1,
        Puzzle::Clock,
        Puzzle::Oh,
    ];

    /// The puzzles in default-session order, so `DEFAULT_ORDER[n]` owns id `n + 1`.
    pub const DEFAULT_ORDER: [Puzzle; 12] = [
        Puzzle::Cube3,
        Puzzle::Cube2,
        Puzzle::Cube4,
        Puzzle::Cube5,
        Puzzle::Cube6,
        Puzzle::Cube7,
        Puzzle::Pyraminx,
        Puzzle::Skewb,
        Puzzle::Megaminx,
        Puzzle::Square1,
        Puzzle::Clock,
        Puzzle::Oh,
    ];

    /// Fixed id of this puzzle's permanent default session. 3x3 comes first because it is the common case.
    pub fn default_session_id(self) -> u64 {
        match self {
            Puzzle::Cube3 => 1,
            Puzzle::Cube2 => 2,
            Puzzle::Cube4 => 3,
            Puzzle::Cube5 => 4,
            Puzzle::Cube6 => 5,
            Puzzle::Cube7 => 6,
            Puzzle::Pyraminx => 7,
            Puzzle::Skewb => 8,
            Puzzle::Megaminx => 9,
            Puzzle::Square1 => 10,
            Puzzle::Clock => 11,
            Puzzle::Oh => 12,
        }
    }

    /// Display / command name: "2x2", "3x3", "pyraminx", ...
    pub fn name(self) -> &'static str {
        match self {
            Puzzle::Cube2 => "2x2",
            Puzzle::Cube3 => "3x3",
            Puzzle::Cube4 => "4x4",
            Puzzle::Cube5 => "5x5",
            Puzzle::Cube6 => "6x6",
            Puzzle::Cube7 => "7x7",
            Puzzle::Pyraminx => "pyraminx",
            Puzzle::Skewb => "skewb",
            Puzzle::Megaminx => "megaminx",
            Puzzle::Square1 => "sq1",
            Puzzle::Clock => "clock",
            Puzzle::Oh => "oh",
        }
    }

    /// Parse an event name or one of its aliases (case-insensitive). Returns None for anything else.
    pub fn from_name(s: &str) -> Option<Puzzle> {
        match s.to_ascii_lowercase().as_str() {
            "2x2" => Some(Puzzle::Cube2),
            "3x3" => Some(Puzzle::Cube3),
            "4x4" => Some(Puzzle::Cube4),
            "5x5" => Some(Puzzle::Cube5),
            "6x6" => Some(Puzzle::Cube6),
            "7x7" => Some(Puzzle::Cube7),
            "pyraminx" | "pyra" => Some(Puzzle::Pyraminx),
            "skewb" => Some(Puzzle::Skewb),
            "megaminx" | "mega" => Some(Puzzle::Megaminx),
            "sq1" | "square1" | "square-1" => Some(Puzzle::Square1),
            "clock" => Some(Puzzle::Clock),
            "oh" | "3x3oh" => Some(Puzzle::Oh),
            _ => None,
        }
    }
}

/// Penalty applied to a solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Penalty {
    #[default]
    None,
    Plus2,
    Dnf,
}

/// A single recorded solve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solve {
    /// Raw measured time in milliseconds (before penalty).
    pub millis: u64,
    #[serde(default)]
    pub penalty: Penalty,
    pub scramble: String,
    /// Unix epoch milliseconds when the solve finished.
    pub timestamp: u64,
}

impl Solve {
    /// Effective time in ms after penalty; None means DNF.
    ///
    /// Saturating, because `millis` comes straight off a hand-editable JSON file and a
    /// value near `u64::MAX` must not panic a debug build.
    pub fn effective_millis(&self) -> Option<u64> {
        match self.penalty {
            Penalty::None => Some(self.millis),
            Penalty::Plus2 => Some(self.millis.saturating_add(2000)),
            Penalty::Dnf => None,
        }
    }
}

/// A practice session: an ordered list of solves for one puzzle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: u64,
    pub name: String,
    pub puzzle: Puzzle,
    pub solves: Vec<Solve>,
    /// Unix epoch milliseconds when the session was created.
    pub created_at: u64,
}

impl Session {
    /// The permanent default session for a puzzle: its fixed id, the name "default", no solves.
    pub fn default_for(puzzle: Puzzle) -> Session {
        Session {
            id: puzzle.default_session_id(),
            name: "default".to_string(),
            puzzle,
            solves: Vec::new(),
            created_at: 0,
        }
    }

    /// True for the twelve permanent defaults, which can never be deleted, renamed or retyped.
    pub fn is_default(&self) -> bool {
        self.id < FIRST_USER_ID
    }
}

/// User preferences that outlive a run.
///
/// Every field carries `serde(default)`, so a save file written before the setting existed
/// parses into the same value a fresh install would get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Settings {
    /// Whether the 15-second WCA inspection countdown runs before each solve.
    #[serde(default)]
    pub inspection: bool,
    /// Whether the running time is hidden until the solve ends.
    #[serde(default)]
    pub hide_time: bool,
}

/// Root of the persisted data file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    /// Monotonic counter for session ids, never below [`FIRST_USER_ID`].
    pub next_session_id: u64,
    /// Every session, the twelve permanent defaults (ids 1 through 12) first.
    pub sessions: Vec<Session>,
    /// Id of the session that was active when the app last ran.
    pub active_session_id: u64,
    /// Preferences. Absent in files written before version 4, hence the default.
    #[serde(default)]
    pub settings: Settings,
}

impl Default for SaveFile {
    fn default() -> Self {
        SaveFile {
            version: SAVE_VERSION,
            next_session_id: FIRST_USER_ID,
            sessions: Puzzle::DEFAULT_ORDER
                .into_iter()
                .map(Session::default_for)
                .collect(),
            active_session_id: Puzzle::Cube3.default_session_id(),
            settings: Settings::default(),
        }
    }
}

/// Format ms as "1:02.45" / "12.34", truncated to centiseconds (WCA style).
pub fn format_millis(ms: u64) -> String {
    let centis = ms / 10;
    let (min, rem) = (centis / 6000, centis % 6000);
    let (sec, cs) = (rem / 100, rem % 100);
    if min > 0 {
        format!("{}:{:02}.{:02}", min, sec, cs)
    } else {
        format!("{}.{:02}", sec, cs)
    }
}

/// Format a solve for display: "12.34", "14.02+" (penalty included), "DNF(13.11)".
pub fn format_solve(s: &Solve) -> String {
    match s.penalty {
        Penalty::None => format_millis(s.millis),
        Penalty::Plus2 => format!("{}+", format_millis(s.millis.saturating_add(2000))),
        Penalty::Dnf => format!("DNF({})", format_millis(s.millis)),
    }
}

/// Format Unix epoch milliseconds as "2026-08-01 09:14 UTC".
///
/// UTC only, and no dependency: a timestamp comes off a hand-editable JSON file, so the
/// arithmetic is done in `u64` with no subtraction that can wrap and no value that can
/// overflow, and `u64::MAX` yields an absurd year rather than a panic.
pub fn format_timestamp(epoch_ms: u64) -> String {
    let secs = epoch_ms / 1_000;
    let days = secs / 86_400;
    let second_of_day = secs % 86_400;
    let (hour, minute) = (second_of_day / 3_600, (second_of_day % 3_600) / 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// Days since the Unix epoch to a (year, month, day) civil date, by Howard Hinnant's algorithm.
///
/// Shifting the epoch to 0000-03-01 puts the leap day at the end of the year, which is what
/// removes every special case from the month arithmetic. The 719468 offset is the distance
/// between that epoch and 1970-01-01.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days.saturating_add(719_468);
    let era = z / 146_097;
    // Day of era, always in 0..=146096, which bounds every quantity below.
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    // March-based month index, 0..=11.
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    // January and February belong to the following calendar year.
    (year + u64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A solve of `ms` raw milliseconds carrying `penalty`.
    fn solve(ms: u64, penalty: Penalty) -> Solve {
        Solve {
            millis: ms,
            penalty,
            scramble: String::new(),
            timestamp: 0,
        }
    }

    // ---- format_millis

    #[test]
    fn format_millis_pads_seconds_and_centiseconds() {
        assert_eq!(format_millis(0), "0.00");
        assert_eq!(format_millis(999), "0.99");
        assert_eq!(format_millis(1_000), "1.00");
        assert_eq!(format_millis(9_050), "9.05");
        assert_eq!(format_millis(59_999), "59.99");
    }

    #[test]
    fn format_millis_switches_to_minutes_at_one_minute() {
        assert_eq!(format_millis(60_000), "1:00.00");
        assert_eq!(format_millis(62_990), "1:02.99");
        assert_eq!(format_millis(599_990), "9:59.99");
    }

    #[test]
    fn format_millis_counts_an_hour_in_minutes() {
        // There is no hours field: an hour is 60 minutes and the field just widens.
        assert_eq!(format_millis(3_600_000), "60:00.00");
        assert_eq!(format_millis(3_661_230), "61:01.23");
    }

    #[test]
    fn format_millis_truncates_it_never_rounds() {
        assert_eq!(format_millis(9), "0.00");
        assert_eq!(format_millis(12_349), "12.34");
        assert_eq!(format_millis(59_999), "59.99", "59.999s must not read as a minute");
        assert_eq!(format_millis(1_999), "1.99");
    }

    // ---- format_solve

    #[test]
    fn format_solve_marks_each_penalty() {
        assert_eq!(format_solve(&solve(12_340, Penalty::None)), "12.34");
        assert_eq!(format_solve(&solve(12_340, Penalty::Plus2)), "14.34+");
        assert_eq!(format_solve(&solve(13_110, Penalty::Dnf)), "DNF(13.11)");
    }

    // ---- hostile input

    #[test]
    fn a_plus2_near_the_end_of_u64_saturates_instead_of_overflowing() {
        let s = solve(u64::MAX, Penalty::Plus2);
        assert_eq!(s.effective_millis(), Some(u64::MAX));
        // The point is that neither call panics in a debug build.
        assert_eq!(format_solve(&s), format!("{}+", format_millis(u64::MAX)));
        assert_eq!(
            solve(u64::MAX - 1, Penalty::Plus2).effective_millis(),
            Some(u64::MAX)
        );
    }

    #[test]
    fn effective_millis_applies_the_penalty() {
        assert_eq!(solve(10_000, Penalty::None).effective_millis(), Some(10_000));
        assert_eq!(solve(10_000, Penalty::Plus2).effective_millis(), Some(12_000));
        assert_eq!(solve(10_000, Penalty::Dnf).effective_millis(), None);
    }

    // ---- format_timestamp

    #[test]
    fn format_timestamp_renders_the_epoch_itself() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_timestamp(999), "1970-01-01 00:00 UTC");
    }

    #[test]
    fn format_timestamp_renders_a_known_date() {
        // 2026-08-01 09:14:00 UTC.
        assert_eq!(format_timestamp(1_785_575_640_000), "2026-08-01 09:14 UTC");
        // 2023-11-14 22:13:20 UTC, the timestamp the storage fixtures use.
        assert_eq!(format_timestamp(1_700_000_000_000), "2023-11-14 22:13 UTC");
    }

    #[test]
    fn format_timestamp_handles_leap_days_and_year_boundaries() {
        assert_eq!(format_timestamp(951_782_400_000), "2000-02-29 00:00 UTC");
        assert_eq!(format_timestamp(1_583_020_800_000), "2020-03-01 00:00 UTC");
        assert_eq!(format_timestamp(1_735_689_599_000), "2024-12-31 23:59 UTC");
        assert_eq!(format_timestamp(1_735_689_600_000), "2025-01-01 00:00 UTC");
    }

    #[test]
    fn format_timestamp_survives_an_absurd_value() {
        // Straight off a hand-edited file: an implausible year is fine, a panic is not.
        let far = format_timestamp(u64::MAX);
        assert!(far.ends_with(" UTC"), "got {far}");
        assert!(!format_timestamp(u64::MAX - 1).is_empty());
    }

    // ---- settings

    #[test]
    fn settings_start_out_off() {
        let s = Settings::default();
        assert!(!s.inspection);
        assert!(!s.hide_time);
        assert_eq!(SaveFile::default().settings, s);
    }

    // ---- the puzzle table

    #[test]
    fn every_puzzle_has_its_own_default_session_id() {
        let mut ids: Vec<u64> = Puzzle::ALL.iter().map(|p| p.default_session_id()).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=Puzzle::ALL.len() as u64).collect::<Vec<_>>());
        assert_eq!(FIRST_USER_ID, Puzzle::ALL.len() as u64 + 1);
        assert_eq!(Puzzle::DEFAULT_ORDER.len(), Puzzle::ALL.len());
        for (i, puzzle) in Puzzle::DEFAULT_ORDER.into_iter().enumerate() {
            assert_eq!(puzzle.default_session_id(), i as u64 + 1);
        }
    }

    #[test]
    fn every_puzzle_name_round_trips_through_from_name() {
        for puzzle in Puzzle::ALL {
            assert_eq!(Puzzle::from_name(puzzle.name()), Some(puzzle));
            assert_eq!(Puzzle::from_name(&puzzle.name().to_uppercase()), Some(puzzle));
        }
    }

    #[test]
    fn one_handed_is_named_oh_and_accepts_its_alias() {
        assert_eq!(Puzzle::Oh.name(), "oh");
        assert_eq!(Puzzle::from_name("oh"), Some(Puzzle::Oh));
        assert_eq!(Puzzle::from_name("3x3oh"), Some(Puzzle::Oh));
        assert_eq!(Puzzle::from_name("3X3OH"), Some(Puzzle::Oh));
        assert_eq!(Puzzle::from_name("3x3"), Some(Puzzle::Cube3), "still the two-handed event");
        assert_eq!(Puzzle::Oh.default_session_id(), 12);
    }

    #[test]
    fn one_handed_serializes_under_its_own_tag() {
        let json = serde_json::to_string(&Puzzle::Oh).expect("serialize");
        assert_eq!(json, "\"oh\"");
        let back: Puzzle = serde_json::from_str("\"oh\"").expect("deserialize");
        assert_eq!(back, Puzzle::Oh);
    }
}
