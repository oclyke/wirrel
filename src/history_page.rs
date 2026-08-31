//! Generated update-history page: the most recent change events.

use crate::commits::{parse_subject, CommitKind};
use crate::git::RawCommit;
use crate::html::{date, escape, url};
use crate::model::{Article, Model};
use std::collections::HashMap;

/// The `<ul>` of the most recent change events, newest first. Shared by the
/// full history page and the index's "recent changes" section.
pub fn events(commits: &[RawCommit], model: &Model, limit: usize) -> String {
    let mut by_sha: HashMap<&str, &Article> = HashMap::new();
    for a in &model.articles {
        for sha in &a.history {
            by_sha.insert(sha.as_str(), a);
        }
    }

    let mut out = String::from("<ul>\n");
    let mut shown = 0;
    for c in commits.iter().rev() {
        if shown >= limit {
            break;
        }
        let Ok(subject) = parse_subject(&c.subject) else { continue };
        let kind = match subject.kind {
            CommitKind::Create => "create",
            CommitKind::Update => "update",
            CommitKind::Move => "move",
            CommitKind::Meta => continue,
        };
        let what = match by_sha.get(c.sha.as_str()) {
            Some(a) => {
                let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
                format!("<a href=\"{}\">{}</a>", url(&a.slug), escape(&title))
            }
            None => escape(&subject.description),
        };
        out.push_str(&format!("<li>{} <b>{}</b> {}</li>\n", date(&c.date), kind, what));
        shown += 1;
    }
    out.push_str("</ul>\n");
    out
}

pub fn body(commits: &[RawCommit], model: &Model, limit: usize) -> String {
    format!("<h1>update history</h1>\n{}", events(commits, model, limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, SigStatus};
    use crate::model::build_model;

    fn commit(sha: &str, subject: &str, date: &str, changed: Vec<FileChange>) -> RawCommit {
        RawCommit {
            sha: sha.into(),
            subject: subject.into(),
            body: String::new(),
            date: date.into(),
            sig: SigStatus::Good,
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
        let html = events(&commits, &model, 10);

        assert!(html.contains("/willow/"));
        // newest (update) appears before oldest (create)
        assert!(html.find("update").unwrap() < html.find("create").unwrap());
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
        assert_eq!(events(&commits, &model, 2).matches("<li>").count(), 2);
    }
}
