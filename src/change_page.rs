//! Generated per-commit page: what one change did, and its git diff.

use crate::commits::Subject;
use crate::html::{escape, slug_change};
use crate::model::{ChangeKind, Model};
use crate::routes;

/// Description, optional commit body, the article(s) the commit touched, and
/// the colorized diff.
pub fn body(model: &Model, subject: &Subject, commit_body: &str, sha: &str, diff: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<h1>{}: {}</h1>\n",
        subject.kind.label(),
        escape(&subject.description)
    ));

    if !commit_body.trim().is_empty() {
        out.push_str(&format!("<p>{}</p>\n", escape(commit_body.trim())));
    }

    for c in model.changes_in(sha) {
        let Some(a) = model.by_id(c.article) else { continue };
        let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
        let moved = match &c.kind {
            ChangeKind::Move { from, to } => slug_change(from, to),
            _ => String::new(),
        };
        out.push_str(&format!(
            "<p>article: <a href=\"{}\">{}</a>{} · <a href=\"{}\">changes</a></p>\n",
            routes::article(&a.slug),
            escape(&title),
            moved,
            routes::changes_for(a.id),
        ));
    }

    out.push_str("<pre class=\"diff\">");
    for line in diff.lines() {
        out.push_str(&format!("<span class=\"{}\">{}</span>\n", diff_class(line), escape(line)));
    }
    out.push_str("</pre>\n");
    out
}

fn diff_class(line: &str) -> &'static str {
    if line.starts_with("@@") {
        "hunk"
    } else if line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("+++")
        || line.starts_with("---")
    {
        "file"
    } else if line.starts_with('+') {
        "add"
    } else if line.starts_with('-') {
        "del"
    } else {
        "ctx"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commits::parse_subject;
    use crate::git::{FileChange, RawCommit};
    use crate::model::build_model;

    #[test]
    fn renders_description_touched_article_and_classified_diff() {
        let commits = [RawCommit {
            sha: "s1".into(),
            subject: "create: willow".into(),
            body: String::new(),
            date: "2026-01-01T00:00:00Z".into(),
            changed: vec![FileChange::Added("src/willow.md".into())],
        }];
        let model = build_model(&commits, "src");
        let subject = parse_subject("update: typo fix").unwrap();

        let diff = "@@ -1 +1 @@\n-old\n+new\n context";
        let html = body(&model, &subject, "more detail", "s1", diff);

        assert!(html.contains("<h1>update: typo fix</h1>"));
        assert!(html.contains("more detail"));
        assert!(html.contains("href=\"/article/willow/\""));
        assert!(html.contains("href=\"/changes/by-id/1/\""));
        assert!(html.contains("class=\"hunk\">@@ -1 +1 @@"));
        assert!(html.contains("class=\"del\">-old"));
        assert!(html.contains("class=\"add\">+new"));
    }
}
