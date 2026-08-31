//! Pure core: fold the commit stream into articles, validate, plan redirects.

use crate::commits::{parse_subject, CommitKind};
use crate::frontmatter::Frontmatter;
use crate::git::{FileChange, RawCommit, SigStatus};
use serde::Serialize;
use std::collections::HashMap;

pub type Id = u32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Article {
    // Derived from the commit stream by `fold`.
    pub id: Id,
    pub slug: String,
    pub path: String,
    pub created: String,
    pub updated: String,
    pub history: Vec<String>,
    pub past_slugs: Vec<String>,

    // Derived from the working-tree file; empty until `enrich_from_files` runs.
    pub frontmatter: Frontmatter,
    /// The resolved title: the frontmatter `title`, else the first heading.
    pub title: Option<String>,
}

#[derive(Debug, Default)]
pub struct Model {
    pub articles: Vec<Article>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Violation {
    BadSubject { sha: String, subject: String },
    BadSignature { sha: String },
    UnsignedCommit { sha: String },
    CreateWithoutFile { sha: String },
    UpdateBeforeCreate { sha: String, path: String },
    MoveOfUnknown { sha: String, path: String },
    MoveTouchesMultipleArticles { sha: String },
    MetaTouchesArticle { sha: String, path: String },
    SlugCollision { slug: String, ids: (Id, Id) },
    NonMarkdownInRoot { path: String },
    DanglingIdLink { path: String, id: Id },
    MissingTitle { path: String },
    AbsoluteSelfLink { path: String, url: String },
    RedirectCollision { slug: String, historical: Id, live: Id },
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
            Violation::UnsignedCommit { .. }
            | Violation::RedirectCollision { .. }
            | Violation::MissingTitle { .. }
            | Violation::AbsoluteSelfLink { .. }
            | Violation::NonMarkdownInRoot { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Violation::BadSubject { sha, subject } => {
                format!("{}: subject not `<type>: <description>`: {subject:?}", short(sha))
            }
            Violation::BadSignature { sha } => {
                format!("{}: commit signature failed to verify", short(sha))
            }
            Violation::UnsignedCommit { sha } => format!("{}: commit is not signed", short(sha)),
            Violation::CreateWithoutFile { sha } => {
                format!("{}: `create:` added no markdown file", short(sha))
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
            Violation::AbsoluteSelfLink { path, url } => {
                format!("{path}: absolute self-link {url:?}; use the relative /id/N/ form")
            }
            Violation::RedirectCollision { slug, historical, live } => format!(
                "redirect for old slug {slug:?} (id {historical}) dropped; now owned by id {live}"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Redirect {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct CheckOptions {
    pub verify_signatures: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self { verify_signatures: true }
    }
}

pub fn build_model(commits: &[RawCommit], root: &str) -> Model {
    fold(commits, root).0
}

/// Structural checks (grammar, signatures, lineage, slug collisions).
pub fn check(commits: &[RawCommit], root: &str, opts: &CheckOptions) -> Vec<Violation> {
    let mut violations = fold(commits, root).1;
    if !opts.verify_signatures {
        violations.retain(|v| {
            !matches!(v, Violation::BadSignature { .. } | Violation::UnsignedCommit { .. })
        });
    }
    violations
}

/// Lint tracked files under the article root: everything there should be
/// markdown.
pub fn check_tracked_files(root: &str, files: &[String]) -> Vec<Violation> {
    let prefix = root_prefix(root);
    files
        .iter()
        .filter(|f| f.starts_with(&prefix) && !f.ends_with(".md"))
        .map(|f| Violation::NonMarkdownInRoot { path: f.clone() })
        .collect()
}

/// Content checks for one article file: missing title, dangling id links, and
/// absolute self-links that should use the relative `/id/N/` form.
pub fn check_content(model: &Model, base_url: &str, path: &str, content: &str) -> Vec<Violation> {
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

    let origin = base_url.trim_end_matches('/');
    if !origin.is_empty() {
        for dest in crate::markdown::link_dests(content) {
            if dest.starts_with(origin) {
                violations.push(Violation::AbsoluteSelfLink { path: path.to_string(), url: dest });
            }
        }
    }

    violations
}

/// Redirects (id permalinks + historical slugs). A historical slug now owned by
/// a different live article yields to it, reported as a `RedirectCollision`.
pub fn plan_redirects(model: &Model) -> (Vec<Redirect>, Vec<Violation>) {
    let live: HashMap<&str, Id> =
        model.articles.iter().map(|a| (a.slug.as_str(), a.id)).collect();

    let mut redirects = Vec::new();
    let mut violations = Vec::new();

    for a in &model.articles {
        let to = format!("/{}/", a.slug);
        redirects.push(Redirect { from: format!("/id/{}/", a.id), to: to.clone() });

        for past in &a.past_slugs {
            if past == &a.slug {
                continue;
            }
            match live.get(past.as_str()) {
                Some(&owner) if owner != a.id => violations.push(Violation::RedirectCollision {
                    slug: past.clone(),
                    historical: a.id,
                    live: owner,
                }),
                _ => redirects.push(Redirect { from: format!("/{past}/"), to: to.clone() }),
            }
        }
    }

    (redirects, violations)
}

fn fold(commits: &[RawCommit], root: &str) -> (Model, Vec<Violation>) {
    let mut model = Model::default();
    let mut violations = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut next_id: Id = 1;

    for c in commits {
        match c.sig {
            SigStatus::Bad => violations.push(Violation::BadSignature { sha: c.sha.clone() }),
            SigStatus::None => violations.push(Violation::UnsignedCommit { sha: c.sha.clone() }),
            SigStatus::Good => {}
        }

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
                let path = match c.changed.iter().find_map(added_md) {
                    Some(p) => p,
                    None => {
                        violations.push(Violation::CreateWithoutFile { sha: c.sha.clone() });
                        continue;
                    }
                };
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
                    history: vec![c.sha.clone()],
                    past_slugs: Vec::new(),
                    frontmatter: Frontmatter::default(),
                    title: None,
                });
                by_path.insert(path, idx);
                next_id += 1;
            }

            CommitKind::Update => {
                for f in &c.changed {
                    let Some(p) = touched_md(f) else { continue };
                    match by_path.get(&p) {
                        Some(&idx) => {
                            model.articles[idx].history.push(c.sha.clone());
                            model.articles[idx].updated = c.date.clone();
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
                    {
                        let art = &mut model.articles[idx];
                        if art.slug != new_slug {
                            art.past_slugs.push(art.slug.clone());
                        }
                        art.slug = new_slug.clone();
                        art.path = to.clone();
                        art.history.push(c.sha.clone());
                        art.updated = c.date.clone();
                    }
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

/// `foo/` for a root of `foo`; empty for `.` (repo root).
fn root_prefix(root: &str) -> String {
    let root = root.trim_end_matches('/');
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
        FileChange::Added(p) | FileChange::Modified(p) if is_md(p) => Some(p.clone()),
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
    use crate::git::{FileChange, RawCommit, SigStatus};

    fn c(subject: &str, changed: Vec<FileChange>) -> RawCommit {
        RawCommit {
            sha: "0".repeat(40),
            subject: subject.into(),
            body: String::new(),
            date: "2026-01-01T00:00:00Z".into(),
            sig: SigStatus::Good,
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
        let v = check(&h, "src", &CheckOptions::default());
        assert!(matches!(v.as_slice(), [Violation::UpdateBeforeCreate { .. }]));
    }

    #[test]
    fn disabling_signatures_drops_signature_violations() {
        let mut unsigned = c("create: x", vec![added("src/x.md")]);
        unsigned.sig = SigStatus::None;
        let opts = CheckOptions { verify_signatures: false };
        assert!(check(&[unsigned], "src", &opts).is_empty());
    }

    #[test]
    fn meta_touching_an_article_flagged() {
        let h = [
            c("create: willow", vec![added("src/willow.md")]),
            c("meta: tweak", vec![modified("src/willow.md")]),
        ];
        let v = check(&h, "src", &CheckOptions::default());
        assert!(v.iter().any(|x| matches!(x, Violation::MetaTouchesArticle { .. })));
    }

    #[test]
    fn meta_touching_non_article_ok() {
        let h = [
            c("create: willow", vec![added("src/willow.md")]),
            c("meta: docs", vec![modified("README.md")]),
        ];
        let v = check(&h, "src", &CheckOptions::default());
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
        assert!(check(&h, "src", &CheckOptions::default()).is_empty());
    }

    #[test]
    fn live_slug_collision_flagged() {
        // `src/dup.md` and `src/dup/index.md` both route to `dup`.
        let h = [
            c("create: a", vec![added("src/dup.md")]),
            c("create: b", vec![added("src/dup/index.md")]),
        ];
        let v = check(&h, "src", &CheckOptions::default());
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
    fn reused_slug_yields_redirect_collision() {
        let h = [
            c("create: direction", vec![added("src/direction.md")]),
            c("move: rename", vec![renamed("src/direction.md", "src/choosing-direction.md")]),
            c("create: new", vec![added("src/direction.md")]),
        ];
        let m = build_model(&h, "src");
        let (reds, viol) = plan_redirects(&m);
        assert!(viol.iter().any(|v| matches!(v, Violation::RedirectCollision { .. })));
        assert!(reds.iter().any(|r| r.from == "/id/1/"));
        assert!(!reds.iter().any(|r| r.from == "/direction/"));
    }

    #[test]
    fn content_check_finds_missing_title_and_dangling_link() {
        let m = build_model(&[c("create: willow", vec![added("src/willow.md")])], "src");
        let v = check_content(&m, "https://x.dev", "src/willow.md", "no heading [x](id:99)");
        assert!(v.iter().any(|x| matches!(x, Violation::MissingTitle { .. })));
        assert!(v.iter().any(|x| matches!(x, Violation::DanglingIdLink { id: 99, .. })));
    }

    #[test]
    fn absolute_self_link_flagged_but_not_foreign() {
        let m = build_model(&[c("create: willow", vec![added("src/willow.md")])], "src");
        let content = "# t\n[self](https://site.dev/id/1) [ext](https://other.site/id/1)";
        let v = check_content(&m, "https://site.dev", "src/willow.md", content);
        let flagged: Vec<_> =
            v.iter().filter(|x| matches!(x, Violation::AbsoluteSelfLink { .. })).collect();
        assert_eq!(flagged.len(), 1);
    }

    #[test]
    fn render_hydrates_id_link() {
        let m = build_model(&[c("create: willow", vec![added("src/willow.md")])], "src");
        let html = crate::markdown::render("[w](id:1)", &m);
        assert!(html.contains("href=\"/willow/\""));
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
        let v = check(&h, "src", &CheckOptions::default());
        assert!(v.iter().any(|x| matches!(x, Violation::MoveTouchesMultipleArticles { .. })));
    }

    #[test]
    fn move_of_single_nested_article_ok() {
        let h = [
            c("create: idx", vec![added("src/dir/index.md")]),
            c("create: child", vec![added("src/dir/child.md")]),
            c("move: rename one", vec![renamed("src/dir/child.md", "src/dir/renamed.md")]),
        ];
        let v = check(&h, "src", &CheckOptions::default());
        assert!(!v.iter().any(|x| matches!(x, Violation::MoveTouchesMultipleArticles { .. })));
    }

    #[test]
    fn non_markdown_under_root_flagged() {
        let files = vec![
            "src/willow.md".to_string(),
            "src/notes.txt".to_string(),
            "README.md".to_string(),      // outside root
            "assets/logo.png".to_string(), // outside root
        ];
        let v = check_tracked_files("src", &files);
        assert!(matches!(v.as_slice(), [Violation::NonMarkdownInRoot { path }] if path == "src/notes.txt"));
    }
}
