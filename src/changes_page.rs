//! Generated per-article change list: every commit that touched one article.

use crate::html::{date, escape, slug_change};
use crate::model::{Article, ChangeKind, Model};
use crate::routes;

pub fn body(model: &Model, a: &Article) -> String {
    let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
    let mut out = format!(
        "<h1>changes: {}</h1>\n\
         <p><a href=\"{}\">the article</a> \
         <small>id {} · created {} · updated {}</small></p>\n<ul>\n",
        escape(&title),
        routes::article(&a.slug),
        a.id,
        date(&a.created),
        date(&a.updated),
    );

    let mut changes: Vec<_> = model.changes_for(a.id).collect();
    changes.reverse();
    for c in changes {
        let moved = match &c.kind {
            ChangeKind::Move { from, to } => slug_change(from, to),
            _ => String::new(),
        };
        out.push_str(&format!(
            "<li>{} <b>{}</b> {}{} <a href=\"{}\">commit</a></li>\n",
            date(&c.date),
            c.kind.label(),
            escape(&c.description),
            moved,
            routes::change(&c.sha),
        ));
    }

    out.push_str("</ul>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, RawCommit};
    use crate::model::build_model;

    fn commit(sha: &str, subject: &str, changed: Vec<FileChange>) -> RawCommit {
        RawCommit {
            sha: sha.into(),
            subject: subject.into(),
            body: String::new(),
            date: "2026-01-01T00:00:00Z".into(),
            changed,
        }
    }

    #[test]
    fn lists_one_articles_changes_newest_first() {
        let commits = [
            commit("s1", "create: direction", vec![FileChange::Added("src/direction.md".into())]),
            commit("s2", "move: rename", vec![FileChange::Renamed {
                from: "src/direction.md".into(),
                to: "src/choosing-direction.md".into(),
            }]),
            // A second article's history must not leak into the first's page.
            commit("s3", "create: willow", vec![FileChange::Added("src/willow.md".into())]),
        ];
        let model = build_model(&commits, "src");
        let html = body(&model, model.by_id(1).unwrap());

        assert!(html.find("<b>move</b>").unwrap() < html.find("<b>create</b>").unwrap());
        assert!(html.contains("<code>direction</code> → <code>choosing-direction</code>"));
        assert!(!html.contains("willow"));
        assert_eq!(html.matches("<li>").count(), 2);
    }
}
