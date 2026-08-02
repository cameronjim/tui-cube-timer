//! JSON persistence for the save file: where it lives, how it is read and atomically written.

use crate::types::{Puzzle, SaveFile, Session, SAVE_VERSION};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Environment variable holding a full path to the data file (overrides everything).
const DATA_ENV_VAR: &str = "CUBETIMER_DATA";
/// File name used inside the platform data directory (and by the fallback).
const DATA_FILE_NAME: &str = "sessions.json";
/// Largest file `load` will read into memory. A lifetime of solves is a few megabytes at most.
const MAX_SAVE_BYTES: u64 = 64 * 1024 * 1024;

/// Sessions file path: `CUBETIMER_DATA` (a full file path), else the platform data dir, else `./sessions.json`.
pub fn data_file_path() -> PathBuf {
    if let Some(from_env) = std::env::var_os(DATA_ENV_VAR) {
        if !from_env.is_empty() {
            return PathBuf::from(from_env);
        }
    }
    if let Some(dirs) = directories::ProjectDirs::from("", "", "cubetimer") {
        return dirs.data_dir().join(DATA_FILE_NAME);
    }
    PathBuf::from("./sessions.json")
}

/// Read the save file: missing yields the default (first run), unparsable is an error so data is never overwritten.
///
/// An older file is migrated in memory and reaches the caller at [`SAVE_VERSION`]; nothing is
/// written back until the app saves. A file from a newer build is refused rather than guessed at,
/// because its fields could mean something this build does not know about.
pub fn load(path: &Path) -> io::Result<SaveFile> {
    // Check the size first: `fs::read` would allocate the whole file before anyone could object.
    match fs::metadata(path) {
        Ok(meta) => check_size(meta.len(), path)?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(SaveFile::default()),
        Err(err) => return Err(err),
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(SaveFile::default()),
        Err(err) => return Err(err),
    };
    let mut save = serde_json::from_slice::<SaveFile>(&bytes).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a valid cubetimer data file: {}", path.display(), err),
        )
    })?;
    if save.version > SAVE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} was written by a newer cubetimer (file version {}, this build reads {})",
                path.display(),
                save.version,
                SAVE_VERSION
            ),
        ));
    }
    // One step per format bump, so each stays a small transformation of the one before it.
    if save.version < 2 {
        save = migrate_to_v2(save);
    }
    if save.version < 3 {
        save = migrate_to_v3(save);
    }
    if save.version < 4 {
        save = migrate_to_v4(save);
    }
    Ok(save)
}

/// The six defaults a version-2 file was built around, in id order.
const V2_DEFAULT_ORDER: [Puzzle; 6] = [
    Puzzle::Cube3,
    Puzzle::Cube2,
    Puzzle::Cube4,
    Puzzle::Cube5,
    Puzzle::Cube6,
    Puzzle::Cube7,
];
/// First user id in a version-2 file: ids 1 through 6 were the reserved defaults.
const V2_FIRST_USER_ID: u64 = 7;
/// Id of the 3x3 default, the fallback a version-1 file lands on when its active session is gone.
const V2_DEFAULT_3X3_ID: u64 = 1;
/// How far version 3 pushes every version-2 user id up, one slot per new default event.
const V3_ID_SHIFT: u64 = 5;
/// The defaults version 3 introduced, on the ids it reserved for them.
const V3_NEW_DEFAULTS: [(u64, Puzzle); 5] = [
    (7, Puzzle::Pyraminx),
    (8, Puzzle::Skewb),
    (9, Puzzle::Megaminx),
    (10, Puzzle::Square1),
    (11, Puzzle::Clock),
];
/// First user id in a version-3 file: ids 1 through 11 were the reserved defaults.
const V3_FIRST_USER_ID: u64 = 12;
/// How far version 4 pushes every version-3 user id up: one slot, for the one-handed default.
const V4_ID_SHIFT: u64 = 1;
/// The default version 4 introduces, on the id it reserves for it.
const V4_NEW_DEFAULT: (u64, Puzzle) = (12, Puzzle::Oh);
/// First user id in a version-4 file: ids 1 through 12 are the reserved defaults.
const V4_FIRST_USER_ID: u64 = 13;

/// A default session on an explicitly given id, for migrations that must not follow `default_session_id`.
fn frozen_default(id: u64, puzzle: Puzzle) -> Session {
    Session {
        id,
        name: "default".to_string(),
        puzzle,
        solves: Vec::new(),
        created_at: 0,
    }
}

/// Migrate a version-1 file: six permanent default sessions, user sessions renumbered from [`V2_FIRST_USER_ID`].
///
/// A version-1 file has one session per `/new`, ids from 1, and no reserved range. Each old session
/// named "default" folds into its puzzle's new default, carrying its solves and creation time, so an
/// existing 3x3 history stays on the session `/3x3` now leads to. Everything else keeps its name,
/// puzzle, solves and file order and is renumbered out of the reserved range. The active session
/// follows whichever session it became.
fn migrate_to_v2(old: SaveFile) -> SaveFile {
    let mut sessions: Vec<Session> = V2_DEFAULT_ORDER
        .into_iter()
        .map(Session::default_for)
        .collect();
    // One flag per default, so a second session called "default" is renumbered instead of merged over the first.
    let mut merged = [false; V2_DEFAULT_ORDER.len()];
    let mut next_session_id = V2_FIRST_USER_ID;
    let mut active_session_id = None;

    for session in old.sessions {
        let old_id = session.id;
        // `V2_DEFAULT_ORDER` is in id order, so a puzzle's default sits at `id - 1`.
        let slot = (session.puzzle.default_session_id() - 1) as usize;
        // A hand-edited version-1 file naming an event that only exists in version 3 has no slot here.
        let new_id = if session.name == "default" && matches!(merged.get(slot), Some(false)) {
            merged[slot] = true;
            sessions[slot].solves = session.solves;
            sessions[slot].created_at = session.created_at;
            sessions[slot].id
        } else {
            let id = next_session_id;
            next_session_id = next_session_id.saturating_add(1);
            sessions.push(Session { id, ..session });
            id
        };
        if old_id == old.active_session_id {
            active_session_id = Some(new_id);
        }
    }

    SaveFile {
        version: 2,
        next_session_id,
        sessions,
        active_session_id: active_session_id.unwrap_or(V2_DEFAULT_3X3_ID),
        settings: old.settings,
    }
}

/// Migrate a version-2 file: Pyraminx, Skewb, Megaminx, Square-1 and Clock take ids 7 through 11.
///
/// Those ids belonged to user sessions in version 2, so every session at or above
/// [`V2_FIRST_USER_ID`] moves up by [`V3_ID_SHIFT`], keeping its name, puzzle, solves and file
/// order. The five new defaults are inserted behind the six that already existed, and the active
/// session follows its renumbering.
///
/// Written entirely against `V2_*` and `V3_*` constants rather than `Puzzle::DEFAULT_ORDER` and
/// `FIRST_USER_ID`, for the reason given above [`migrate_to_v2`]: a migration describes a fixed
/// historical format, and reading the live constants would make it emit a twelfth default and
/// renumber from 13 the moment an event is added.
fn migrate_to_v3(old: SaveFile) -> SaveFile {
    let mut defaults: Vec<Session> = Vec::new();
    let mut users: Vec<Session> = Vec::new();
    let mut active_session_id = old.active_session_id;

    for session in old.sessions {
        if session.id < V2_FIRST_USER_ID {
            defaults.push(session);
        } else {
            let id = session.id.saturating_add(V3_ID_SHIFT);
            if session.id == old.active_session_id {
                active_session_id = id;
            }
            users.push(Session { id, ..session });
        }
    }

    // The five events version 3 adds, each on the id it reserved for it.
    defaults.extend(
        V3_NEW_DEFAULTS
            .into_iter()
            .map(|(id, puzzle)| frozen_default(id, puzzle)),
    );

    let next_session_id = users
        .iter()
        .map(|s| s.id.saturating_add(1))
        .max()
        .unwrap_or(V3_FIRST_USER_ID)
        .max(V3_FIRST_USER_ID);

    defaults.append(&mut users);

    SaveFile {
        version: 3,
        next_session_id,
        sessions: defaults,
        active_session_id,
        settings: old.settings,
    }
}

/// Migrate a version-3 file: 3x3 One-Handed takes id 12, which belonged to user sessions before.
///
/// Every session at or above [`V3_FIRST_USER_ID`] moves up by [`V4_ID_SHIFT`], keeping its name,
/// puzzle, solves and file order, the one-handed default is appended behind the eleven that
/// already existed, and the active session follows its renumbering. Settings need no work: they
/// arrived with this version and `serde(default)` has already filled them in.
fn migrate_to_v4(old: SaveFile) -> SaveFile {
    let mut defaults: Vec<Session> = Vec::new();
    let mut users: Vec<Session> = Vec::new();
    let mut active_session_id = old.active_session_id;

    for session in old.sessions {
        if session.id < V3_FIRST_USER_ID {
            defaults.push(session);
        } else {
            let id = session.id.saturating_add(V4_ID_SHIFT);
            if session.id == old.active_session_id {
                active_session_id = id;
            }
            users.push(Session { id, ..session });
        }
    }

    let (oh_id, oh_puzzle) = V4_NEW_DEFAULT;
    defaults.push(frozen_default(oh_id, oh_puzzle));

    let next_session_id = users
        .iter()
        .map(|s| s.id.saturating_add(1))
        .max()
        .unwrap_or(V4_FIRST_USER_ID)
        .max(V4_FIRST_USER_ID);

    defaults.append(&mut users);

    SaveFile {
        version: 4,
        next_session_id,
        sessions: defaults,
        active_session_id,
        settings: old.settings,
    }
}

/// Reject a file too large to be one of ours, naming the path and the ceiling.
fn check_size(len: u64, path: &Path) -> io::Result<()> {
    if len > MAX_SAVE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, past the {} byte limit cubetimer will read",
                path.display(),
                len,
                MAX_SAVE_BYTES
            ),
        ));
    }
    Ok(())
}

/// Write pretty JSON atomically via a sibling `<name>.tmp` + rename, creating parent dirs.
pub fn save(path: &Path, data: &SaveFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut json = serde_json::to_string_pretty(data)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    json.push('\n');

    let tmp = tmp_path(path);
    write_fresh(&tmp, json.as_bytes())?;
    // `fs::rename` replaces an existing destination on both Unix and Windows.
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

/// Write `bytes` to a file this call creates, never to one that was already there.
///
/// `fs::write` opens the path however it finds it, so a symlink planted at the scratch path
/// would redirect the write. `create_new` refuses any existing path instead. The one thing
/// legitimately found there is a `.tmp` left by a crashed run, so a single retry clears it;
/// a second failure is real and propagates.
fn write_fresh(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = match create_new(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(path)?;
            create_new(path)?
        }
        Err(err) => return Err(err),
    };
    file.write_all(bytes)
}

fn create_new(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Current Unix epoch time in milliseconds (0 if the clock predates the epoch).
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Sibling scratch path used by `save`: `<name>.tmp` in the same directory.
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from(DATA_FILE_NAME));
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Penalty, Settings, Solve, FIRST_USER_ID};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A temp path that deletes itself (and any leftover `.tmp` sibling) on drop.
    struct TempFile {
        path: PathBuf,
    }

    impl TempFile {
        /// Unique per process *and* per call, so parallel tests never collide.
        fn new(tag: &str) -> TempFile {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                "cubetimer-test-{}-{}-{}-{}.json",
                tag,
                std::process::id(),
                n,
                now_millis()
            );
            TempFile {
                path: std::env::temp_dir().join(name),
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(tmp_path(&self.path));
        }
    }

    /// A current-version file: the twelve defaults, solves on the 3x3 one, plus a user session.
    fn sample() -> SaveFile {
        let mut save = SaveFile::default();
        save.sessions[0].solves = vec![
            Solve {
                millis: 12_345,
                penalty: Penalty::None,
                scramble: "R U R' U'".to_string(),
                timestamp: 1_700_000_000_000,
            },
            Solve {
                millis: 9_870,
                penalty: Penalty::Plus2,
                scramble: "F R U".to_string(),
                timestamp: 1_700_000_001_000,
            },
            Solve {
                millis: 20_000,
                penalty: Penalty::Dnf,
                scramble: "L D B".to_string(),
                timestamp: 1_700_000_002_000,
            },
        ];
        save.sessions[0].created_at = 1_699_999_999_000;
        save.sessions.push(Session {
            id: FIRST_USER_ID,
            name: "one-handed".to_string(),
            puzzle: Puzzle::Cube2,
            solves: Vec::new(),
            created_at: 1_700_000_003_000,
        });
        save.next_session_id = FIRST_USER_ID + 1;
        save.active_session_id = FIRST_USER_ID;
        save.settings = Settings {
            inspection: true,
            hide_time: false,
        };
        save
    }

    fn assert_same(a: &SaveFile, b: &SaveFile) {
        assert_eq!(a.version, b.version);
        assert_eq!(a.next_session_id, b.next_session_id);
        assert_eq!(a.active_session_id, b.active_session_id);
        assert_eq!(a.settings, b.settings);
        assert_eq!(a.sessions.len(), b.sessions.len());
        for (x, y) in a.sessions.iter().zip(b.sessions.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.name, y.name);
            assert_eq!(x.puzzle, y.puzzle);
            assert_eq!(x.created_at, y.created_at);
            assert_eq!(x.solves.len(), y.solves.len());
            for (s, t) in x.solves.iter().zip(y.solves.iter()) {
                assert_eq!(s.millis, t.millis);
                assert_eq!(s.penalty, t.penalty);
                assert_eq!(s.scramble, t.scramble);
                assert_eq!(s.timestamp, t.timestamp);
            }
        }
    }

    #[test]
    fn round_trip_preserves_everything() {
        let file = TempFile::new("roundtrip");
        let data = sample();
        save(file.path(), &data).expect("save");
        let loaded = load(file.path()).expect("load");
        assert_same(&data, &loaded);
    }

    #[test]
    fn saved_json_is_pretty_and_tmp_file_is_gone() {
        let file = TempFile::new("pretty");
        save(file.path(), &sample()).expect("save");
        let text = fs::read_to_string(file.path()).expect("read");
        assert!(text.contains("\n  \"version\""), "expected indented JSON, got: {text}");
        assert!(text.contains("\"3x3\""), "puzzle should serialize by rename: {text}");
        assert!(
            !tmp_path(file.path()).exists(),
            "temporary file should be renamed away"
        );
    }

    // ---- the size ceiling

    #[test]
    fn check_size_accepts_everything_up_to_the_limit() {
        let path = Path::new("sessions.json");
        assert!(check_size(0, path).is_ok());
        assert!(check_size(MAX_SAVE_BYTES - 1, path).is_ok());
        assert!(check_size(MAX_SAVE_BYTES, path).is_ok(), "the limit itself is allowed");
    }

    #[test]
    fn check_size_rejects_one_byte_past_the_limit() {
        let path = Path::new("some/where/sessions.json");
        let err = check_size(MAX_SAVE_BYTES + 1, path).expect_err("must be refused");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let text = err.to_string();
        assert!(text.contains("sessions.json"), "the error names the path: {text}");
        assert!(
            text.contains(&MAX_SAVE_BYTES.to_string()),
            "the error names the limit: {text}"
        );
        assert!(check_size(u64::MAX, path).is_err());
    }

    #[test]
    fn a_normal_file_is_well_under_the_ceiling() {
        let file = TempFile::new("ceiling");
        save(file.path(), &sample()).expect("save");
        let len = fs::metadata(file.path()).expect("metadata").len();
        assert!(len < MAX_SAVE_BYTES, "a real save file is {len} bytes");
        assert!(load(file.path()).is_ok());
    }

    // ---- the scratch file

    #[test]
    fn save_replaces_a_tmp_file_left_by_a_crashed_run() {
        let file = TempFile::new("staletmp");
        let tmp = tmp_path(file.path());
        fs::write(&tmp, b"leftover garbage from a crash").expect("plant a stale tmp");

        save(file.path(), &sample()).expect("save must clear the stale tmp and retry");

        assert!(!tmp.exists(), "the tmp file is renamed away");
        let loaded = load(file.path()).expect("load");
        assert_same(&sample(), &loaded);
    }

    #[test]
    fn save_fails_rather_than_write_through_something_it_did_not_create() {
        // A directory at the scratch path is the portable stand-in for a planted symlink:
        // both mean "this path already exists", which `create_new` refuses.
        let file = TempFile::new("tmpdir");
        let tmp = tmp_path(file.path());
        fs::create_dir_all(&tmp).expect("plant a directory at the tmp path");

        let err = save(file.path(), &sample()).expect_err("must not write into it");
        assert!(!file.path().exists(), "the real file is left untouched");

        let _ = fs::remove_dir_all(&tmp);
        drop(err);
    }

    #[test]
    fn save_overwrites_existing_file() {
        let file = TempFile::new("overwrite");
        save(file.path(), &sample()).expect("first save");

        let second = SaveFile {
            active_session_id: 1,
            next_session_id: 99,
            ..Default::default()
        };
        save(file.path(), &second).expect("second save");

        let loaded = load(file.path()).expect("load");
        assert_eq!(loaded.next_session_id, 99);
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len());
    }

    /// A temp directory tree that removes itself on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let file = TempFile::new(tag);
            TempDir {
                path: file.path().with_extension("d"),
            }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn save_creates_missing_parent_dirs() {
        let dir = TempDir::new("parents");
        assert!(!dir.path.exists());
        let target = dir.path.join("a").join("b").join("sessions.json");
        save(&target, &SaveFile::default()).expect("save into new dirs");
        assert!(target.exists());
        let loaded = load(&target).expect("load");
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len());
    }

    #[test]
    fn missing_file_yields_default() {
        let file = TempFile::new("missing");
        assert!(!file.path().exists());
        let loaded = load(file.path()).expect("missing file must not be an error");
        let default = SaveFile::default();
        assert_same(&default, &loaded);
        assert!(
            !file.path().exists(),
            "load must not create the file as a side effect"
        );
    }

    #[test]
    fn default_save_file_has_one_default_session_per_puzzle() {
        let save = SaveFile::default();
        assert_eq!(save.version, SAVE_VERSION);
        assert_eq!(save.next_session_id, FIRST_USER_ID);
        assert_eq!(save.active_session_id, 1);
        let shape: Vec<(u64, &str, Puzzle)> = save
            .sessions
            .iter()
            .map(|s| (s.id, s.name.as_str(), s.puzzle))
            .collect();
        assert_eq!(
            shape,
            vec![
                (1, "default", Puzzle::Cube3),
                (2, "default", Puzzle::Cube2),
                (3, "default", Puzzle::Cube4),
                (4, "default", Puzzle::Cube5),
                (5, "default", Puzzle::Cube6),
                (6, "default", Puzzle::Cube7),
                (7, "default", Puzzle::Pyraminx),
                (8, "default", Puzzle::Skewb),
                (9, "default", Puzzle::Megaminx),
                (10, "default", Puzzle::Square1),
                (11, "default", Puzzle::Clock),
                (12, "default", Puzzle::Oh),
            ]
        );
        assert!(save.sessions.iter().all(|s| s.solves.is_empty()));
        assert!(save.sessions.iter().all(|s| s.is_default()));
        assert_eq!(save.settings, Settings::default());
    }

    /// A hand-written version-1 file: the old single `default` session plus one the user made.
    const V1_FILE: &str = r#"{
      "version": 1,
      "next_session_id": 3,
      "sessions": [
        {
          "id": 1,
          "name": "default",
          "puzzle": "3x3",
          "solves": [
            { "millis": 12345, "penalty": "None", "scramble": "R U R' U'", "timestamp": 1700000000000 },
            { "millis": 11000, "penalty": "Plus2", "scramble": "F R U", "timestamp": 1700000001000 }
          ],
          "created_at": 1699999999000
        },
        {
          "id": 2,
          "name": "one-handed",
          "puzzle": "2x2",
          "solves": [
            { "millis": 4000, "penalty": "None", "scramble": "R U", "timestamp": 1700000002000 }
          ],
          "created_at": 1700000003000
        }
      ],
      "active_session_id": 2
    }"#;

    #[test]
    fn a_version_one_file_migrates_all_the_way_to_the_current_format() {
        let file = TempFile::new("migrate-v1");
        fs::write(file.path(), V1_FILE).expect("write");
        let loaded = load(file.path()).expect("a version 1 file must still load");

        assert_eq!(loaded.version, SAVE_VERSION);
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len() + 1);

        // The old "default" session folded into the 3x3 default, solves and creation time included.
        let three = &loaded.sessions[0];
        assert_eq!(three.id, 1);
        assert_eq!(three.puzzle, Puzzle::Cube3);
        assert_eq!(three.created_at, 1_699_999_999_000);
        assert_eq!(three.solves.len(), 2);
        assert_eq!(three.solves[1].penalty, Penalty::Plus2);
        assert!(
            loaded.sessions[1..Puzzle::DEFAULT_ORDER.len()]
                .iter()
                .all(|s| s.name == "default" && s.solves.is_empty()),
            "the other defaults are created empty"
        );
        let default_ids: Vec<u64> = loaded.sessions[..Puzzle::DEFAULT_ORDER.len()]
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(
            default_ids,
            (1..=Puzzle::DEFAULT_ORDER.len() as u64).collect::<Vec<_>>(),
            "both later steps append their defaults on the reserved ids"
        );
        assert_eq!(
            loaded.sessions[Puzzle::DEFAULT_ORDER.len() - 1].puzzle,
            Puzzle::Oh,
            "the one-handed default is last of the reserved range"
        );

        // The user session is renumbered out of the reserved range and keeps everything else.
        let user = &loaded.sessions[Puzzle::DEFAULT_ORDER.len()];
        assert_eq!(user.id, FIRST_USER_ID);
        assert_eq!(user.name, "one-handed");
        assert_eq!(user.puzzle, Puzzle::Cube2);
        assert_eq!(user.solves.len(), 1);
        assert_eq!(user.created_at, 1_700_000_003_000);

        assert_eq!(loaded.next_session_id, FIRST_USER_ID + 1);
        assert_eq!(
            loaded.active_session_id, FIRST_USER_ID,
            "the active session follows its renumbering"
        );

        // Migration is in memory only: the file on disk is still the version 1 text.
        assert_eq!(fs::read_to_string(file.path()).expect("read back"), V1_FILE);
    }

    #[test]
    fn migration_remaps_an_active_default_and_renumbers_a_second_default() {
        let file = TempFile::new("migrate-dupes");
        fs::write(
            file.path(),
            br#"{
              "version": 1,
              "next_session_id": 4,
              "sessions": [
                { "id": 3, "name": "default", "puzzle": "4x4", "solves": [], "created_at": 5 },
                { "id": 1, "name": "default", "puzzle": "4x4", "solves": [], "created_at": 9 }
              ],
              "active_session_id": 1
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");

        // The first one wins the 4x4 default; the second becomes a user session.
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len() + 1);
        let four = &loaded.sessions[2];
        assert_eq!(four.id, 3);
        assert_eq!(four.puzzle, Puzzle::Cube4);
        assert_eq!(four.created_at, 5, "the first default kept its creation time");
        let renumbered = &loaded.sessions[Puzzle::DEFAULT_ORDER.len()];
        assert_eq!(renumbered.id, FIRST_USER_ID);
        assert_eq!(renumbered.created_at, 9);
        assert_eq!(
            loaded.active_session_id, FIRST_USER_ID,
            "the active session was the renumbered one"
        );
    }

    #[test]
    fn a_current_version_file_loads_untouched() {
        let file = TempFile::new("v4-untouched");
        fs::write(
            file.path(),
            br#"{
              "version": 4,
              "next_session_id": 15,
              "sessions": [
                { "id": 14, "name": "evening", "puzzle": "5x5", "solves": [], "created_at": 12 }
              ],
              "active_session_id": 14,
              "settings": { "inspection": true, "hide_time": true }
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");

        assert_eq!(loaded.version, SAVE_VERSION);
        assert_eq!(loaded.sessions.len(), 1, "load must not inject defaults");
        assert_eq!(loaded.sessions[0].name, "evening");
        assert_eq!(loaded.next_session_id, 15);
        assert_eq!(loaded.active_session_id, 14);
        assert!(loaded.settings.inspection && loaded.settings.hide_time);
    }

    // ---- settings

    #[test]
    fn settings_are_defaulted_when_the_file_predates_them() {
        let file = TempFile::new("settings-absent");
        fs::write(
            file.path(),
            br#"{
              "version": 4,
              "next_session_id": 13,
              "sessions": [],
              "active_session_id": 1
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("a file without settings must still load");
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn a_half_written_settings_object_fills_in_the_rest() {
        let file = TempFile::new("settings-partial");
        fs::write(
            file.path(),
            br#"{
              "version": 4,
              "next_session_id": 13,
              "sessions": [],
              "active_session_id": 1,
              "settings": { "hide_time": true }
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");
        assert!(loaded.settings.hide_time);
        assert!(!loaded.settings.inspection, "the missing field takes its default");
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let file = TempFile::new("settings-roundtrip");
        let data = SaveFile {
            settings: Settings {
                inspection: true,
                hide_time: true,
            },
            ..Default::default()
        };
        save(file.path(), &data).expect("save");
        let text = fs::read_to_string(file.path()).expect("read");
        assert!(text.contains("\"settings\""), "settings must be written: {text}");
        assert_eq!(load(file.path()).expect("load").settings, data.settings);
    }

    #[test]
    fn settings_carry_through_the_whole_migration_chain() {
        let file = TempFile::new("settings-migrated");
        fs::write(
            file.path(),
            br#"{
              "version": 1,
              "next_session_id": 2,
              "sessions": [
                { "id": 1, "name": "default", "puzzle": "3x3", "solves": [], "created_at": 1 }
              ],
              "active_session_id": 1,
              "settings": { "inspection": true, "hide_time": false }
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");
        assert!(
            loaded.settings.inspection,
            "no migration step may drop a setting"
        );
    }

    /// A hand-written version-2 file: the six old defaults plus one user session on id 7.
    const V2_FILE: &str = r#"{
      "version": 2,
      "next_session_id": 8,
      "sessions": [
        { "id": 1, "name": "default", "puzzle": "3x3", "solves": [
            { "millis": 12345, "penalty": "None", "scramble": "R U R' U'", "timestamp": 1700000000000 }
          ], "created_at": 11 },
        { "id": 2, "name": "default", "puzzle": "2x2", "solves": [], "created_at": 12 },
        { "id": 3, "name": "default", "puzzle": "4x4", "solves": [], "created_at": 13 },
        { "id": 4, "name": "default", "puzzle": "5x5", "solves": [], "created_at": 14 },
        { "id": 5, "name": "default", "puzzle": "6x6", "solves": [], "created_at": 15 },
        { "id": 6, "name": "default", "puzzle": "7x7", "solves": [], "created_at": 16 },
        { "id": 7, "name": "one-handed", "puzzle": "3x3", "solves": [
            { "millis": 4000, "penalty": "None", "scramble": "R U", "timestamp": 1700000002000 }
          ], "created_at": 17 }
      ],
      "active_session_id": 7
    }"#;

    #[test]
    fn a_version_two_file_gains_the_five_new_defaults_and_renumbers_user_sessions() {
        let file = TempFile::new("migrate-v2");
        fs::write(file.path(), V2_FILE).expect("write");
        let loaded = load(file.path()).expect("a version 2 file must still load");

        assert_eq!(loaded.version, SAVE_VERSION);
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len() + 1);

        // The six old defaults keep their ids, solves and creation times.
        let three = &loaded.sessions[0];
        assert_eq!((three.id, three.puzzle), (1, Puzzle::Cube3));
        assert_eq!(three.solves.len(), 1);
        assert_eq!(three.created_at, 11);

        // The five new events arrive on ids 7 through 11, behind the ones already there.
        let added: Vec<(u64, Puzzle)> = loaded.sessions[6..11]
            .iter()
            .map(|s| (s.id, s.puzzle))
            .collect();
        assert_eq!(
            added,
            vec![
                (7, Puzzle::Pyraminx),
                (8, Puzzle::Skewb),
                (9, Puzzle::Megaminx),
                (10, Puzzle::Square1),
                (11, Puzzle::Clock),
            ]
        );
        assert!(loaded.sessions[6..11]
            .iter()
            .all(|s| s.name == "default" && s.solves.is_empty()));

        // Version 4 adds one more behind them.
        let oh = &loaded.sessions[11];
        assert_eq!((oh.id, oh.puzzle), (12, Puzzle::Oh));
        assert!(oh.name == "default" && oh.solves.is_empty());

        // The user session moves out of the way of them, keeping everything but its id.
        let user = &loaded.sessions[12];
        assert_eq!(user.id, FIRST_USER_ID, "old id 7 moves up by five, then by one more");
        assert_eq!(user.name, "one-handed");
        assert_eq!(user.puzzle, Puzzle::Cube3);
        assert_eq!(user.solves.len(), 1);
        assert_eq!(user.created_at, 17);

        assert_eq!(
            loaded.active_session_id, FIRST_USER_ID,
            "the active session follows its renumbering"
        );
        assert_eq!(loaded.next_session_id, FIRST_USER_ID + 1);

        // Migration is in memory only: the file on disk is still the version 2 text.
        assert_eq!(fs::read_to_string(file.path()).expect("read back"), V2_FILE);
    }

    #[test]
    fn a_version_two_file_without_user_sessions_lands_on_the_first_user_id() {
        let file = TempFile::new("migrate-v2-bare");
        fs::write(
            file.path(),
            br#"{
              "version": 2,
              "next_session_id": 7,
              "sessions": [
                { "id": 1, "name": "default", "puzzle": "3x3", "solves": [], "created_at": 1 }
              ],
              "active_session_id": 1
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");

        assert_eq!(
            loaded.sessions.len(),
            7,
            "only the five defaults version 3 adds, plus the one version 4 adds"
        );
        assert_eq!(loaded.next_session_id, FIRST_USER_ID);
        assert_eq!(loaded.active_session_id, 1);
    }

    #[test]
    fn version_three_renumbering_keeps_every_user_session_in_order() {
        let file = TempFile::new("migrate-v2-many");
        fs::write(
            file.path(),
            br#"{
              "version": 2,
              "next_session_id": 31,
              "sessions": [
                { "id": 9,  "name": "b", "puzzle": "2x2", "solves": [], "created_at": 2 },
                { "id": 7,  "name": "a", "puzzle": "3x3", "solves": [], "created_at": 1 },
                { "id": 30, "name": "c", "puzzle": "4x4", "solves": [], "created_at": 3 }
              ],
              "active_session_id": 9
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");

        let users: Vec<(u64, &str)> = loaded
            .sessions
            .iter()
            .filter(|s| s.id >= FIRST_USER_ID)
            .map(|s| (s.id, s.name.as_str()))
            .collect();
        assert_eq!(
            users,
            vec![(15, "b"), (13, "a"), (36, "c")],
            "every user id shifts by five then by one, and file order survives"
        );
        assert_eq!(loaded.active_session_id, 15);
        assert_eq!(loaded.next_session_id, 37);
    }

    /// A hand-written version-3 file: the eleven old defaults plus one user session on id 12.
    const V3_FILE: &str = r#"{
      "version": 3,
      "next_session_id": 13,
      "sessions": [
        { "id": 1,  "name": "default", "puzzle": "3x3", "solves": [], "created_at": 11 },
        { "id": 2,  "name": "default", "puzzle": "2x2", "solves": [], "created_at": 12 },
        { "id": 3,  "name": "default", "puzzle": "4x4", "solves": [], "created_at": 13 },
        { "id": 4,  "name": "default", "puzzle": "5x5", "solves": [], "created_at": 14 },
        { "id": 5,  "name": "default", "puzzle": "6x6", "solves": [], "created_at": 15 },
        { "id": 6,  "name": "default", "puzzle": "7x7", "solves": [], "created_at": 16 },
        { "id": 7,  "name": "default", "puzzle": "pyraminx", "solves": [], "created_at": 17 },
        { "id": 8,  "name": "default", "puzzle": "skewb", "solves": [], "created_at": 18 },
        { "id": 9,  "name": "default", "puzzle": "megaminx", "solves": [], "created_at": 19 },
        { "id": 10, "name": "default", "puzzle": "sq1", "solves": [], "created_at": 20 },
        { "id": 11, "name": "default", "puzzle": "clock", "solves": [], "created_at": 21 },
        { "id": 12, "name": "one-handed", "puzzle": "3x3", "solves": [
            { "millis": 22000, "penalty": "None", "scramble": "R U", "timestamp": 1700000002000 }
          ], "created_at": 22 }
      ],
      "active_session_id": 12
    }"#;

    #[test]
    fn a_version_three_file_gains_the_one_handed_default_and_renumbers_user_sessions() {
        let file = TempFile::new("migrate-v3");
        fs::write(file.path(), V3_FILE).expect("write");
        let loaded = load(file.path()).expect("a version 3 file must still load");

        assert_eq!(loaded.version, SAVE_VERSION);
        assert_eq!(loaded.sessions.len(), Puzzle::DEFAULT_ORDER.len() + 1);

        // The eleven old defaults keep their ids, puzzles and creation times.
        let old_defaults: Vec<(u64, Puzzle, u64)> = loaded.sessions[..11]
            .iter()
            .map(|s| (s.id, s.puzzle, s.created_at))
            .collect();
        assert_eq!(old_defaults[0], (1, Puzzle::Cube3, 11));
        assert_eq!(old_defaults[10], (11, Puzzle::Clock, 21));

        // One-handed arrives on id 12, empty, behind the ones already there.
        let oh = &loaded.sessions[11];
        assert_eq!((oh.id, oh.puzzle), (12, Puzzle::Oh));
        assert_eq!(oh.name, "default");
        assert!(oh.solves.is_empty());
        assert!(oh.is_default());

        // The user session moves out of its way, keeping everything but its id.
        let user = &loaded.sessions[12];
        assert_eq!(user.id, 13, "old id 12 moves up by one");
        assert_eq!(user.name, "one-handed");
        assert_eq!(user.puzzle, Puzzle::Cube3);
        assert_eq!(user.solves.len(), 1);
        assert_eq!(user.solves[0].millis, 22_000);
        assert_eq!(user.created_at, 22);

        assert_eq!(
            loaded.active_session_id, 13,
            "the active session follows its renumbering"
        );
        assert_eq!(loaded.next_session_id, 14);
        assert_eq!(loaded.settings, Settings::default());

        // Migration is in memory only: the file on disk is still the version 3 text.
        assert_eq!(fs::read_to_string(file.path()).expect("read back"), V3_FILE);
    }

    #[test]
    fn a_version_three_file_without_user_sessions_lands_on_the_first_user_id() {
        let file = TempFile::new("migrate-v3-bare");
        fs::write(
            file.path(),
            br#"{
              "version": 3,
              "next_session_id": 12,
              "sessions": [
                { "id": 1, "name": "default", "puzzle": "3x3", "solves": [], "created_at": 1 }
              ],
              "active_session_id": 1
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");

        assert_eq!(loaded.sessions.len(), 2, "only the one-handed default arrives");
        assert_eq!(loaded.sessions[1].puzzle, Puzzle::Oh);
        assert_eq!(loaded.next_session_id, FIRST_USER_ID);
        assert_eq!(loaded.active_session_id, 1);
    }

    #[test]
    fn version_four_renumbering_keeps_every_user_session_in_order() {
        let file = TempFile::new("migrate-v3-many");
        fs::write(
            file.path(),
            br#"{
              "version": 3,
              "next_session_id": 41,
              "sessions": [
                { "id": 14, "name": "b", "puzzle": "2x2", "solves": [], "created_at": 2 },
                { "id": 12, "name": "a", "puzzle": "3x3", "solves": [], "created_at": 1 },
                { "id": 40, "name": "c", "puzzle": "4x4", "solves": [], "created_at": 3 }
              ],
              "active_session_id": 40
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");

        let users: Vec<(u64, &str)> = loaded
            .sessions
            .iter()
            .filter(|s| s.id >= FIRST_USER_ID)
            .map(|s| (s.id, s.name.as_str()))
            .collect();
        assert_eq!(
            users,
            vec![(15, "b"), (13, "a"), (41, "c")],
            "every user id shifts by one and file order survives"
        );
        assert_eq!(loaded.active_session_id, 41);
        assert_eq!(loaded.next_session_id, 42);
    }

    #[test]
    fn a_newer_version_file_is_an_error() {
        let file = TempFile::new("v5");
        let text = r#"{
          "version": 5,
          "next_session_id": 13,
          "sessions": [],
          "active_session_id": 1
        }"#;
        fs::write(file.path(), text).expect("write");

        let err = load(file.path()).expect_err("a future format must not be guessed at");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = err.to_string();
        assert!(
            msg.contains(&file.path().display().to_string()),
            "the error must name the path: {msg}"
        );
        assert!(msg.contains("newer cubetimer"), "got {msg}");
        assert_eq!(
            fs::read_to_string(file.path()).expect("read back"),
            text,
            "the file must survive a refused load"
        );
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let file = TempFile::new("corrupt");
        fs::write(file.path(), b"{ this is not json ]").expect("write garbage");
        let err = load(file.path()).expect_err("corrupt file must be an error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        // The original bytes must survive an attempted load.
        assert_eq!(
            fs::read(file.path()).expect("read back"),
            b"{ this is not json ]".to_vec()
        );
    }

    #[test]
    fn valid_json_with_wrong_shape_is_an_error() {
        let file = TempFile::new("shape");
        fs::write(file.path(), b"{\"version\": 1}").expect("write");
        let err = load(file.path()).expect_err("missing fields must be an error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn empty_file_is_an_error() {
        let file = TempFile::new("empty");
        fs::write(file.path(), b"").expect("write");
        assert!(load(file.path()).is_err());
    }

    #[test]
    fn tmp_path_is_a_sibling() {
        let p = Path::new("/some/dir/sessions.json");
        assert_eq!(tmp_path(p), PathBuf::from("/some/dir/sessions.json.tmp"));
        assert_eq!(
            tmp_path(Path::new("sessions.json")),
            PathBuf::from("sessions.json.tmp")
        );
    }

    #[test]
    fn data_file_path_is_usable() {
        // No specific location asserted: CUBETIMER_DATA may be set in the environment.
        let p = data_file_path();
        assert!(!p.as_os_str().is_empty());
        assert!(p.file_name().is_some());
    }

    #[test]
    fn now_millis_is_plausible() {
        let now = now_millis();
        // After 2020-01-01 and before 2100-01-01.
        assert!(now > 1_577_836_800_000, "clock looks wrong: {now}");
        assert!(now < 4_102_444_800_000, "clock looks wrong: {now}");
    }
}
