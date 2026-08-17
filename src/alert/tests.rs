use super::*;

fn nothing_exists(_path: &Path) -> bool {
    false
}

fn everything_exists(_path: &Path) -> bool {
    true
}

#[test]
fn an_installed_binary_reads_the_share_directory_of_its_prefix() {
    let share_dir = share_dir_for(
        Some(Path::new("/usr/local/bin/agent-notifier")),
        Some(Path::new("/home/agent")),
        &everything_exists,
    );

    assert_eq!(share_dir, Path::new("/usr/local/share/agent-notifier"));
}

#[test]
fn a_cargo_installed_binary_falls_back_to_the_user_share_directory() {
    let share_dir = share_dir_for(
        Some(Path::new("/home/agent/.cargo/bin/agent-notifier")),
        Some(Path::new("/home/agent")),
        &nothing_exists,
    );

    assert_eq!(
        share_dir,
        Path::new("/home/agent/.local/share/agent-notifier")
    );
}

#[test]
fn an_unlocatable_binary_falls_back_to_the_user_share_directory() {
    let share_dir = share_dir_for(None, Some(Path::new("/home/agent")), &everything_exists);

    assert_eq!(
        share_dir,
        Path::new("/home/agent/.local/share/agent-notifier")
    );
}

#[test]
fn the_sound_file_override_wins_over_every_directory() {
    let file = sound_file_for(
        Some(OsStr::new("/tmp/ping.mp3")),
        Some(OsStr::new("/tmp/sounds")),
        Path::new("/usr/share/agent-notifier"),
    );

    assert_eq!(file, Path::new("/tmp/ping.mp3"));
}

#[test]
fn the_sound_directory_override_wins_over_the_share_directory() {
    let file = sound_file_for(
        None,
        Some(OsStr::new("/tmp/sounds")),
        Path::new("/usr/share/agent-notifier"),
    );

    assert_eq!(file, Path::new("/tmp/sounds/agent-complete.mp3"));
}

#[test]
fn the_share_directory_holds_the_sound_when_no_override_is_set() {
    let file = sound_file_for(None, None, Path::new("/usr/share/agent-notifier"));

    assert_eq!(
        file,
        Path::new("/usr/share/agent-notifier/agent-complete.mp3")
    );
}

#[test]
fn an_empty_override_is_no_override() {
    let file = sound_file_for(
        Some(OsStr::new("")),
        Some(OsStr::new("")),
        Path::new("/usr/share/agent-notifier"),
    );

    assert_eq!(
        file,
        Path::new("/usr/share/agent-notifier/agent-complete.mp3")
    );
}

#[test]
fn only_zero_mutes_the_completion_sound() {
    assert!(sound_is_muted(Some(OsStr::new("0"))));
    assert!(!sound_is_muted(Some(OsStr::new("1"))));
    assert!(!sound_is_muted(None));
}
