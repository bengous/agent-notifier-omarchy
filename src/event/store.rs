use chrono::{DateTime, SecondsFormat, Utc};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::fs::{File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::event::{empty_state, parse_state, AgentNotifierState};

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

pub(crate) fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

#[cfg(test)]
mod tests;
