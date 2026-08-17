use chrono::{DateTime, Utc};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::setup::probe::{config_path_for, hook_program_resolves, program_is_on_path};
use crate::setup::wire::{
    target_spec, wire_plan, written_verifies, WireAction, WireChange, WireError, WireOutcome,
    WirePlan, WirePlanInput, WireTarget,
};

const BACKUPS_KEPT: usize = 2;
const LOCK_DIR_MODE: u32 = 0o700;

/// Everything the write needs from the world, so the whole sequence runs on a
/// tempdir under test with no environment mutation.
pub(in crate::setup) struct WireRequest<'a> {
    pub(in crate::setup) target: WireTarget,
    pub(in crate::setup) action: WireAction,
    pub(in crate::setup) config_path: PathBuf,
    pub(in crate::setup) lock_path: PathBuf,
    pub(in crate::setup) harness_on_path: bool,
    pub(in crate::setup) now: DateTime<Utc>,
    pub(in crate::setup) resolves: &'a dyn Fn(&str) -> bool,
}

pub(crate) fn wire_system(
    target: WireTarget,
    action: WireAction,
    now: DateTime<Utc>,
) -> io::Result<WireOutcome> {
    let home = env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from);
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "XDG_RUNTIME_DIR is not set; the setup lock has nowhere to live",
            )
        })?;
    let resolves = |command: &str| {
        hook_program_resolves(command, home.as_deref(), &program_is_on_path, &|path| {
            path.is_file()
        })
    };
    apply_wire(&WireRequest {
        target,
        action,
        config_path: config_path_for(target.agent(), home.as_deref()),
        lock_path: runtime_dir.join("agent-notifier/setup.lock"),
        harness_on_path: program_is_on_path(target.agent().id()),
        now,
        resolves: &resolves,
    })
}

pub(in crate::setup) fn apply_wire(request: &WireRequest) -> io::Result<WireOutcome> {
    let _lock = acquire_lock(&request.lock_path)?;
    let spec = target_spec(request.target);
    let config_path = request.config_path.as_path();
    let displayed = config_path.to_string_lossy().into_owned();
    let existing = read_existing(config_path)?;
    let plan = wire_plan(&WirePlanInput {
        spec: &spec,
        action: request.action,
        config_path: &displayed,
        harness_on_path: request.harness_on_path,
        existing: existing.as_deref(),
        resolves: request.resolves,
    })
    .map_err(refused)?;
    let text = match plan {
        WirePlan::AlreadyDone(change) => {
            return Ok(WireOutcome {
                config_path: displayed,
                change,
            })
        }
        WirePlan::Write(text) => text,
    };

    let harness_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    if !harness_dir.is_dir() {
        return Err(refused(WireError::HarnessDirAbsent {
            harness: spec.agent.display_name(),
            dir: harness_dir.to_string_lossy().into_owned(),
        }));
    }

    let backup = match existing {
        Some(_) => Some(back_up(config_path, request.now)?),
        None => None,
    };
    write_atomic(config_path, &text, request.now)?;
    if !written_verifies(
        &spec,
        request.action,
        &fs::read_to_string(config_path)?,
        request.resolves,
    ) {
        roll_back(config_path, backup.as_deref())?;
        return Err(refused(WireError::HookDidNotVerify { path: displayed }));
    }
    Ok(WireOutcome {
        config_path: displayed,
        change: match request.action {
            WireAction::Wire => WireChange::Wired,
            WireAction::Remove => WireChange::Removed,
        },
    })
}

fn refused(error: WireError) -> io::Error {
    io::Error::other(error)
}

fn read_existing(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn back_up(path: &Path, now: DateTime<Utc>) -> io::Result<PathBuf> {
    let backup = path.with_file_name(format!(
        "{}.bak.{}",
        file_name(path),
        now.timestamp_millis()
    ));
    fs::copy(path, &backup)?;
    purge_backups(path)?;
    Ok(backup)
}

/// Only files this command wrote are purged: the prefix must match exactly and
/// the stamp must be entirely numeric, or the file belongs to somebody else.
fn purge_backups(path: &Path) -> io::Result<()> {
    let prefix = format!("{}.bak.", file_name(path));
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut backups = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stamp) = name
            .strip_prefix(&prefix)
            .and_then(|stamp| stamp.parse::<i64>().ok())
        else {
            continue;
        };
        backups.push((stamp, entry.path()));
    }
    backups.sort_by_key(|(stamp, _)| *stamp);
    for (_, stale) in backups.iter().rev().skip(BACKUPS_KEPT) {
        fs::remove_file(stale)?;
    }
    Ok(())
}

fn write_atomic(path: &Path, text: &str, now: DateTime<Utc>) -> io::Result<()> {
    let temporary = path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        file_name(path),
        std::process::id(),
        now.timestamp_millis()
    ));
    fs::write(&temporary, text)?;
    fs::rename(temporary, path)
}

fn roll_back(path: &Path, backup: Option<&Path>) -> io::Result<()> {
    match backup {
        Some(backup) => fs::copy(backup, path).map(|_bytes| ()),
        None => fs::remove_file(path),
    }
}

fn file_name(path: &Path) -> &str {
    path.file_name().and_then(OsStr::to_str).unwrap_or("config")
}

#[derive(Debug)]
struct LockGuard(PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// One lock for every harness: a setup run edits one config, but the operator
/// runs one setup at a time.
fn acquire_lock(path: &Path) -> io::Result<LockGuard> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(LOCK_DIR_MODE))?;
    match create_lock(path) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => reclaim_lock(path),
        result => result,
    }
}

fn create_lock(path: &Path) -> io::Result<LockGuard> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(format!("{}\n", std::process::id()).as_bytes())?;
    Ok(LockGuard(path.to_path_buf()))
}

fn reclaim_lock(path: &Path) -> io::Result<LockGuard> {
    let holder = fs::read_to_string(path).unwrap_or_default();
    let pid = holder.trim().to_owned();
    let held = || {
        refused(WireError::LockHeld {
            path: path.to_string_lossy().into_owned(),
            pid: pid.clone(),
        })
    };
    if pid_is_live(&pid) {
        return Err(held());
    }
    fs::remove_file(path)?;
    create_lock(path).map_err(|_error| held())
}

/// `/proc` is read here rather than through `window::proc`: setup must not
/// depend on the window concept.
fn pid_is_live(pid: &str) -> bool {
    !pid.is_empty()
        && pid.chars().all(|digit| digit.is_ascii_digit())
        && Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(test)]
mod tests;
