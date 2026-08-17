use super::*;
use std::cell::Cell;
use std::error::Error;
use tempfile::{tempdir, TempDir};

use crate::test_fixtures::fixture_clock;

const CLAUDE_SETTINGS: &str = r#"{"model":"opus"}"#;
const CODEX_CONFIG: &str = "model = \"gpt\"\n";

fn everything_resolves(_command: &str) -> bool {
    true
}

fn harness_dir(dir: &TempDir, target: WireTarget) -> PathBuf {
    match target {
        WireTarget::Claude => dir.path().join("home/.claude"),
        WireTarget::Codex => dir.path().join("home/.codex"),
    }
}

fn config_path(dir: &TempDir, target: WireTarget) -> PathBuf {
    let file = match target {
        WireTarget::Claude => "settings.json",
        WireTarget::Codex => "config.toml",
    };
    harness_dir(dir, target).join(file)
}

fn lock_path(dir: &TempDir) -> PathBuf {
    dir.path().join("run/agent-notifier/setup.lock")
}

fn seed(dir: &TempDir, target: WireTarget, contents: Option<&str>) -> io::Result<()> {
    fs::create_dir_all(harness_dir(dir, target))?;
    match contents {
        Some(contents) => fs::write(config_path(dir, target), contents),
        None => Ok(()),
    }
}

fn request<'a>(
    dir: &TempDir,
    target: WireTarget,
    action: WireAction,
    resolves: &'a dyn Fn(&str) -> bool,
) -> WireRequest<'a> {
    WireRequest {
        target,
        action,
        config_path: config_path(dir, target),
        lock_path: lock_path(dir),
        harness_on_path: true,
        now: fixture_clock(),
        resolves,
    }
}

fn backup_names(dir: &TempDir, target: WireTarget) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(harness_dir(dir, target))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.contains(".bak.") {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// One resolution succeeds, every later one fails: the binary leaves PATH
/// between the decision and the validation of the written file.
fn resolves_once() -> impl Fn(&str) -> bool {
    let calls = Cell::new(0_usize);
    move |_command: &str| {
        let seen = calls.get();
        calls.set(seen + 1);
        seen == 0
    }
}

#[test]
fn wiring_claude_into_a_tempdir_writes_settings_and_verifies_wired() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, None)?;

    let outcome = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;

    assert_eq!(outcome.change, WireChange::Wired);
    assert_eq!(
        outcome.config_path,
        config_path(&dir, WireTarget::Claude).to_string_lossy()
    );
    let written = fs::read_to_string(config_path(&dir, WireTarget::Claude))?;
    assert!(written.contains(r#""command": "agent-notifier claude-hook""#));
    assert!(backup_names(&dir, WireTarget::Claude)?.is_empty());
    Ok(())
}

#[test]
fn rerunning_setup_on_a_wired_config_leaves_the_file_byte_identical() -> Result<(), Box<dyn Error>>
{
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, None)?;
    apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;
    let wired = fs::read_to_string(config_path(&dir, WireTarget::Claude))?;

    let outcome = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;

    assert_eq!(outcome.change, WireChange::AlreadyWired);
    assert_eq!(
        fs::read_to_string(config_path(&dir, WireTarget::Claude))?,
        wired
    );
    assert!(backup_names(&dir, WireTarget::Claude)?.is_empty());
    Ok(())
}

#[test]
fn setup_backs_up_the_existing_config_before_writing() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, Some(CLAUDE_SETTINGS))?;

    apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;

    let backup = harness_dir(&dir, WireTarget::Claude).join(format!(
        "settings.json.bak.{}",
        fixture_clock().timestamp_millis()
    ));
    assert_eq!(fs::read_to_string(backup)?, CLAUDE_SETTINGS);
    Ok(())
}

#[test]
fn only_the_two_newest_backups_survive_the_purge() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, Some(CLAUDE_SETTINGS))?;
    for stamp in ["1", "2", "3"] {
        fs::write(
            harness_dir(&dir, WireTarget::Claude).join(format!("settings.json.bak.{stamp}")),
            "old",
        )?;
    }
    fs::write(
        harness_dir(&dir, WireTarget::Claude).join("settings.json.bak.keep-me"),
        "not ours",
    )?;

    apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;

    assert_eq!(
        backup_names(&dir, WireTarget::Claude)?,
        vec![
            format!("settings.json.bak.{}", fixture_clock().timestamp_millis()),
            "settings.json.bak.3".to_owned(),
            "settings.json.bak.keep-me".to_owned(),
        ]
    );
    Ok(())
}

#[test]
fn a_failed_validation_restores_the_backup_and_reports_the_error() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, Some(CLAUDE_SETTINGS))?;

    let error = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &resolves_once(),
    ))
    .err()
    .ok_or("the write verified against a binary that left PATH")?;

    assert!(error.to_string().contains("does not read back"));
    assert_eq!(
        fs::read_to_string(config_path(&dir, WireTarget::Claude))?,
        CLAUDE_SETTINGS
    );
    Ok(())
}

#[test]
fn a_failed_validation_on_a_created_file_removes_it() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, None)?;

    let error = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &resolves_once(),
    ))
    .err()
    .ok_or("the write verified against a binary that left PATH")?;

    assert!(error.to_string().contains("does not read back"));
    assert!(!config_path(&dir, WireTarget::Claude).exists());
    Ok(())
}

#[test]
fn unparsable_claude_settings_leave_the_file_untouched_byte_for_byte() -> Result<(), Box<dyn Error>>
{
    let dir = tempdir()?;
    let broken = r#"{"model": "opus",}"#;
    seed(&dir, WireTarget::Claude, Some(broken))?;

    let error = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))
    .err()
    .ok_or("a broken settings file was edited")?;

    assert!(error.to_string().contains("cannot parse"));
    assert_eq!(
        fs::read_to_string(config_path(&dir, WireTarget::Claude))?,
        broken
    );
    assert!(backup_names(&dir, WireTarget::Claude)?.is_empty());
    Ok(())
}

#[test]
fn remove_then_setup_round_trips_the_codex_config() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Codex, Some(CODEX_CONFIG))?;

    let wired = apply_wire(&request(
        &dir,
        WireTarget::Codex,
        WireAction::Wire,
        &everything_resolves,
    ))?;
    let removed = apply_wire(&request(
        &dir,
        WireTarget::Codex,
        WireAction::Remove,
        &everything_resolves,
    ))?;

    assert_eq!(wired.change, WireChange::Wired);
    assert_eq!(removed.change, WireChange::Removed);
    assert_eq!(
        fs::read_to_string(config_path(&dir, WireTarget::Codex))?,
        CODEX_CONFIG
    );
    Ok(())
}

#[test]
fn removing_an_absent_hook_reports_nothing_to_remove() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Codex, Some(CODEX_CONFIG))?;

    let outcome = apply_wire(&request(
        &dir,
        WireTarget::Codex,
        WireAction::Remove,
        &everything_resolves,
    ))?;

    assert_eq!(outcome.change, WireChange::NothingToRemove);
    assert_eq!(
        fs::read_to_string(config_path(&dir, WireTarget::Codex))?,
        CODEX_CONFIG
    );
    Ok(())
}

#[test]
fn the_config_dir_must_exist_before_a_config_is_created() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;

    let error = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))
    .err()
    .ok_or("a config was created outside its harness directory")?;

    assert!(error
        .to_string()
        .contains("does not exist; run Claude once"));
    assert!(!config_path(&dir, WireTarget::Claude).exists());
    Ok(())
}

#[test]
fn a_held_lock_fails_fast_without_touching_the_config() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, None)?;
    let lock = lock_path(&dir);
    fs::create_dir_all(lock.parent().ok_or("the lock has no directory")?)?;
    fs::write(&lock, format!("{}\n", std::process::id()))?;

    let error = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))
    .err()
    .ok_or("a held lock was ignored")?;

    assert!(error
        .to_string()
        .contains("another agent-notifier setup holds"));
    assert!(!config_path(&dir, WireTarget::Claude).exists());
    assert!(lock.exists(), "the holder's lock must survive");
    Ok(())
}

#[test]
fn a_stale_lock_from_a_dead_pid_is_reclaimed() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, None)?;
    let lock = lock_path(&dir);
    fs::create_dir_all(lock.parent().ok_or("the lock has no directory")?)?;
    fs::write(&lock, "4294967294\n")?;

    let outcome = apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;

    assert_eq!(outcome.change, WireChange::Wired);
    Ok(())
}

#[test]
fn the_lock_is_released_after_success_and_after_failure() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, None)?;

    apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;
    assert!(!lock_path(&dir).exists());

    let failing = tempdir()?;
    assert!(apply_wire(&request(
        &failing,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves
    ))
    .is_err());
    assert!(!lock_path(&failing).exists());
    Ok(())
}

#[test]
fn the_lock_dir_is_private() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    seed(&dir, WireTarget::Claude, None)?;

    apply_wire(&request(
        &dir,
        WireTarget::Claude,
        WireAction::Wire,
        &everything_resolves,
    ))?;

    let lock_dir = lock_path(&dir)
        .parent()
        .ok_or("the lock has no directory")?
        .to_path_buf();
    assert_eq!(
        fs::metadata(lock_dir)?.permissions().mode() & 0o777,
        LOCK_DIR_MODE
    );
    Ok(())
}
