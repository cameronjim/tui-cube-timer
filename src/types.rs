//! Core shared types. Every module builds on these; keep this file dependency-light.

use serde::{Deserialize, Serialize};

/// Save-file format this build writes. `storage::load` migrates anything older and refuses anything newer.
pub const SAVE_VERSION: u32 = 2;
/// First id a user-created session can take: ids 1 through 6 are the permanent per-puzzle defaults.
pub const FIRST_USER_ID: u64 = 7;

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
}

impl Puzzle {
    pub const ALL: [Puzzle; 6] = [
        Puzzle::Cube2,
        Puzzle::Cube3,
        Puzzle::Cube4,
        Puzzle::Cube5,
        Puzzle::Cube6,
        Puzzle::Cube7,
    ];

    /// The puzzles in default-session order, so `DEFAULT_ORDER[n]` owns id `n + 1`.
    pub const DEFAULT_ORDER: [Puzzle; 6] = [
        Puzzle::Cube3,
        Puzzle::Cube2,
        Puzzle::Cube4,
        Puzzle::Cube5,
        Puzzle::Cube6,
        Puzzle::Cube7,
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
        }
    }

    /// Display / command name: "2x2", "3x3", ...
    pub fn name(self) -> &'static str {
        match self {
            Puzzle::Cube2 => "2x2",
            Puzzle::Cube3 => "3x3",
            Puzzle::Cube4 => "4x4",
            Puzzle::Cube5 => "5x5",
            Puzzle::Cube6 => "6x6",
            Puzzle::Cube7 => "7x7",
        }
    }

    /// Parse "2x2".."7x7" (case-insensitive). Returns None for anything else.
    pub fn from_name(s: &str) -> Option<Puzzle> {
        match s.to_ascii_lowercase().as_str() {
            "2x2" => Some(Puzzle::Cube2),
            "3x3" => Some(Puzzle::Cube3),
            "4x4" => Some(Puzzle::Cube4),
            "5x5" => Some(Puzzle::Cube5),
            "6x6" => Some(Puzzle::Cube6),
            "7x7" => Some(Puzzle::Cube7),
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
    pub fn effective_millis(&self) -> Option<u64> {
        match self.penalty {
            Penalty::None => Some(self.millis),
            Penalty::Plus2 => Some(self.millis + 2000),
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

    /// True for the six permanent defaults, which can never be deleted, renamed or retyped.
    pub fn is_default(&self) -> bool {
        self.id < FIRST_USER_ID
    }
}

/// Root of the persisted data file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    /// Monotonic counter for session ids, never below [`FIRST_USER_ID`].
    pub next_session_id: u64,
    /// Every session, the six permanent defaults (ids 1 through 6) first.
    pub sessions: Vec<Session>,
    /// Id of the session that was active when the app last ran.
    pub active_session_id: u64,
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
        Penalty::Plus2 => format!("{}+", format_millis(s.millis + 2000)),
        Penalty::Dnf => format!("DNF({})", format_millis(s.millis)),
    }
}
