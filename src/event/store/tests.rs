use super::*;
use crate::event::{append_and_trim, set_event_status, EventStatus};
use crate::test_fixtures::{base_event, event_with_address, fixture_clock};
use tempfile::tempdir;

#[test]
fn backs_up_corrupted_state() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    fs::write(&path, "{bad json")?;
    let state = with_state_update(&path, fixture_clock()?, |state| {
        append_and_trim(state, base_event())
    })?;
    assert_eq!(state.events.len(), 1);
    let backups = fs::read_dir(dir.path())?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("events.json.corrupt-")
        })
        .count();
    assert_eq!(backups, 1);
    Ok(())
}

#[test]
fn mark_read_persists_the_read_status() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    let now = fixture_clock()?;
    with_state_update(&path, now, |state| {
        append_and_trim(state, event_with_address("read-me", 300, "0xbeef"))
    })?;

    with_state_update(&path, now, |state| {
        set_event_status(state, "read-me", EventStatus::Read)
    })?;

    let stored = parse_state(&fs::read_to_string(&path)?)?;
    assert_eq!(
        stored.events.first().map(|event| event.status),
        Some(EventStatus::Read)
    );
    Ok(())
}

#[test]
fn keeps_unknown_fields_across_state_rewrites() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    let raw = r#"{"version":1,"rootExtra":"root-kept","events":[{"id":"e","agent":"claude",
        "kind":"main","projectName":"p","projectPath":"/repo/dotfiles","cwd":"/repo/dotfiles",
        "sessionId":"s","createdAt":"2026-07-26T08:00:00.000Z",
        "workspace":{"id":1,"name":"1","monitor":"DP-3","clientPid":42,"title":"t",
            "workspaceExtra":"workspace-kept"},
        "status":"unread","eventExtra":"event-kept"}]}"#;
    fs::write(&path, raw)?;
    let now = DateTime::from_timestamp_millis(0).unwrap_or_else(Utc::now);

    with_state_update(&path, now, |state| {
        set_event_status(state, "e", EventStatus::Read)
    })?;

    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    assert_eq!(value["events"][0]["status"], "read");
    assert_eq!(value["rootExtra"], "root-kept");
    assert_eq!(value["events"][0]["eventExtra"], "event-kept");
    assert_eq!(
        value["events"][0]["workspace"]["workspaceExtra"],
        "workspace-kept"
    );
    Ok(())
}

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
    assert!(read_state(&path, fixture_clock()?).is_err());
    Ok(())
}

#[test]
fn a_read_quarantines_a_corrupt_state_only_under_the_store_lock(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    fs::write(&path, "{bad json")?;
    let now = fixture_clock()?;
    let held = acquire_lock(&path)?;

    assert!(read_state(&path, now).is_err());
    assert!(path.exists());
    drop(held);

    assert_eq!(read_state(&path, now)?, empty_state());
    assert!(!path.exists());
    assert_eq!(read_state(&path, now)?, empty_state());
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

    let state = read_state(&path, now)?;

    assert_eq!(state, empty_state());
    assert!(fs::symlink_metadata(lock_path)?.is_file());
    assert_eq!(fs::metadata(path)?.modified()?, modified);
    Ok(())
}
