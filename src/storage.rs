use chrono::{DateTime, SecondsFormat, Utc};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::fs::{File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::state::{empty_state, parse_state, AgentNotifierState};

pub(crate) fn state_path() -> io::Result<PathBuf> {
    let home = state_home_from(
        env::var_os("XDG_STATE_HOME").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
    )?;
    Ok(home.join("agent-notifier/events.json"))
}

pub(crate) fn read_state_or_recover(
    path: &Path,
    now: DateTime<Utc>,
) -> io::Result<AgentNotifierState> {
    if let Err(error) = migrate_legacy_lock_directory(path) {
        eprintln!("agent-notifier: legacy lock migration failed: {error}");
    }
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(empty_state()),
        Err(error) => return Err(error),
    };
    if let Ok(state) = parse_state(&raw) {
        return Ok(state);
    }
    let backup = format!(
        "{}.corrupt-{}",
        path.display(),
        now.to_rfc3339_opts(SecondsFormat::Millis, true)
            .replace([':', '.'], "-")
    );
    fs::rename(path, &backup)?;
    eprintln!("agent-notifier: quarantined corrupt state to {backup}");
    Ok(empty_state())
}

pub(crate) fn with_state_update<F>(
    path: &Path,
    now: DateTime<Utc>,
    update: F,
) -> io::Result<AgentNotifierState>
where
    F: FnOnce(AgentNotifierState) -> AgentNotifierState,
{
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let _guard = acquire_lock(path)?;
    let current = read_state_or_recover(path, now)?;
    let next = update(current.clone());
    if next != current {
        write_state_atomic(path, &next)?;
    }
    Ok(next)
}

fn state_home_from(xdg: Option<PathBuf>, home: Option<PathBuf>) -> io::Result<PathBuf> {
    if let Some(dir) = xdg.filter(|dir| !dir.as_os_str().is_empty()) {
        return Ok(dir);
    }
    home.filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(".local/state"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "neither XDG_STATE_HOME nor HOME is set; refusing to write state to the current directory",
            )
        })
}

fn write_state_atomic(path: &Path, state: &AgentNotifierState) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp_path = dir.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("events.json"),
        std::process::id(),
        current_millis()
    ));
    let data = serde_json::to_string_pretty(state).map_err(io::Error::other)?;
    fs::write(&tmp_path, format!("{data}\n"))?;
    fs::rename(tmp_path, path)
}

const LOCK_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct LockGuard(File);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn lock_path_for(path: &Path) -> PathBuf {
    path.with_extension("json.lock")
}

fn migrate_legacy_lock_directory(path: &Path) -> io::Result<()> {
    let lock_path = lock_path_for(path);
    let metadata = match fs::symlink_metadata(&lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() {
        return Ok(());
    }
    match fs::remove_dir(&lock_path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => return Ok(()),
        Err(error) => return Err(error),
    }
    drop(File::create(lock_path)?);
    Ok(())
}

fn acquire_lock(path: &Path) -> io::Result<LockGuard> {
    let lock_path = lock_path_for(path);
    migrate_legacy_lock_directory(path)?;
    let file = File::create(&lock_path)?;
    let deadline = Instant::now() + LOCK_TIMEOUT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(LockGuard(file)),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "agent-notifier state is locked by another process",
                ));
            }
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn skips_the_write_when_the_update_changes_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("events.json");
        let now = DateTime::from_timestamp_millis(0).unwrap_or_else(Utc::now);
        write_state_atomic(&path, &empty_state())?;
        let first = fs::metadata(&path)?.modified()?;
        thread::sleep(Duration::from_millis(20));
        with_state_update(&path, now, |state| state)?;
        assert_eq!(fs::metadata(&path)?.modified()?, first);
        Ok(())
    }

    #[test]
    fn state_home_requires_an_environment() {
        assert!(state_home_from(None, None).is_err());
        assert_eq!(
            state_home_from(Some(PathBuf::from("/x")), None).ok(),
            Some(PathBuf::from("/x"))
        );
        assert_eq!(
            state_home_from(None, Some(PathBuf::from("/home/u"))).ok(),
            Some(PathBuf::from("/home/u/.local/state"))
        );
    }

    #[test]
    fn propagates_an_unreadable_state_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("events.json");
        // A directory where the state file belongs: reading it is an error, not "empty".
        fs::create_dir(&path)?;
        let now = DateTime::from_timestamp_millis(0).unwrap_or_else(Utc::now);
        assert!(read_state_or_recover(&path, now).is_err());
        Ok(())
    }

    #[test]
    fn lock_is_released_when_the_guard_drops() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("events.json");
        drop(acquire_lock(&path)?);
        let _second = acquire_lock(&path)?;
        Ok(())
    }

    #[test]
    fn lock_is_exclusive_while_held() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("events.json");
        let _held = acquire_lock(&path)?;
        assert!(acquire_lock(&path).is_err());
        Ok(())
    }

    #[test]
    fn replaces_a_legacy_lock_directory() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("events.json");
        fs::create_dir_all(lock_path_for(&path))?;
        let _guard = acquire_lock(&path)?;
        Ok(())
    }

    #[test]
    fn read_replaces_a_legacy_lock_directory_without_rewriting_state(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("events.json");
        let lock_path = lock_path_for(&path);
        let now = DateTime::from_timestamp_millis(0).unwrap_or_else(Utc::now);
        write_state_atomic(&path, &empty_state())?;
        let modified = fs::metadata(&path)?.modified()?;
        fs::create_dir(&lock_path)?;
        thread::sleep(Duration::from_millis(20));

        let state = read_state_or_recover(&path, now)?;

        assert_eq!(state, empty_state());
        assert!(fs::symlink_metadata(lock_path)?.is_file());
        assert_eq!(fs::metadata(path)?.modified()?, modified);
        Ok(())
    }
}
