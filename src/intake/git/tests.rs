use super::*;
use tempfile::tempdir;

#[test]
fn a_worktree_common_dir_resolves_to_the_main_repository() {
    assert_eq!(
        main_repository_root("/repo/dotfiles/.git").as_deref(),
        Some("/repo/dotfiles")
    );
}

#[test]
fn a_bare_repository_path_stays_the_key() {
    assert_eq!(
        main_repository_root("/repo/dotfiles.git").as_deref(),
        Some("/repo/dotfiles.git")
    );
}

#[test]
fn a_git_directory_without_a_parent_yields_no_key() {
    assert_eq!(main_repository_root(""), None);
    assert_eq!(main_repository_root(".git"), None);
}

#[test]
fn outside_a_repository_the_key_is_the_project_path() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;

    assert_eq!(
        repository_key(&dir.path().to_string_lossy(), "/repo/dotfiles"),
        "/repo/dotfiles"
    );
    Ok(())
}
