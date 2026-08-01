//! JSON persistence for the save file: where it lives, how it is read and atomically written.

use crate::types::{Puzzle, SaveFile, Session, FIRST_USER_ID, SAVE_VERSION};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Environment variable holding a full path to the data file (overrides everything).
const DATA_ENV_VAR: &str = "CUBETIMER_DATA";
/// File name used inside the platform data directory (and by the fallback).
const DATA_FILE_NAME: &str = "sessions.json";

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
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(SaveFile::default()),
        Err(err) => return Err(err),
    };
    let save = serde_json::from_slice::<SaveFile>(&bytes).map_err(|err| {
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
    Ok(if save.version < SAVE_VERSION {
        migrate_to_v2(save)
    } else {
        save
    })
}

/// Migrate a version-1 file: six permanent default sessions, user sessions renumbered from [`FIRST_USER_ID`].
///
/// A version-1 file has one session per `/new`, ids from 1, and no reserved range. Each old session
/// named "default" folds into its puzzle's new default, carrying its solves and creation time, so an
/// existing 3x3 history stays on the session `/3x3` now leads to. Everything else keeps its name,
/// puzzle, solves and file order and is renumbered out of the reserved range. The active session
/// follows whichever session it became.
fn migrate_to_v2(old: SaveFile) -> SaveFile {
    let mut sessions: Vec<Session> = Puzzle::DEFAULT_ORDER
        .into_iter()
        .map(Session::default_for)
        .collect();
    // One flag per default, so a second session called "default" is renumbered instead of merged over the first.
    let mut merged = [false; Puzzle::DEFAULT_ORDER.len()];
    let mut next_session_id = FIRST_USER_ID;
    let mut active_session_id = None;

    for session in old.sessions {
        let old_id = session.id;
        // `DEFAULT_ORDER` is in id order, so a puzzle's default sits at `id - 1`.
        let slot = (session.puzzle.default_session_id() - 1) as usize;
        let new_id = if session.name == "default" && !merged[slot] {
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
        version: SAVE_VERSION,
        next_session_id,
        sessions,
        active_session_id: active_session_id.unwrap_or_else(|| Puzzle::Cube3.default_session_id()),
    }
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
    fs::write(&tmp, json.as_bytes())?;
    // `fs::rename` replaces an existing destination on both Unix and Windows.
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
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
    use crate::types::{Penalty, Solve};
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

    /// A current-version file: the six defaults, solves on the 3x3 one, plus a user session.
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
        save
    }

    fn assert_same(a: &SaveFile, b: &SaveFile) {
        assert_eq!(a.version, b.version);
        assert_eq!(a.next_session_id, b.next_session_id);
        assert_eq!(a.active_session_id, b.active_session_id);
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
            ]
        );
        assert!(save.sessions.iter().all(|s| s.solves.is_empty()));
        assert!(save.sessions.iter().all(|s| s.is_default()));
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
    fn a_version_one_file_migrates_to_the_six_defaults() {
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
        let file = TempFile::new("v2-untouched");
        fs::write(
            file.path(),
            br#"{
              "version": 2,
              "next_session_id": 9,
              "sessions": [
                { "id": 8, "name": "evening", "puzzle": "5x5", "solves": [], "created_at": 12 }
              ],
              "active_session_id": 8
            }"#,
        )
        .expect("write");
        let loaded = load(file.path()).expect("load");

        assert_eq!(loaded.version, SAVE_VERSION);
        assert_eq!(loaded.sessions.len(), 1, "load must not inject defaults");
        assert_eq!(loaded.sessions[0].name, "evening");
        assert_eq!(loaded.next_session_id, 9);
        assert_eq!(loaded.active_session_id, 8);
    }

    #[test]
    fn a_newer_version_file_is_an_error() {
        let file = TempFile::new("v3");
        let text = r#"{
          "version": 3,
          "next_session_id": 7,
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
