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

    let prose = "# willow\n".to_string() + &"a line of prose\n".repeat(8);
    fs::write(repo.join("src/willow.md"), &prose).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "create: the willow"]);

    let commits = wirrel::git::load(repo).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "create: the willow");
    assert!(matches!(
        commits[0].changed.as_slice(),
        [wirrel::git::FileChange::Added(p)] if p == "src/willow.md"
    ));
    assert_eq!(
        commits[0].stats,
        [wirrel::git::FileStat { path: "src/willow.md".into(), added: 9, deleted: 0 }]
    );

    // A rename's stat is keyed by the path the file has afterwards.
    git(repo, &["mv", "src/willow.md", "src/oak.md"]);
    fs::write(repo.join("src/oak.md"), prose.replace("# willow", "# oak")).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "move: to oak"]);
    let commits = wirrel::git::load(repo).unwrap();
    assert_eq!(
        commits[1].stats,
        [wirrel::git::FileStat { path: "src/oak.md".into(), added: 1, deleted: 1 }]
    );
}

#[test]
fn errors_on_non_repo() {
    let dir = tempfile::tempdir().unwrap();
    assert!(wirrel::git::load(dir.path()).is_err()); // exists but not a git repo
    assert!(wirrel::git::load(Path::new("/no/such/path")).is_err());
}
