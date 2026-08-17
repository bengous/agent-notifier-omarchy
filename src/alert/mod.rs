use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::thread;

use crate::exec::{run_command, run_command_owned, DEFAULT_TIMEOUT};

const SOUND_FILE_NAME: &str = "agent-complete.mp3";

/// Tell the user a turn is over: one notification, one sound, both at once.
pub(crate) fn alert(app_name: &str, title: &str, body: &str) {
    let notification = [
        "notify-send".to_owned(),
        format!("--app-name={app_name}"),
        title.to_owned(),
        body.to_owned(),
    ];
    let sound = thread::spawn(play_sound);
    let notify = thread::spawn(move || {
        let _ = run_command_owned(&notification, DEFAULT_TIMEOUT);
    });
    let _ = sound.join();
    let _ = notify.join();
}

fn play_sound() {
    if sound_is_muted(env::var_os("AGENT_NOTIFIER_SOUND").as_deref()) {
        return;
    }
    let file = sound_file().to_string_lossy().into_owned();
    if run_command(
        &["mpv", "--no-video", "--really-quiet", &file],
        DEFAULT_TIMEOUT,
    )
    .unwrap_or(1)
        == 0
    {
        return;
    }
    let _ = run_command(&["canberra-gtk-play", "-f", &file], DEFAULT_TIMEOUT);
}

fn sound_file() -> PathBuf {
    let file = env::var_os("AGENT_NOTIFIER_SOUND_FILE");
    let dir = env::var_os("AGENT_NOTIFIER_SOUND_DIR");
    sound_file_for(file.as_deref(), dir.as_deref(), &share_dir())
}

fn share_dir() -> PathBuf {
    let override_dir = env::var_os("AGENT_NOTIFIER_SHARE_DIR");
    if let Some(dir) = set(override_dir.as_deref()) {
        return PathBuf::from(dir);
    }
    let exe = env::current_exe().ok();
    let home = env::var_os("HOME").map(PathBuf::from);
    share_dir_for(exe.as_deref(), home.as_deref(), &|path| path.is_dir())
}

fn sound_is_muted(setting: Option<&OsStr>) -> bool {
    setting == Some(OsStr::new("0"))
}

fn sound_file_for(file: Option<&OsStr>, dir: Option<&OsStr>, share_dir: &Path) -> PathBuf {
    if let Some(file) = set(file) {
        return PathBuf::from(file);
    }
    set(dir)
        .map_or_else(|| share_dir.to_path_buf(), PathBuf::from)
        .join(SOUND_FILE_NAME)
}

/// An installed `<prefix>/bin/agent-notifier` ships its data in
/// `<prefix>/share/agent-notifier`, so the binary answers where its data is.
/// Reading the prefix off the binary beats matching its path: `cargo install`
/// lands in `~/.cargo/bin`, which no `.local` pattern recognises.
fn share_dir_for(
    exe: Option<&Path>,
    home: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    exe.and_then(Path::parent)
        .and_then(Path::parent)
        .map(|prefix| prefix.join("share/agent-notifier"))
        .filter(|dir| exists(dir))
        .unwrap_or_else(|| user_share_dir(home))
}

fn user_share_dir(home: Option<&Path>) -> PathBuf {
    home.unwrap_or_else(|| Path::new(""))
        .join(".local/share/agent-notifier")
}

fn set(value: Option<&OsStr>) -> Option<&OsStr> {
    value.filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;
