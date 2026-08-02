//! csTimer interchange: the save file as a csTimer export, and a csTimer export as sessions.
//!
//! Pure conversion. Nothing here opens a file; `app::commands` does the IO and the id
//! bookkeeping.
//!
//! The shape is csTimer's own, read off its source. The top level is an object of `session1`
//! through `sessionN` solve arrays beside a `properties` object. One solve is
//! `[[penalty, millis], scramble, comment, timestamp]`, where the penalty is 0 for a clean
//! solve, 2000 for a +2 and -1 for a DNF, `millis` is the raw time with no penalty folded into
//! it, and the timestamp is Unix seconds. Per-session names and scramble types live under
//! `properties.sessionData`, which csTimer stores as a JSON-encoded string rather than as an
//! object, and `properties.sessionN` is how many sessions its importer will look for.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::types::{Penalty, Puzzle, SaveFile, Session, Solve};

/// A parsed csTimer export.
#[derive(Debug)]
pub struct Import {
    /// Sessions ready to adopt: id 0 and `created_at` 0, because the caller assigns both.
    pub sessions: Vec<Session>,
    /// Sessions dropped, either for a scramble type Cubetimer has no event for or for a
    /// solve list that could not be read.
    pub skipped: usize,
}

/// Serialize every session as a csTimer-compatible export.
pub fn export(save: &SaveFile) -> String {
    let mut root = Map::new();
    let mut session_data = Map::new();

    for (i, session) in save.sessions.iter().enumerate() {
        let index = i + 1;
        let solves: Vec<Value> = session.solves.iter().map(export_solve).collect();
        root.insert(format!("session{}", index), Value::Array(solves));
        session_data.insert(
            index.to_string(),
            json!({
                "name": export_name(session),
                "opt": { "scrType": scramble_type(session.puzzle) },
                "rank": index,
            }),
        );
    }

    let active = save
        .sessions
        .iter()
        .position(|s| s.id == save.active_session_id)
        .map_or(1, |i| i + 1);
    root.insert(
        "properties".to_string(),
        json!({
            "sessionN": save.sessions.len(),
            "session": active,
            "sessionData": to_json_string(&Value::Object(session_data)),
        }),
    );

    to_pretty_string(&Value::Object(root))
}

/// Parse a csTimer export. Ids, names of clashing sessions and ordering are the caller's problem.
pub fn import(json: &str) -> Result<Import, String> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| format!("not a csTimer export: {}", e))?;
    let Value::Object(root) = root else {
        return Err("not a csTimer export: the top level is not a JSON object".to_string());
    };

    let mut indices: Vec<u64> = root
        .keys()
        .filter_map(|k| k.strip_prefix("session"))
        .filter_map(|n| n.parse().ok())
        .collect();
    if indices.is_empty() && !root.contains_key("properties") {
        return Err("not a csTimer export: it holds no sessions".to_string());
    }
    indices.sort_unstable();

    let meta = session_meta(&root);
    let mut sessions = Vec::new();
    let mut skipped = 0;

    for index in indices {
        let entry = meta.get(&index);
        // csTimer's own default when a session names no scramble type is 3x3.
        let scramble_type = entry
            .and_then(|m| m.scramble_type.as_deref())
            .unwrap_or("333");
        let Some(puzzle) = puzzle_of(scramble_type) else {
            skipped += 1;
            continue;
        };
        let Some(raw) = solve_list(root.get(&format!("session{}", index))) else {
            skipped += 1;
            continue;
        };

        let name = entry
            .and_then(|m| m.name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("imported {}", index));
        sessions.push(Session {
            id: 0,
            name,
            puzzle,
            solves: raw.iter().filter_map(import_solve).collect(),
            created_at: 0,
        });
    }

    Ok(Import { sessions, skipped })
}

/// The csTimer scramble type Cubetimer writes for an event, the WCA one in every case.
fn scramble_type(puzzle: Puzzle) -> &'static str {
    match puzzle {
        Puzzle::Cube2 => "222so",
        Puzzle::Cube3 => "333",
        Puzzle::Cube4 => "444wca",
        Puzzle::Cube5 => "555wca",
        Puzzle::Cube6 => "666wca",
        Puzzle::Cube7 => "777wca",
        Puzzle::Pyraminx => "pyrso",
        Puzzle::Skewb => "skbso",
        Puzzle::Megaminx => "mgmp",
        Puzzle::Square1 => "sqrs",
        Puzzle::Clock => "clkwca",
        Puzzle::Oh => "333oh",
    }
}

/// The event a csTimer scramble type belongs to, its WCA type and its whole-puzzle variants alike.
///
/// None for everything else, which is every case trainer, every blindfolded and fewest-moves
/// type and every puzzle Cubetimer does not have. Filing a 3x3 BLD session under 3x3 would put
/// times into an average that never meant to hold them, so those sessions are skipped instead.
fn puzzle_of(scramble_type: &str) -> Option<Puzzle> {
    match scramble_type {
        "333" | "333o" | "333noob" => Some(Puzzle::Cube3),
        "333oh" => Some(Puzzle::Oh),
        "222so" | "222o" | "2223" | "222nb" => Some(Puzzle::Cube2),
        "444wca" | "444m" | "444" | "444yj" => Some(Puzzle::Cube4),
        "555wca" | "555" => Some(Puzzle::Cube5),
        "666wca" | "666si" | "666p" | "666s" => Some(Puzzle::Cube6),
        "777wca" | "777si" | "777p" | "777s" => Some(Puzzle::Cube7),
        "pyrso" | "pyro" | "pyrm" | "pyrnb" => Some(Puzzle::Pyraminx),
        "skbso" | "skbo" | "skb" | "skbnb" => Some(Puzzle::Skewb),
        "mgmp" | "mgmc" | "mgmo" | "mgmso" => Some(Puzzle::Megaminx),
        "sqrs" | "sq1h" | "sq1t" => Some(Puzzle::Square1),
        "clkwca" | "clkwcab" | "clknf" | "clk" | "clko" | "clkc" | "clke" => Some(Puzzle::Clock),
        _ => None,
    }
}

/// csTimer has one flat list of sessions, so the twelve defaults say which event they are.
fn export_name(session: &Session) -> String {
    if session.is_default() {
        format!("default {}", session.puzzle.name())
    } else {
        session.name.clone()
    }
}

fn export_solve(solve: &Solve) -> Value {
    let penalty = match solve.penalty {
        Penalty::None => 0,
        Penalty::Plus2 => 2000,
        Penalty::Dnf => -1,
    };
    // csTimer keeps the raw time and adds the penalty when it displays one, exactly as
    // `Solve::effective_millis` does, so nothing has to be folded in here.
    json!([
        [penalty, solve.millis],
        solve.scramble,
        "",
        solve.timestamp / 1_000
    ])
}

fn import_solve(value: &Value) -> Option<Solve> {
    let row = value.as_array()?;
    let head = row.first()?.as_array()?;
    let raw_penalty = head.first()?.as_i64()?;
    let millis = nonneg_number(head.get(1)?)?;

    let (penalty, millis) = match raw_penalty {
        0 => (Penalty::None, millis),
        2000 => (Penalty::Plus2, millis),
        n if n < 0 => (Penalty::Dnf, millis),
        // csTimer accepts any positive penalty in milliseconds and Cubetimer has only the
        // +2, so an unusual one goes into the time and the total still reads right.
        n => (Penalty::None, millis.saturating_add(n as u64)),
    };

    Some(Solve {
        millis,
        penalty,
        scramble: row
            .get(1)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        timestamp: row
            .get(3)
            .and_then(nonneg_number)
            .unwrap_or(0)
            .saturating_mul(1_000),
    })
}

/// Name and scramble type per session index, out of `properties.sessionData`.
struct SessionMeta {
    name: Option<String>,
    scramble_type: Option<String>,
}

fn session_meta(root: &Map<String, Value>) -> BTreeMap<u64, SessionMeta> {
    let mut out = BTreeMap::new();
    let Some(properties) = root.get("properties").and_then(as_object) else {
        return out;
    };
    let Some(data) = properties.get("sessionData").and_then(as_object) else {
        return out;
    };

    for (key, value) in data {
        let Ok(index) = key.parse::<u64>() else {
            continue;
        };
        let Some(entry) = value.as_object() else {
            continue;
        };
        // csTimer names an unnamed session after its index, so the name can be a number.
        let name = match entry.get("name") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            _ => None,
        };
        // `scr` is where the scramble type sat before csTimer moved it under `opt`.
        let scramble_type = entry
            .get("opt")
            .and_then(Value::as_object)
            .and_then(|opt| opt.get("scrType"))
            .or_else(|| entry.get("scr"))
            .and_then(Value::as_str)
            .map(str::to_string);
        out.insert(
            index,
            SessionMeta {
                name,
                scramble_type,
            },
        );
    }
    out
}

/// A `session<n>` value as a list of solves, accepting the JSON-encoded string form too.
fn solve_list(value: Option<&Value>) -> Option<Vec<Value>> {
    match value? {
        Value::Array(a) => Some(a.clone()),
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Array(a)) => Some(a),
            _ => None,
        },
        _ => None,
    }
}

/// An object, whether it arrived as one or as the JSON-encoded string csTimer stores.
fn as_object(value: &Value) -> Option<Map<String, Value>> {
    match value {
        Value::Object(m) => Some(m.clone()),
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Object(m)) => Some(m),
            _ => None,
        },
        _ => None,
    }
}

/// A JSON number as whole non-negative milliseconds.
///
/// Floats are rounded rather than refused: a time written as `12.34 * 1000` lands a hair off
/// an integer, and tools that build csTimer files from text exports do exactly that.
fn nonneg_number(value: &Value) -> Option<u64> {
    let n = value.as_f64()?;
    if !n.is_finite() || n <= 0.0 {
        return Some(0);
    }
    Some(n.round().min(u64::MAX as f64) as u64)
}

/// Serialization of a `Value` cannot fail: every key is a string and no number is a NaN.
fn to_json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn to_pretty_string(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FIRST_USER_ID;

    fn solve(millis: u64, penalty: Penalty, scramble: &str, timestamp: u64) -> Solve {
        Solve {
            millis,
            penalty,
            scramble: scramble.to_string(),
            timestamp,
        }
    }

    /// A save file whose 3x3 default holds `solves`, plus one user session.
    fn save_with(solves: Vec<Solve>) -> SaveFile {
        let mut save = SaveFile::default();
        save.sessions[0].solves = solves;
        save.sessions.push(Session {
            id: FIRST_USER_ID,
            name: "evening".to_string(),
            puzzle: Puzzle::Skewb,
            solves: vec![solve(4_000, Penalty::None, "R L B", 1_700_000_000_000)],
            created_at: 7,
        });
        save.next_session_id = FIRST_USER_ID + 1;
        save
    }

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).expect("export is valid JSON")
    }

    /// The `sessionData` object, decoded out of the string csTimer stores it as.
    fn session_data(root: &Value) -> Value {
        let raw = root["properties"]["sessionData"]
            .as_str()
            .expect("sessionData is a JSON-encoded string, as csTimer writes it");
        serde_json::from_str(raw).expect("sessionData parses")
    }

    // ---- export

    #[test]
    fn an_empty_save_exports_the_cstimer_shape() {
        let root = parse(&export(&SaveFile::default()));
        assert_eq!(root["properties"]["sessionN"], json!(12));
        assert_eq!(root["properties"]["session"], json!(1), "3x3 is active");
        for i in 1..=12 {
            assert_eq!(
                root[&format!("session{}", i)],
                json!([]),
                "session{} is an empty solve list",
                i
            );
        }
        let data = session_data(&root);
        assert_eq!(data["1"]["name"], json!("default 3x3"));
        assert_eq!(data["1"]["opt"]["scrType"], json!("333"));
        assert_eq!(data["1"]["rank"], json!(1));
        assert_eq!(data["12"]["name"], json!("default oh"));
        assert_eq!(data["12"]["opt"]["scrType"], json!("333oh"));
    }

    #[test]
    fn export_writes_one_solve_as_cstimer_reads_it() {
        let save = save_with(vec![
            solve(12_340, Penalty::None, "R U R'", 1_700_000_000_000),
            solve(13_000, Penalty::Plus2, "U F", 1_700_000_060_000),
            solve(9_990, Penalty::Dnf, "L D", 1_700_000_120_000),
        ]);
        let root = parse(&export(&save));
        let solves = root["session1"].as_array().expect("session1 is a list");
        assert_eq!(solves.len(), 3);
        assert_eq!(solves[0], json!([[0, 12_340], "R U R'", "", 1_700_000_000]));
        assert_eq!(
            solves[1],
            json!([[2000, 13_000], "U F", "", 1_700_000_060]),
            "a +2 keeps the raw time beside the 2000, exactly as csTimer stores it"
        );
        assert_eq!(
            solves[2],
            json!([[-1, 9_990], "L D", "", 1_700_000_120]),
            "a DNF keeps the time it would have been"
        );
    }

    #[test]
    fn export_names_the_defaults_after_their_event_and_leaves_user_names_alone() {
        let root = parse(&export(&save_with(Vec::new())));
        let data = session_data(&root);
        assert_eq!(data["2"]["name"], json!("default 2x2"));
        assert_eq!(data["13"]["name"], json!("evening"));
        assert_eq!(data["13"]["opt"]["scrType"], json!("skbso"));
        assert_eq!(root["properties"]["sessionN"], json!(13));
    }

    #[test]
    fn every_event_exports_a_scramble_type_that_imports_back_to_it() {
        for puzzle in Puzzle::ALL {
            assert_eq!(
                puzzle_of(scramble_type(puzzle)),
                Some(puzzle),
                "{} does not survive its own scramble type",
                puzzle.name()
            );
        }
        // One-handed is its own csTimer type rather than 3x3 under a different name.
        assert_eq!(scramble_type(Puzzle::Oh), "333oh");
        assert_ne!(scramble_type(Puzzle::Oh), scramble_type(Puzzle::Cube3));
    }

    // ---- round trip

    #[test]
    fn export_then_import_recovers_every_solve() {
        let save = save_with(vec![
            solve(12_340, Penalty::None, "R U R'", 1_700_000_000_000),
            solve(13_000, Penalty::Plus2, "U F", 1_700_000_060_000),
            solve(9_990, Penalty::Dnf, "L D", 1_700_000_120_000),
        ]);
        let back = import(&export(&save)).expect("our own export parses");
        assert_eq!(back.skipped, 0);
        assert_eq!(back.sessions.len(), 13);

        let three = &back.sessions[0];
        assert_eq!(three.puzzle, Puzzle::Cube3);
        assert_eq!(three.name, "default 3x3");
        assert_eq!(three.id, 0, "ids are the caller's to hand out");
        assert_eq!(three.created_at, 0);
        for (i, original) in save.sessions[0].solves.iter().enumerate() {
            let got = &three.solves[i];
            assert_eq!(got.millis, original.millis, "solve {} time", i);
            assert_eq!(got.penalty, original.penalty, "solve {} penalty", i);
            assert_eq!(got.scramble, original.scramble, "solve {} scramble", i);
            assert_eq!(
                got.timestamp, original.timestamp,
                "solve {} timestamp, to the second",
                i
            );
            assert_eq!(got.effective_millis(), original.effective_millis());
        }

        let skewb = back.sessions.last().expect("the user session came back");
        assert_eq!(skewb.name, "evening");
        assert_eq!(skewb.puzzle, Puzzle::Skewb);
        assert_eq!(skewb.solves.len(), 1);
    }

    #[test]
    fn a_timestamp_round_trips_to_the_second() {
        // csTimer counts seconds, so the milliseconds under one are the one thing lost.
        let save = save_with(vec![solve(1_000, Penalty::None, "R", 1_700_000_000_999)]);
        let back = import(&export(&save)).expect("parses");
        assert_eq!(back.sessions[0].solves[0].timestamp, 1_700_000_000_000);
    }

    // ---- import of a handcrafted file

    /// A file in the shape csTimer itself writes, down to the string-encoded `sessionData`.
    fn fixture() -> String {
        let session_data = json!({
            "1": { "name": "main", "opt": { "scrType": "333" }, "rank": 1 },
            "2": { "name": "one handed", "opt": { "scrType": "333oh" }, "rank": 2 },
            "3": { "name": "blind", "opt": { "scrType": "333ni" }, "rank": 3 },
            "4": { "name": 4, "opt": {}, "rank": 4 }
        });
        let root = json!({
            "session1": [
                [[0, 12_340], "R U R' U'", "", 1_700_000_000],
                [[2000, 13_000], "F R U", "a comment", 1_700_000_060],
                [[-1, 9_990], "L D B", "", 1_700_000_120]
            ],
            "session2": [[[0, 20_000], "R U", "", 1_700_000_180]],
            "session3": [[[0, 60_000], "R U", "", 1_700_000_240]],
            "session4": [[[0, 8_000], "U R", "", 1_700_000_300]],
            "properties": {
                "sessionN": 4,
                "session": 1,
                "sessionData": to_json_string(&session_data)
            }
        });
        to_pretty_string(&root)
    }

    #[test]
    fn a_real_shaped_export_imports_with_its_names_and_events() {
        let got = import(&fixture()).expect("the fixture parses");
        assert_eq!(got.skipped, 1, "3x3 bld is not one of our events");
        assert_eq!(got.sessions.len(), 3);

        assert_eq!(got.sessions[0].name, "main");
        assert_eq!(got.sessions[0].puzzle, Puzzle::Cube3);
        assert_eq!(got.sessions[1].name, "one handed");
        assert_eq!(got.sessions[1].puzzle, Puzzle::Oh);
        assert_eq!(
            got.sessions[2].name, "4",
            "csTimer names an unnamed session after its index, as a number"
        );
        assert_eq!(
            got.sessions[2].puzzle,
            Puzzle::Cube3,
            "a session naming no scramble type is 3x3, as it is in csTimer"
        );
    }

    #[test]
    fn import_reads_the_penalties_exactly() {
        let got = import(&fixture()).expect("parses");
        let solves = &got.sessions[0].solves;
        assert_eq!(solves.len(), 3);

        assert_eq!(solves[0].penalty, Penalty::None);
        assert_eq!(solves[0].millis, 12_340);
        assert_eq!(solves[0].scramble, "R U R' U'");
        assert_eq!(
            solves[0].timestamp, 1_700_000_000_000,
            "seconds become millis"
        );

        assert_eq!(solves[1].penalty, Penalty::Plus2);
        assert_eq!(
            solves[1].millis, 13_000,
            "the 2000 is the penalty, not part of the time"
        );
        assert_eq!(solves[1].effective_millis(), Some(15_000));

        assert_eq!(solves[2].penalty, Penalty::Dnf);
        assert_eq!(solves[2].millis, 9_990, "a DNF keeps the time under it");
        assert_eq!(solves[2].effective_millis(), None);
    }

    #[test]
    fn import_skips_a_session_whose_scramble_type_is_not_an_event() {
        let root = json!({
            "session1": [[[0, 1_000], "R", "", 1]],
            "session2": [[[0, 2_000], "R", "", 2]],
            "properties": {
                "sessionN": 2,
                "sessionData": to_json_string(&json!({
                    "1": { "name": "fmc", "opt": { "scrType": "333fm" } },
                    "2": { "name": "pyra", "opt": { "scrType": "pyrso" } }
                }))
            }
        });
        let got = import(&to_pretty_string(&root)).expect("parses");
        assert_eq!(got.skipped, 1);
        assert_eq!(got.sessions.len(), 1);
        assert_eq!(got.sessions[0].name, "pyra");
        assert_eq!(got.sessions[0].puzzle, Puzzle::Pyraminx);
    }

    #[test]
    fn import_accepts_the_string_encoded_forms_older_tools_write() {
        // Both `properties` and each session list may arrive as JSON inside a string.
        let root = json!({
            "session1": r#"[[[0, 5000], "R U", ""]]"#,
            "properties": to_json_string(&json!({
                "sessionN": 1,
                "sessionData": to_json_string(&json!({ "1": { "scr": "222so" } }))
            }))
        });
        let got = import(&to_pretty_string(&root)).expect("parses");
        assert_eq!(got.skipped, 0);
        assert_eq!(got.sessions.len(), 1);
        assert_eq!(
            got.sessions[0].puzzle,
            Puzzle::Cube2,
            "`scr` is where the scramble type sat before it moved under `opt`"
        );
        assert_eq!(got.sessions[0].name, "imported 1", "no name to take");
        assert_eq!(got.sessions[0].solves.len(), 1);
        assert_eq!(got.sessions[0].solves[0].millis, 5_000);
        assert_eq!(
            got.sessions[0].solves[0].timestamp, 0,
            "a solve with no timestamp is not a parse failure"
        );
    }

    #[test]
    fn import_tolerates_a_time_that_is_not_a_whole_number() {
        // Converters that build these files from text exports multiply a float by 1000.
        let root = json!({
            "session1": [[[0, 12_340.000_000_000_002_f64], "R", "", 1]],
            "properties": { "sessionN": 1 }
        });
        let got = import(&to_pretty_string(&root)).expect("parses");
        assert_eq!(got.sessions[0].solves[0].millis, 12_340);
    }

    #[test]
    fn import_folds_a_penalty_cstimer_allows_and_cubetimer_does_not() {
        let root = json!({
            "session1": [[[4000, 10_000], "R", "", 1]],
            "properties": { "sessionN": 1 }
        });
        let got = import(&to_pretty_string(&root)).expect("parses");
        let s = &got.sessions[0].solves[0];
        assert_eq!(s.penalty, Penalty::None);
        assert_eq!(s.millis, 14_000, "the total is what survives");
        assert_eq!(s.effective_millis(), Some(14_000));
    }

    #[test]
    fn import_drops_a_solve_it_cannot_read_and_keeps_the_rest() {
        let root = json!({
            "session1": [
                [[0, 1_000], "R", "", 1],
                "not a solve",
                [[], "R", "", 2],
                [[0, 3_000], "U", "", 3]
            ],
            "properties": { "sessionN": 1 }
        });
        let got = import(&to_pretty_string(&root)).expect("parses");
        let times: Vec<u64> = got.sessions[0].solves.iter().map(|s| s.millis).collect();
        assert_eq!(times, vec![1_000, 3_000]);
    }

    #[test]
    fn an_empty_export_is_valid_input() {
        // csTimer always writes its properties, so a file with those and no solves is an
        // export of nothing rather than the wrong file entirely.
        let got = import("{\"properties\": {\"sessionN\": 0}}").expect("an export of nothing");
        assert!(got.sessions.is_empty());
        assert_eq!(got.skipped, 0);

        let got = import(&export(&SaveFile::default())).expect("parses");
        assert_eq!(got.sessions.len(), 12, "the twelve defaults come back");
        assert!(got.sessions.iter().all(|s| s.solves.is_empty()));
    }

    // ---- errors

    #[test]
    fn garbage_input_is_an_error_and_not_a_panic() {
        for bad in ["", "not json at all", "{\"session1\":", "\u{0}"] {
            let err = import(bad).expect_err("garbage must not parse");
            assert!(
                err.starts_with("not a csTimer export"),
                "{bad:?} gave {err:?}"
            );
        }
    }

    #[test]
    fn json_that_is_not_an_export_is_an_error() {
        for bad in ["[1, 2, 3]", "\"hello\"", "42", "null"] {
            let err = import(bad).expect_err("valid JSON is not automatically an export");
            assert_eq!(
                err, "not a csTimer export: the top level is not a JSON object",
                "{bad:?}"
            );
        }
        for bad in ["{}", "{\"foo\": 1}"] {
            let err = import(bad).expect_err("an unrelated object is not an export");
            assert_eq!(err, "not a csTimer export: it holds no sessions", "{bad:?}");
        }
    }
}
