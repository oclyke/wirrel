//! End-to-end build test, driving the real binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(repo: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(repo).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed");
}

fn commit(repo: &Path, msg: &str) {
    git(repo, &["add", "-A"]);
    git(repo, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg]);
}

fn init(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t.dev"]);
    git(repo, &["config", "user.name", "t"]);
}

fn wirrel(repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wirrel"))
        .env_remove("WIRREL_ARTICLES")
        .env_remove("WIRREL_ASSETS")
        .arg("--repo")
        .arg(repo)
        .args(["--articles", "src"])
        .args(args)
        .output()
        .unwrap()
}

fn build(repo: &Path) -> PathBuf {
    let out = repo.join("dist");
    let status = wirrel(repo, &["build", "--out", out.to_str().unwrap()]).status;
    assert!(status.success());
    out
}

#[test]
fn builds_nested_articles() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::create_dir_all(repo.join("src/parent")).unwrap();

    // parent/index.md -> /article/parent/ ; parent/article.md -> /article/parent/article/
    fs::write(repo.join("src/parent/index.md"), "# parent\n").unwrap();
    commit(repo, "create: parent index");
    fs::write(repo.join("src/parent/article.md"), "# article\nsee [p](/id/1/)\n").unwrap();
    commit(repo, "create: nested article");

    let out = build(repo);
    assert!(out.join("article/parent/index.html").is_file());
    assert!(out.join("article/parent/article/index.html").is_file());

    // /id/1/ hydrates to the parent index's route.
    let article = fs::read_to_string(out.join("article/parent/article/index.html")).unwrap();
    assert!(article.contains("href=\"/article/parent/\""));

    // The id permalink resolves without help from the host.
    let stub = fs::read_to_string(out.join("articles/by-id/1/index.html")).unwrap();
    assert!(stub.contains("url=/article/parent/"));
}

/// The point of the scheme: an article can be named for a generated address
/// without contesting it.
#[test]
fn article_slugs_cannot_claim_a_generated_address() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::write(repo.join("src/changelog.md"), "# notes on changelogs\nold\n").unwrap();
    commit(repo, "create: changelog");
    fs::write(repo.join("src/changelog.md"), "# notes on changelogs\nnew\n").unwrap();
    commit(repo, "update: revise");

    let out = build(repo);

    let generated = fs::read_to_string(out.join("changelog/index.html")).unwrap();
    assert!(generated.contains("<h1>changelog</h1>"));
    let article = fs::read_to_string(out.join("article/changelog/index.html")).unwrap();
    assert!(article.contains("notes on changelogs"));

    // Nothing but the reserved names sits at the root, so no slug can reach it.
    let mut root: Vec<String> =
        fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    root.sort();
    assert_eq!(
        root,
        ["404.html", "article", "articles", "assets", "changelog", "changes", "index.html"]
    );
}

#[test]
fn redirects_command_emits_json_map() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::write(repo.join("src/willow.md"), "# willow\n").unwrap();
    commit(repo, "create: willow");

    let output = wirrel(repo, &["redirects"]);
    assert!(output.status.success());

    let json = String::from_utf8(output.stdout).unwrap();
    assert!(json.contains("\"from\": \"/articles/by-id/1/\""));
    assert!(json.contains("\"from\": \"/id/1/\""));
    assert!(json.contains("\"to\": \"/article/willow/\""));
}

#[test]
fn moved_article_leaves_a_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::write(repo.join("src/direction.md"), "# direction\n").unwrap();
    commit(repo, "create: direction");
    fs::rename(repo.join("src/direction.md"), repo.join("src/choosing-direction.md")).unwrap();
    commit(repo, "move: rename");

    let out = build(repo);
    assert!(out.join("article/choosing-direction/index.html").is_file());

    // The old address stays honest: it says the article moved, not where to.
    let tombstone = fs::read_to_string(out.join("article/direction/index.html")).unwrap();
    assert!(tombstone.contains("this article has moved"));
    assert!(!tombstone.contains("choosing-direction"));
    assert!(tombstone.contains("noindex"));

    let missing = fs::read_to_string(out.join("404.html")).unwrap();
    assert!(missing.contains("no such article"));
}

#[test]
fn builds_change_page_with_diff() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::write(repo.join("src/willow.md"), "# willow\nold line\n").unwrap();
    commit(repo, "create: willow");
    fs::write(repo.join("src/willow.md"), "# willow\nnew line\n").unwrap();
    commit(repo, "update: change line");

    let out = build(repo);
    let pages: Vec<String> = fs::read_dir(out.join("changes/by-sha"))
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path().join("index.html")).unwrap())
        .collect();
    assert_eq!(pages.len(), 2, "one page each for the create and the update");

    let update = pages.iter().find(|p| p.contains("<h1>update: change line</h1>")).unwrap();
    assert!(update.contains("class=\"add\">+new line"));
    assert!(update.contains("class=\"del\">-old line"));
}

/// The article header offers a way into its own history, and every hop from
/// there resolves.
#[test]
fn article_links_to_its_changes_and_on_to_each_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::write(repo.join("src/direction.md"), "# Direction\nfirst\n").unwrap();
    commit(repo, "create: direction");
    fs::write(repo.join("src/direction.md"), "# Direction\nsecond\n").unwrap();
    commit(repo, "update: revise");
    fs::rename(repo.join("src/direction.md"), repo.join("src/choosing-direction.md")).unwrap();
    commit(repo, "move: rename");

    let out = build(repo);

    let article = fs::read_to_string(out.join("article/choosing-direction/index.html")).unwrap();
    assert!(article.contains("href=\"/changes/by-id/1/\">changes</a>"));

    // All three operations are listed, each linking to its own commit page.
    let changes = fs::read_to_string(out.join("changes/by-id/1/index.html")).unwrap();
    for kind in ["<b>create</b>", "<b>update</b>", "<b>move</b>"] {
        assert!(changes.contains(kind), "missing {kind}");
    }
    assert!(changes.contains("<code>direction</code> → <code>choosing-direction</code>"));

    // A commit page exists for every one of them, creates and moves included.
    let commits: Vec<_> = fs::read_dir(out.join("changes/by-sha")).unwrap().collect();
    assert_eq!(commits.len(), 3);

    let changelog = fs::read_to_string(out.join("changelog/index.html")).unwrap();
    assert!(changelog.contains("<code>direction</code> → <code>choosing-direction</code>"));
    // The slug it left is named, never offered as a link.
    assert!(!changelog.contains("href=\"/article/direction/\""));
}

/// The default layout: articles and assets as siblings under the repo root,
/// neither flag given.
#[test]
fn default_layout_needs_no_flags() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::create_dir_all(repo.join("articles")).unwrap();
    fs::create_dir_all(repo.join("assets")).unwrap();
    fs::write(repo.join("articles/willow.md"), "# willow\n").unwrap();
    fs::write(repo.join("assets/style.css"), "body { color: rebeccapurple }\n").unwrap();
    commit(repo, "create: willow");

    // This test is about the defaults, so the ambient env must not reach it.
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_wirrel"))
            .env_remove("WIRREL_ARTICLES")
            .env_remove("WIRREL_ASSETS")
            .arg("--repo")
            .arg(repo)
            .args(args)
            .output()
            .unwrap()
    };

    let out = repo.join("dist");
    assert!(run(&["build", "--out", out.to_str().unwrap()]).status.success());
    assert!(out.join("article/willow/index.html").is_file());
    assert_eq!(
        fs::read_to_string(out.join("assets/style.css")).unwrap(),
        "body { color: rebeccapurple }\n"
    );

    // Assets sit outside the article root, so they are never article candidates.
    let stderr = String::from_utf8(run(&["check"]).stderr).unwrap();
    assert!(!stderr.contains("non-markdown"), "{stderr}");
}

/// The stylesheet slot: wirrel fills it, and stands aside once the repo does.
#[test]
fn assets_are_copied_and_the_stylesheet_is_linked() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);
    fs::write(repo.join("src/willow.md"), "# willow\n").unwrap();
    commit(repo, "create: willow");

    let out = build(repo);
    // Root-relative, so it resolves from an article nested at any depth.
    let article = fs::read_to_string(out.join("article/willow/index.html")).unwrap();
    assert!(article.contains("<link rel=\"stylesheet\" href=\"/assets/style.css\">"));
    // Absent a stylesheet of its own, the repo gets wirrel's, diff rules and all.
    assert!(fs::read_to_string(out.join("assets/style.css")).unwrap().contains(".diff"));

    // Once the repo supplies assets they are copied verbatim, and win.
    fs::create_dir_all(repo.join("assets/sub")).unwrap();
    fs::write(repo.join("assets/style.css"), "body { color: rebeccapurple }\n").unwrap();
    fs::write(repo.join("assets/sub/note.txt"), "nested\n").unwrap();
    commit(repo, "meta: add assets");

    let out = build(repo);
    assert_eq!(
        fs::read_to_string(out.join("assets/style.css")).unwrap(),
        "body { color: rebeccapurple }\n"
    );
    assert!(out.join("assets/sub/note.txt").is_file());

    // Assets are not articles, so they draw no non-markdown complaint.
    let stderr = String::from_utf8(wirrel(repo, &["check"]).stderr).unwrap();
    assert!(!stderr.contains("non-markdown"), "{stderr}");
}

#[test]
fn frontmatter_title_drives_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init(repo);

    // No h1 at all: the frontmatter title supplants it.
    fs::write(repo.join("src/willow.md"), "---\ntitle: A Willow\n---\njust prose\n").unwrap();
    commit(repo, "create: willow");
    // An h1 is present: the frontmatter title overrides it for the page title.
    fs::write(repo.join("src/oak.md"), "---\ntitle: Real Oak\n---\n# stale oak\n").unwrap();
    commit(repo, "create: oak");

    let out = build(repo);

    let willow = fs::read_to_string(out.join("article/willow/index.html")).unwrap();
    assert!(willow.contains("<title>A Willow</title>"));
    assert!(willow.contains("<h1>A Willow</h1>"));
    assert!(!willow.contains("title: A Willow"), "frontmatter leaked into the body");

    let oak = fs::read_to_string(out.join("article/oak/index.html")).unwrap();
    assert!(oak.contains("<title>Real Oak</title>"));
    assert!(oak.contains("<h1>stale oak</h1>"));
    assert_eq!(oak.matches("<h1>").count(), 1, "duplicated the heading");
}
