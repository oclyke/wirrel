//! Generated changelog: every create, update and move, newest first.

use crate::html::{date, escape, slug_change};
use crate::model::{Change, ChangeKind, Model};
use crate::routes;

/// The `<ul>` of change events, newest first. Shared by the changelog and the
/// index's "recent changes" section, which passes a limit.
pub fn events(model: &Model, limit: Option<usize>) -> String {
    let mut out = String::from("<ul>\n");
    for c in model.changes.iter().rev().take(limit.unwrap_or(usize::MAX)) {
        out.push_str(&format!("<li>{}</li>\n", row(model, c)));
    }
    out.push_str("</ul>\n");
    out
}

fn row(model: &Model, c: &Change) -> String {
    let what = match model.by_id(c.article) {
        Some(a) => {
            let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
            format!("<a href=\"{}\">{}</a>", routes::article(&a.slug), escape(&title))
        }
        None => escape(&c.description),
    };
    let moved = match &c.kind {
        ChangeKind::Move { from, to } => slug_change(from, to),
        _ => String::new(),
    };
    format!(
        "{} <b>{}</b> {}{} <a href=\"{}\">commit</a>",
        date(&c.date),
        c.kind.label(),
        what,
        moved,
        routes::change(&c.sha),
    )
}

pub fn body(model: &Model) -> String {
    format!("<h1>changelog</h1>\n{}", events(model, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, RawCommit};
    use crate::model::build_model;

    fn commit(sha: &str, subject: &str, date: &str, changed: Vec<FileChange>) -> RawCommit {
        RawCommit {
            sha: sha.into(),
            subject: subject.into(),
            body: String::new(),
            date: date.into(),
            changed,
        }
    }

    #[test]
    fn events_newest_first() {
        let commits = [
            commit("s1", "create: willow", "2026-01-01T00:00:00Z", vec![FileChange::Added("src/willow.md".into())]),
            commit("s2", "update: fix", "2026-01-03T00:00:00Z", vec![FileChange::Modified("src/willow.md".into())]),
        ];
        let model = build_model(&commits, "src");
        let html = events(&model, None);

        assert!(html.contains("/article/willow/"));
        assert!(html.find("update").unwrap() < html.find("create").unwrap());
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
