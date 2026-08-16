use std::path::Path;
use std::process::Command;

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_owned())
}

fn rerun_if_exists(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn main() {
    let commit =
        git_output(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let dirty =
        git_output(&["status", "--porcelain", "--untracked-files=no"]).map_or("false", |_| "true");
    let commit_date =
        git_output(&["show", "-s", "--format=%cI", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=AGENT_NOTIFIER_COMMIT={commit}");
    println!("cargo:rustc-env=AGENT_NOTIFIER_DIRTY={dirty}");
    println!("cargo:rustc-env=AGENT_NOTIFIER_COMMIT_DATE={commit_date}");

    rerun_if_exists(Path::new(".git/HEAD"));
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(reference) = head.trim().strip_prefix("ref: ") {
            rerun_if_exists(&Path::new(".git").join(reference));
        }
    }
    rerun_if_exists(Path::new(".git/index"));
    println!("cargo:rerun-if-changed=src");
}
