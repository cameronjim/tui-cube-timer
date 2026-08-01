//! JSON persistence for the save file.
//!
//! Layout decisions live here and nowhere else:
//! * where the data file lives (`data_file_path`),
//! * how it is read (`load` — a missing file is fine, a corrupt one is not),
//! * how it is written (`save` — pretty JSON, written atomically).

use crate::types::SaveFile;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Environment variable holding a full path to the data file (overrides everything).
const DATA_ENV_VAR: &str = "CUBETIMER_DATA";
/// File name used inside the platform data directory (and by the fallback).
const DATA_FILE_NAME: &str = "sessions.json";

/// Path of the JSON file holding all sessions.
///
/// Resolution order:
/// 1. `CUBETIMER_DATA` (a full *file* path, not a directory),
/// 2. the platform data dir from `directories::ProjectDirs` + `sessions.json`,
/// 3. `./sessions.json` next to the current working directory.
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

/// Read the save file.
///
/// A missing file is not an error — it just means "first run" — and yields
/// `SaveFile::default()`. Anything that exists but cannot be parsed *is* an
/// error, so the caller can bail out instead of overwriting real user data
/// with a fresh default on the next save.
pub fn load(path: &Path) -> io::Result<SaveFile> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(SaveFile::default()),
        Err(err) => return Err(err),
    };
    serde_json::from_slice::<SaveFile>(&bytes).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a valid cubetimer data file: {}", path.display(), err),
        )
    })
}

/// Write the save file as pretty-printed JSON, creating parent directories.
///
/// Atomic: the JSON goes to a sibling `<name>.tmp` first and is then renamed
/// over the target, so an interrupted write can never truncate the real file.
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
    use crate::types::{Penalty, Puzzle, Session, Solve};
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

    fn sample() -> SaveFile {
        SaveFile {
            version: 1,
            next_session_id: 3,
            sessions: vec![
                Session {
                    id: 1,
                    name: "default".to_string(),
                    puzzle: Puzzle::Cube3,
                    solves: vec![
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
                    ],
                    created_at: 1_699_999_999_000,
                },
                Session {
                    id: 2,
                    name: "one-handed".to_string(),
                    puzzle: Puzzle::Cube2,
                    solves: Vec::new(),
                    created_at: 1_700_000_003_000,
                },
            ],
            active_session_id: 2,
        }
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
        assert_eq!(loaded.sessions.len(), 1);
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
        assert_eq!(loaded.sessions.len(), 1);
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
        // Deliberately does not assert a specific location: CUBETIMER_DATA may be
        // set in the ambient environment, and tests must not touch the real dir.
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
