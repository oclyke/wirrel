//! Integration test for the git boundary; the only test that touches real git.

use std::fs;
use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

#[test]
fn loads_and_parses_history() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir(repo.join("src")).unwrap();

    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t.dev"]);
    git(repo, &["config", "user.name", "t"]);

    fs::write(repo.join("src/willow.md"), "# willow\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "create: the willow"]);

    let commits = wirrel::git::load(repo).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "create: the willow");
    assert!(matches!(
        commits[0].changed.as_slice(),
        [wirrel::git::FileChange::Added(p)] if p == "src/willow.md"
    ));
}

#[test]
fn errors_on_non_repo() {
    let dir = tempfile::tempdir().unwrap();
    assert!(wirrel::git::load(dir.path()).is_err()); // exists but not a git repo
    assert!(wirrel::git::load(Path::new("/no/such/path")).is_err());
}
