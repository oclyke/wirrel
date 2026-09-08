//! Pure core: fold the commit stream into articles, validate, plan redirects.

use crate::commits::{parse_subject, CommitKind};
use crate::frontmatter::Frontmatter;
use crate::git::{FileChange, RawCommit};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub type Id = u32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Article {
    // Derived from the commit stream by `fold`.
    pub id: Id,
    pub slug: String,
    pub path: String,
    pub created: String,
    pub updated: String,
    pub past_slugs: Vec<String>,

    // Derived from the working-tree file; empty until `enrich_from_files` runs.
    pub frontmatter: Frontmatter,
    /// The resolved title: the frontmatter `title`, else the first heading.
    pub title: Option<String>,
}

/// What one commit did to one article. An `update:` touching two articles
/// yields two of these, sharing a sha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub article: Id,
    pub sha: String,
    pub date: String,
    pub description: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ChangeKind {
    Create,
    Update,
    /// Slugs, not paths: a rename that leaves the route alone has `from == to`.
    Move { from: String, to: String },
}

impl ChangeKind {
    pub fn label(&self) -> &'static str {
        match self {
            ChangeKind::Create => "create",
            ChangeKind::Update => "update",
            ChangeKind::Move { .. } => "move",
        }
    }
}

#[derive(Debug, Default)]
pub struct Model {
    pub articles: Vec<Article>,
    /// Every article-affecting commit, oldest first.
    pub changes: Vec<Change>,
}

impl Model {
    pub fn by_slug(&self, slug: &str) -> Option<&Article> {
        self.articles.iter().find(|a| a.slug == slug)
    }

    pub fn by_id(&self, id: Id) -> Option<&Article> {
        self.articles.iter().find(|a| a.id == id)
    }

    pub fn max_id(&self) -> Id {
        self.articles.iter().map(|a| a.id).max().unwrap_or(0)
    }

    /// One article's changes, oldest first.
    pub fn changes_for(&self, id: Id) -> impl Iterator<Item = &Change> {
        self.changes.iter().filter(move |c| c.article == id)
    }

    /// What one commit did — more than one article for a multi-file `update:`.
    pub fn changes_in<'a>(&'a self, sha: &'a str) -> impl Iterator<Item = &'a Change> {
        self.changes.iter().filter(move |c| c.sha == sha)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Violation {
    BadSubject { sha: String, subject: String },
    CreateWithoutFile { sha: String },
    CreateAddsMultipleArticles { sha: String },
    CreateTouchesExistingArticle { sha: String, path: String },
    UpdateAddsArticle { sha: String, path: String },
    UpdateBeforeCreate { sha: String, path: String },
    MoveOfUnknown { sha: String, path: String },
    MoveTouchesMultipleArticles { sha: String },
    MetaTouchesArticle { sha: String, path: String },
    SlugCollision { slug: String, ids: (Id, Id) },
    NonMarkdownInRoot { path: String },
    DanglingIdLink { path: String, id: Id },
    MissingTitle { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Blocks `build` and `link`.
    Error,
    /// Reported but non-blocking.
    Warning,
}

impl Violation {
    pub fn severity(&self) -> Severity {
        match self {
            // These don't corrupt the output; titles fall back to the slug.
            Violation::MissingTitle { .. } | Violation::NonMarkdownInRoot { .. } => {
                Severity::Warning
            }
            _ => Severity::Error,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Violation::BadSubject { sha, subject } => {
                format!("{}: subject not `<type>: <description>`: {subject:?}", short(sha))
            }
            Violation::CreateWithoutFile { sha } => {
                format!("{}: `create:` added no markdown file", short(sha))
            }
            Violation::CreateAddsMultipleArticles { sha } => {
                format!("{}: `create:` adds more than one article; split it", short(sha))
            }
            Violation::CreateTouchesExistingArticle { sha, path } => {
                format!("{}: `create:` must only add; it changes {path}", short(sha))
            }
            Violation::UpdateAddsArticle { sha, path } => {
                format!("{}: `update:` adds {path}; a new article needs `create:`", short(sha))
            }
            Violation::UpdateBeforeCreate { sha, path } => {
                format!("{}: `update:` touched {path} before any `create:`", short(sha))
            }
            Violation::MoveOfUnknown { sha, path } => {
                format!("{}: `move:` renamed unknown file {path}", short(sha))
            }
            Violation::MoveTouchesMultipleArticles { sha } => {
                format!("{}: `move:` touches more than one article; split it", short(sha))
            }
            Violation::MetaTouchesArticle { sha, path } => {
                format!("{}: `meta:` must not change article {path}", short(sha))
            }
            Violation::SlugCollision { slug, ids } => {
                format!("slug {slug:?} claimed by both id {} and id {}", ids.0, ids.1)
            }
            Violation::NonMarkdownInRoot { path } => {
                format!("{path}: non-markdown file under the article root")
            }
            Violation::DanglingIdLink { path, id } => {
                format!("{path}: link references nonexistent id {id}")
            }
            Violation::MissingTitle { path } => {
                format!("{path}: no frontmatter `title` and no level-1 heading")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Redirect {
    pub from: String,
    pub to: String,
}

pub fn build_model(commits: &[RawCommit], root: &str) -> Model {
    fold(commits, root).0
}

/// Structural checks (grammar, lineage, slug collisions).
pub fn check(commits: &[RawCommit], root: &str) -> Vec<Violation> {
    fold(commits, root).1
}

/// Lint tracked files under the article root: everything there should be
/// markdown. The assets directory is the exception — it exists to hold the
/// things that aren't — and only matters when it sits inside the article root,
/// which the default layout keeps it out of.
pub fn check_tracked_files(root: &str, assets: &str, files: &[String]) -> Vec<Violation> {
    let prefix = root_prefix(root);
    let assets = root_prefix(assets);
    files
        .iter()
        .filter(|f| f.starts_with(&prefix) && !f.ends_with(".md"))
        .filter(|f| assets.is_empty() || !f.starts_with(&assets))
        .map(|f| Violation::NonMarkdownInRoot { path: f.clone() })
        .collect()
}

/// Content checks for one article file: missing title and dangling id links.
pub fn check_content(model: &Model, path: &str, content: &str) -> Vec<Violation> {
    let mut violations = Vec::new();

    if crate::markdown::extract_title(content).is_none() {
        violations.push(Violation::MissingTitle { path: path.to_string() });
    }

    let max = model.max_id();
    violations.extend(
        crate::markdown::id_refs(content)
            .into_iter()
            .filter(|&id| id == 0 || id > max || model.by_id(id).is_none())
            .map(|id| Violation::DanglingIdLink { path: path.to_string(), id }),
    );

    violations
}

/// Id permalinks, for hosts that can serve a real redirect; `build` also writes
/// a stub page at each. Slug history is deliberately not redirected: a retired
/// slug gets a tombstone page instead, so a URL that changed hands never has to
/// pick a winner. See `retired_slugs`.
pub fn plan_redirects(model: &Model) -> Vec<Redirect> {
    model
        .articles
        .iter()
        .flat_map(|a| {
            let to = crate::routes::article(&a.slug);
            [
                Redirect { from: crate::routes::article_by_id(a.id), to: to.clone() },
                Redirect { from: crate::routes::id_permalink(a.id), to },
            ]
        })
        .collect()
}

/// Slugs some article has moved away from and no live article has reclaimed,
/// deduped and sorted. A tombstone says only that something moved on, so two
/// lineages sharing a retired slug need no disambiguation.
pub fn retired_slugs(model: &Model) -> Vec<String> {
    let live: HashSet<&str> = model.articles.iter().map(|a| a.slug.as_str()).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for a in &model.articles {
        for past in &a.past_slugs {
            if !live.contains(past.as_str()) && seen.insert(past.as_str()) {
                out.push(past.clone());
            }
        }
    }
    out.sort();
    out
}

fn fold(commits: &[RawCommit], root: &str) -> (Model, Vec<Violation>) {
    let mut model = Model::default();
    let mut violations = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut next_id: Id = 1;

    for c in commits {
        let subject = match parse_subject(&c.subject) {
            Ok(s) => s,
            Err(_) => {
                violations.push(Violation::BadSubject {
                    sha: c.sha.clone(),
                    subject: c.subject.clone(),
                });
                continue;
            }
        };

        match subject.kind {
            CommitKind::Meta => {
                for path in c.changed.iter().flat_map(changed_paths) {
                    if by_path.contains_key(path) {
                        violations.push(Violation::MetaTouchesArticle {
                            sha: c.sha.clone(),
                            path: path.clone(),
                        });
                    }
                }
            }

            CommitKind::Create => {
                // Changing a file that already exists is update or move work.
                for f in &c.changed {
                    let path = match f {
                        FileChange::Modified(p) if is_md(p) => p,
                        FileChange::Renamed { from, to } if is_md(from) || is_md(to) => from,
                        _ => continue,
                    };
                    violations.push(Violation::CreateTouchesExistingArticle {
                        sha: c.sha.clone(),
                        path: path.clone(),
                    });
                }

                // Each article needs its own id, so one `create:` means one file.
                let added: Vec<String> = c.changed.iter().filter_map(added_md).collect();
                match added.len() {
                    0 => {
                        violations.push(Violation::CreateWithoutFile { sha: c.sha.clone() });
                        continue;
                    }
                    1 => {}
                    _ => violations
                        .push(Violation::CreateAddsMultipleArticles { sha: c.sha.clone() }),
                }

                for path in added {
                    let slug = slug_of(root, &path);

                    if let Some(other) = slug_owner(&model, &by_path, &slug, None) {
                        violations.push(Violation::SlugCollision {
                            slug: slug.clone(),
                            ids: (other, next_id),
                        });
                    }

                    let idx = model.articles.len();
                    model.articles.push(Article {
                        id: next_id,
                        slug,
                        path: path.clone(),
                        created: c.date.clone(),
                        updated: c.date.clone(),
                        past_slugs: Vec::new(),
                        frontmatter: Frontmatter::default(),
                        title: None,
                    });
                    model.changes.push(change(c, &subject, next_id, ChangeKind::Create));
                    by_path.insert(path, idx);
                    next_id += 1;
                }
            }

            CommitKind::Update => {
                for f in &c.changed {
                    // A new article needs its own `create:` to earn an id.
                    if let Some(p) = added_md(f) {
                        violations
                            .push(Violation::UpdateAddsArticle { sha: c.sha.clone(), path: p });
                        continue;
                    }
                    let Some(p) = touched_md(f) else { continue };
                    match by_path.get(&p) {
                        Some(&idx) => {
                            model.articles[idx].updated = c.date.clone();
                            let id = model.articles[idx].id;
                            model.changes.push(change(c, &subject, id, ChangeKind::Update));
                        }
                        None => violations.push(Violation::UpdateBeforeCreate {
                            sha: c.sha.clone(),
                            path: p,
                        }),
                    }
                }
            }

            CommitKind::Move => {
                let mut touched: Vec<usize> = Vec::new();
                for f in &c.changed {
                    let FileChange::Renamed { from, to } = f else { continue };
                    if !is_md(from) && !is_md(to) {
                        continue;
                    }
                    let Some(idx) = by_path.remove(from) else {
                        violations.push(Violation::MoveOfUnknown {
                            sha: c.sha.clone(),
                            path: from.clone(),
                        });
                        continue;
                    };

                    let new_slug = slug_of(root, to);
                    let old_slug = {
                        let art = &mut model.articles[idx];
                        let old = art.slug.clone();
                        if art.slug != new_slug {
                            art.past_slugs.push(art.slug.clone());
                        }
                        art.slug = new_slug.clone();
                        art.path = to.clone();
                        art.updated = c.date.clone();
                        old
                    };
                    let kind = ChangeKind::Move { from: old_slug, to: new_slug.clone() };
                    model.changes.push(change(c, &subject, model.articles[idx].id, kind));
                    by_path.insert(to.clone(), idx);
                    if !touched.contains(&idx) {
                        touched.push(idx);
                    }

                    let id = model.articles[idx].id;
                    if let Some(other) = slug_owner(&model, &by_path, &new_slug, Some(id)) {
                        violations.push(Violation::SlugCollision {
                            slug: new_slug,
                            ids: (other, id),
                        });
                    }
                }
                if touched.len() > 1 {
                    violations.push(Violation::MoveTouchesMultipleArticles { sha: c.sha.clone() });
                }
            }
        }
    }

    (model, violations)
}

fn change(c: &RawCommit, subject: &crate::commits::Subject, article: Id, kind: ChangeKind) -> Change {
    Change {
        article,
        sha: c.sha.clone(),
        date: c.date.clone(),
        description: subject.description.clone(),
        kind,
    }
}

fn slug_owner(
    model: &Model,
    by_path: &HashMap<String, usize>,
    slug: &str,
    exclude: Option<Id>,
) -> Option<Id> {
    by_path
        .values()
        .map(|&i| &model.articles[i])
        .find(|a| a.slug == slug && Some(a.id) != exclude)
        .map(|a| a.id)
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn is_md(path: &str) -> bool {
    path.ends_with(".md")
}

/// `foo/` for a root of `foo`, `./foo` or `foo/`; empty for the repo root.
fn root_prefix(root: &str) -> String {
    let root = root.trim_end_matches('/');
    let root = root.strip_prefix("./").unwrap_or(root);
    if root.is_empty() || root == "." {
        String::new()
    } else {
        format!("{root}/")
    }
}

/// Output route for a source path: strip the article root and `.md`, and collapse
/// `index` so `a/index.md` -> `a` while `a/b.md` -> `a/b`.
fn slug_of(root: &str, path: &str) -> String {
    let p = path.strip_prefix(&root_prefix(root)).unwrap_or(path);
    let p = p.strip_suffix(".md").unwrap_or(p);
    if p == "index" {
        String::new()
    } else if let Some(dir) = p.strip_suffix("/index") {
        dir.to_string()
    } else {
        p.to_string()
    }
}

fn added_md(f: &FileChange) -> Option<String> {
    match f {
        FileChange::Added(p) if is_md(p) => Some(p.clone()),
        _ => None,
    }
}

fn touched_md(f: &FileChange) -> Option<String> {
    match f {
        FileChange::Modified(p) if is_md(p) => Some(p.clone()),
        FileChange::Renamed { to, .. } if is_md(to) => Some(to.clone()),
        _ => None,
    }
}

/// Every repo path a change references (both sides of a rename).
fn changed_paths(f: &FileChange) -> Vec<&String> {
    match f {
        FileChange::Added(p) | FileChange::Modified(p) => vec![p],
        FileChange::Renamed { from, to } => vec![from, to],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, RawCommit};

    fn c(subject: &str, changed: Vec<FileChange>) -> RawCommit {
        RawCommit {
            sha: "0".repeat(40),
            subject: subject.into(),
            body: String::new(),
            date: "2026-01-01T00:00:00Z".into(),
            changed,
        }
    }

    fn added(p: &str) -> FileChange {
        FileChange::Added(p.into())
    }

    fn modified(p: &str) -> FileChange {
        FileChange::Modified(p.into())
    }

    fn renamed(from: &str, to: &str) -> FileChange {
        FileChange::Renamed { from: from.into(), to: to.into() }
    }

    #[test]
    fn ids_by_create_order_slug_from_path() {
        let h = [
            c("create: willow", vec![added("src/willow.md")]),
            c("create: direction", vec![added("src/choosing-direction.md")]),
        ];
        let m = build_model(&h, "src");
        assert_eq!(m.by_slug("willow").unwrap().id, 1);
        assert_eq!(m.by_id(2).unwrap().slug, "choosing-direction");
    }

    #[test]
    fn update_before_create_flagged() {
        let h = [c("update: edits", vec![modified("src/foo.md")])];
        let v = check(&h, "src");
        assert!(matches!(v.as_slice(), [Violation::UpdateBeforeCreate { .. }]));
    }

    #[test]
    fn meta_touching_an_article_flagged() {
        let h = [
            c("create: willow", vec![added("src/willow.md")]),
            c("meta: tweak", vec![modified("src/willow.md")]),
        ];
        let v = check(&h, "src");
        assert!(v.iter().any(|x| matches!(x, Violation::MetaTouchesArticle { .. })));
    }

    #[test]
    fn meta_touching_non_article_ok() {
        let h = [
            c("create: willow", vec![added("src/willow.md")]),
            c("meta: docs", vec![modified("README.md")]),
        ];
        let v = check(&h, "src");
        assert!(!v.iter().any(|x| matches!(x, Violation::MetaTouchesArticle { .. })));
    }

    #[test]
    fn move_preserves_id_and_records_past_slug() {
        let h = [
            c("create: direction", vec![added("src/direction.md")]),
            c("move: rename", vec![renamed("src/direction.md", "src/choosing-direction.md")]),
        ];
        let m = build_model(&h, "src");
        let a = m.by_slug("choosing-direction").unwrap();
        assert_eq!(a.id, 1);
        assert_eq!(a.past_slugs, ["direction"]);
        assert!(check(&h, "src").is_empty());
    }

    #[test]
    fn live_slug_collision_flagged() {
        // `src/dup.md` and `src/dup/index.md` both route to `dup`.
        let h = [
            c("create: a", vec![added("src/dup.md")]),
            c("create: b", vec![added("src/dup/index.md")]),
        ];
        let v = check(&h, "src");
        assert!(v.iter().any(|x| matches!(x, Violation::SlugCollision { .. })));
    }

    #[test]
    fn nested_and_index_routing() {
        let h = [
            c("create: idx", vec![added("src/parent/index.md")]),
            c("create: art", vec![added("src/parent/article.md")]),
            c("create: flat", vec![added("src/willow.md")]),
        ];
        let m = build_model(&h, "src");
        assert_eq!(m.by_id(1).unwrap().slug, "parent");
        assert_eq!(m.by_id(2).unwrap().slug, "parent/article");
        assert_eq!(m.by_id(3).unwrap().slug, "willow");
    }

    #[test]
    fn redirects_are_id_permalinks_only() {
        let h = [
            c("create: direction", vec![added("src/direction.md")]),
            c("move: rename", vec![renamed("src/direction.md", "src/choosing-direction.md")]),
        ];
        let reds = plan_redirects(&build_model(&h, "src"));
        assert_eq!(
            reds,
            [
                Redirect {
                    from: "/articles/by-id/1/".into(),
                    to: "/article/choosing-direction/".into()
                },
                Redirect { from: "/id/1/".into(), to: "/article/choosing-direction/".into() },
            ]
        );
    }

    #[test]
    fn reclaimed_slug_is_not_retired() {
        // Article 2 now lives at `direction`; its live page owns that path.
        let h = [
            c("create: direction", vec![added("src/direction.md")]),
            c("move: rename", vec![renamed("src/direction.md", "src/choosing-direction.md")]),
            c("create: new", vec![added("src/direction.md")]),
        ];
        assert!(retired_slugs(&build_model(&h, "src")).is_empty());
    }

    #[test]
    fn retired_slugs_collect_chains_and_dedupe_lineages() {
        let h = [
            // A chain: every slug the article passed through is retired.
            c("create: a", vec![added("src/alpha.md")]),
            c("move: a->b", vec![renamed("src/alpha.md", "src/bravo.md")]),
            c("move: b->c", vec![renamed("src/bravo.md", "src/charlie.md")]),
            // A second lineage that also passed through `alpha`.
            c("create: a again", vec![added("src/alpha.md")]),
            c("move: a->d", vec![renamed("src/alpha.md", "src/delta.md")]),
        ];
        assert_eq!(retired_slugs(&build_model(&h, "src")), ["alpha", "bravo"]);
    }

    #[test]
    fn slug_returned_to_is_not_retired() {
        // alpha -> bravo -> alpha: the article is live at `alpha` again.
        let h = [
            c("create: a", vec![added("src/alpha.md")]),
            c("move: a->b", vec![renamed("src/alpha.md", "src/bravo.md")]),
            c("move: b->a", vec![renamed("src/bravo.md", "src/alpha.md")]),
        ];
        assert_eq!(retired_slugs(&build_model(&h, "src")), ["bravo"]);
    }

    #[test]
    fn content_check_finds_missing_title_and_dangling_link() {
        let m = build_model(&[c("create: willow", vec![added("src/willow.md")])], "src");
        let v = check_content(&m, "src/willow.md", "no heading [x](id:99)");
        assert!(v.iter().any(|x| matches!(x, Violation::MissingTitle { .. })));
        assert!(v.iter().any(|x| matches!(x, Violation::DanglingIdLink { id: 99, .. })));
    }

    #[test]
    fn render_hydrates_id_link() {
        let m = build_model(&[c("create: willow", vec![added("src/willow.md")])], "src");
        let html = crate::markdown::render("[w](id:1)", &m);
        assert!(html.contains("href=\"/article/willow/\""));
    }

    #[test]
    fn create_adding_multiple_articles_flagged() {
        let h = [c(
            "create: two at once",
            vec![added("src/willow.md"), added("src/oak.md")],
        )];
        let v = check(&h, "src");
        assert!(v.iter().any(|x| matches!(x, Violation::CreateAddsMultipleArticles { .. })));
    }

    #[test]
    fn create_changing_an_existing_article_flagged() {
        let h = [
            c("create: willow", vec![added("src/willow.md")]),
            c("create: oak", vec![added("src/oak.md"), modified("src/willow.md")]),
        ];
        let v = check(&h, "src");
        assert!(v.iter().any(|x| matches!(x, Violation::CreateTouchesExistingArticle { path, .. } if path == "src/willow.md")));
    }

    #[test]
    fn update_adding_an_article_flagged() {
        let h = [
            c("create: willow", vec![added("src/willow.md")]),
            c("update: edits", vec![modified("src/willow.md"), added("src/oak.md")]),
        ];
        let v = check(&h, "src");
        assert!(v.iter().any(|x| matches!(x, Violation::UpdateAddsArticle { path, .. } if path == "src/oak.md")));
        // The specific message replaces the misleading "before any create".
        assert!(!v.iter().any(|x| matches!(x, Violation::UpdateBeforeCreate { .. })));
    }

    #[test]
    fn move_touching_multiple_articles_flagged() {
        // `dir/index.md` (article `dir`) and `dir/child.md` (article `dir/child`)
        // are two independent nested articles; a directory rename touches both.
        let h = [
            c("create: idx", vec![added("src/dir/index.md")]),
            c("create: child", vec![added("src/dir/child.md")]),
            c(
                "move: reorg",
                vec![
                    renamed("src/dir/index.md", "src/moved/index.md"),
                    renamed("src/dir/child.md", "src/moved/child.md"),
                ],
            ),
        ];
        let v = check(&h, "src");
        assert!(v.iter().any(|x| matches!(x, Violation::MoveTouchesMultipleArticles { .. })));
    }

    #[test]
    fn move_of_single_nested_article_ok() {
        let h = [
            c("create: idx", vec![added("src/dir/index.md")]),
            c("create: child", vec![added("src/dir/child.md")]),
            c("move: rename one", vec![renamed("src/dir/child.md", "src/dir/renamed.md")]),
        ];
        let v = check(&h, "src");
        assert!(!v.iter().any(|x| matches!(x, Violation::MoveTouchesMultipleArticles { .. })));
    }

    #[test]
    fn non_markdown_under_root_flagged() {
        let files = vec![
            "articles/willow.md".to_string(),
            "articles/notes.txt".to_string(),
            "README.md".to_string(),        // outside root
            "assets/style.css".to_string(), // outside root
        ];
        let v = check_tracked_files("./articles", "./assets", &files);
        assert!(matches!(v.as_slice(), [Violation::NonMarkdownInRoot { path }] if path == "articles/notes.txt"));
    }

    #[test]
    fn assets_exempt_where_they_sit_inside_the_article_root() {
        let files = vec!["assets/style.css".to_string(), "notes.txt".to_string()];
        let v = check_tracked_files(".", "assets", &files);
        assert!(matches!(v.as_slice(), [Violation::NonMarkdownInRoot { path }] if path == "notes.txt"));

        // Somewhere else entirely, and the exemption follows it.
        let files = vec!["static/logo.png".to_string()];
        assert!(check_tracked_files(".", "./static", &files).is_empty());
    }

    #[test]
    fn leading_dot_slash_is_the_same_root() {
        let h = [c("create: willow", vec![added("articles/willow.md")])];
        assert_eq!(build_model(&h, "./articles").by_id(1).unwrap().slug, "willow");
    }
}
