//! Generated changelog: every create, update and move, newest first.

use crate::html::{date, escape, slug_change};
use crate::model::{Change, ChangeKind, Model};
use crate::routes;

/// The `<ul>` of change events, newest first. Shared by the changelog and the
/// index's "recent changes" section, which passes a limit.
pub fn events(model: &Model, limit: Option<usize>) -> String {
    let mut out = String::from("<ul class=\"stream\">\n");
    for c in model.changes.iter().rev().take(limit.unwrap_or(usize::MAX)) {
        out.push_str(&format!("<li>{}</li>\n", row(model, c)));
    }
    out.push_str("</ul>\n");
    out
}

fn row(model: &Model, c: &Change) -> String {
    let what = match model.by_id(c.article) {
        Some(a) => {
            let title = a.title();
            format!("<a href=\"{}\">{}</a>", routes::article(&a.slug), escape(&title))
        }
        None => escape(&c.description),
    };
    let moved = match &c.kind {
        ChangeKind::Move { from, to } => slug_change(from, to),
        _ => String::new(),
    };
    format!(
        "<span class=\"date\">{}</span> <b class=\"kind\">{}</b> {}{} {} \
         <a class=\"sha\" href=\"{}\">{}</a>",
        date(&c.date),
        c.kind.label(),
        what,
        moved,
        diffstat(c.added, c.deleted),
        routes::change(&c.sha),
        short(&c.sha),
    )
}

/// `+added −deleted`, each coloured by `.add` / `.del` like the diff pages.
fn diffstat(added: u32, deleted: u32) -> String {
    format!(
        "<span class=\"stat\"><span class=\"add\">+{added}</span> <span class=\"del\">−{deleted}</span></span>"
    )
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

pub fn body(model: &Model) -> String {
    format!("<h1>changelog</h1>\n{}", events(model, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, FileStat, RawCommit};
    use crate::model::build_model;

    fn commit(sha: &str, subject: &str, date: &str, changed: Vec<FileChange>) -> RawCommit {
        RawCommit {
            sha: sha.into(),
            subject: subject.into(),
            body: String::new(),
            date: date.into(),
            changed,
            stats: Vec::new(),
        }
    }

    #[test]
    fn events_newest_first_with_diffstat_and_short_sha() {
        let mut commits = [
            commit("s1", "create: willow", "2026-01-01T00:00:00Z", vec![FileChange::Added("src/willow.md".into())]),
            commit("0123456789abcdef", "update: fix", "2026-01-03T00:00:00Z", vec![FileChange::Modified("src/willow.md".into())]),
        ];
        commits[1].stats.push(FileStat { path: "src/willow.md".into(), added: 4, deleted: 2 });
        let model = build_model(&commits, "src");
        let html = events(&model, None);

        assert!(html.contains("/article/willow/"));
        assert!(html.find("update").unwrap() < html.find("create").unwrap());
        assert!(html.contains("<span class=\"add\">+4</span> <span class=\"del\">−2</span>"));
        assert!(html.contains("href=\"/changes/by-sha/0123456789abcdef/\">0123456</a>"));
    }

    #[test]
    fn move_names_both_slugs_without_linking_them() {
        let commits = [
            commit("s1", "create: direction", "2026-01-01T00:00:00Z", vec![FileChange::Added("src/direction.md".into())]),
            commit("s2", "move: rename", "2026-01-02T00:00:00Z", vec![FileChange::Renamed {
                from: "src/direction.md".into(),
                to: "src/choosing-direction.md".into(),
            }]),
        ];
        let model = build_model(&commits, "src");
        let html = events(&model, None);

        assert!(html.contains("<code>direction</code> → <code>choosing-direction</code>"));
        // A slug an article has left is not an address; it never becomes a link.
        assert!(!html.contains("href=\"/article/direction/\""));
    }

    #[test]
    fn events_honor_limit() {
        let commits: Vec<_> = (0..5)
            .map(|i| {
                let p = format!("src/a{i}.md");
                commit(&format!("s{i}"), &format!("create: a{i}"), "2026-01-01T00:00:00Z", vec![FileChange::Added(p)])
            })
            .collect();
        let model = build_model(&commits, "src");
        assert_eq!(events(&model, Some(2)).matches("<li>").count(), 2);
    }
}
