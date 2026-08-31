//! End-to-end build test, driving the real binary.

use std::fs;
use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(repo).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed");
}

fn commit(repo: &Path, msg: &str) {
    git(repo, &["add", "-A"]);
    git(repo, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg]);
}

#[test]
fn builds_nested_articles() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src/parent")).unwrap();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t.dev"]);
    git(repo, &["config", "user.name", "t"]);

    // parent/index.md -> /parent/ ; parent/article.md -> /parent/article/
    fs::write(repo.join("src/parent/index.md"), "# parent\n").unwrap();
    commit(repo, "create: parent index");
    fs::write(repo.join("src/parent/article.md"), "# article\nsee [p](/id/1/)\n").unwrap();
    commit(repo, "create: nested article");

    let out = repo.join("dist");
    let ok = Command::new(env!("CARGO_BIN_EXE_wirrel"))
        .arg("--repo")
        .arg(repo)
        .arg("--article-root")
        .arg("src")
        .arg("--no-verify-signatures")
        .arg("build")
        .arg("--out")
        .arg(&out)
        .status()
        .unwrap()
        .success();
    assert!(ok);

    assert!(out.join("parent/index.html").is_file());
    assert!(out.join("parent/article/index.html").is_file());

    // /id/1/ hydrates to the parent index's route.
    let article = fs::read_to_string(out.join("parent/article/index.html")).unwrap();
    assert!(article.contains("href=\"/parent/\""));
}

#[test]
fn redirects_command_emits_json_map() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t.dev"]);
    git(repo, &["config", "user.name", "t"]);
    fs::write(repo.join("src/willow.md"), "# willow\n").unwrap();
    commit(repo, "create: willow");

    let output = Command::new(env!("CARGO_BIN_EXE_wirrel"))
        .arg("--repo")
        .arg(repo)
        .arg("--article-root")
        .arg("src")
        .arg("--no-verify-signatures")
        .arg("redirects")
        .output()
        .unwrap();
    assert!(output.status.success());

    let json = String::from_utf8(output.stdout).unwrap();
    assert!(json.contains("\"from\": \"/id/1/\""));
    assert!(json.contains("\"to\": \"/willow/\""));
}

#[test]
fn builds_update_page_with_diff() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t.dev"]);
    git(repo, &["config", "user.name", "t"]);
    fs::write(repo.join("src/willow.md"), "# willow\nold line\n").unwrap();
    commit(repo, "create: willow");
    fs::write(repo.join("src/willow.md"), "# willow\nnew line\n").unwrap();
    commit(repo, "update: change line");

    let out = repo.join("dist");
    let ok = Command::new(env!("CARGO_BIN_EXE_wirrel"))
        .arg("--repo")
        .arg(repo)
        .arg("--article-root")
        .arg("src")
        .arg("--no-verify-signatures")
        .arg("build")
        .arg("--out")
        .arg(&out)
        .status()
        .unwrap()
        .success();
    assert!(ok);

    let sha_dir = fs::read_dir(out.join("updates")).unwrap().next().unwrap().unwrap().path();
    let html = fs::read_to_string(sha_dir.join("index.html")).unwrap();
    assert!(html.contains("update: change line"));
    assert!(html.contains("class=\"add\">+new line"));
    assert!(html.contains("class=\"del\">-old line"));
}

#[test]
fn frontmatter_title_drives_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t.dev"]);
    git(repo, &["config", "user.name", "t"]);

    // No h1 at all: the frontmatter title supplants it.
    fs::write(repo.join("src/willow.md"), "---\ntitle: A Willow\n---\njust prose\n").unwrap();
    commit(repo, "create: willow");
    // An h1 is present: the frontmatter title overrides it for the page title.
    fs::write(repo.join("src/oak.md"), "---\ntitle: Real Oak\n---\n# stale oak\n").unwrap();
    commit(repo, "create: oak");

    let out = repo.join("dist");
    let ok = Command::new(env!("CARGO_BIN_EXE_wirrel"))
        .arg("--repo")
        .arg(repo)
        .arg("--article-root")
        .arg("src")
        .arg("--no-verify-signatures")
        .arg("build")
        .arg("--out")
        .arg(&out)
        .status()
        .unwrap()
        .success();
    assert!(ok);

    let willow = fs::read_to_string(out.join("willow/index.html")).unwrap();
    assert!(willow.contains("<title>A Willow</title>"));
    assert!(willow.contains("<h1>A Willow</h1>"));
    assert!(!willow.contains("title: A Willow"), "frontmatter leaked into the body");

    let oak = fs::read_to_string(out.join("oak/index.html")).unwrap();
    assert!(oak.contains("<title>Real Oak</title>"));
    assert!(oak.contains("<h1>stale oak</h1>"));
    assert_eq!(oak.matches("<h1>").count(), 1, "duplicated the heading");
}
