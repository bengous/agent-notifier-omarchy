use std::ffi::OsStr;
use std::path::Path;

use crate::exec::command_output;

pub(in crate::intake) fn project_root(cwd: &str) -> String {
    command_output(["git", "-C", cwd, "rev-parse", "--show-toplevel"])
        .filter(|output| !output.is_empty())
        .unwrap_or_else(|| cwd.to_owned())
}

/// Every worktree of one repository shares the main repository as its key.
pub(in crate::intake) fn repository_key(cwd: &str, project_path: &str) -> String {
    command_output([
        "git",
        "-C",
        cwd,
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])
    .as_deref()
    .and_then(main_repository_root)
    .unwrap_or_else(|| project_path.to_owned())
}

fn main_repository_root(git_common_dir: &str) -> Option<String> {
    if git_common_dir.is_empty() {
        return None;
    }
    let path = Path::new(git_common_dir);
    if path.file_name() != Some(OsStr::new(".git")) {
        return Some(git_common_dir.to_owned());
    }
    path.parent()
        .map(|root| root.to_string_lossy().into_owned())
        .filter(|root| !root.is_empty())
}

pub(in crate::intake) fn current_git_branch(cwd: &str) -> Option<String> {
    command_output(["git", "-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
}

#[cfg(test)]
mod tests;
