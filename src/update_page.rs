//! Generated per-update page: an update commit's description and its git diff.

use crate::html::{escape, url};
use crate::model::Article;

const STYLE: &str = "<style>\
.diff{background:#f6f8fa;padding:1em;overflow:auto}\
.diff .add{color:#116329}.diff .del{color:#a40e26}\
.diff .hunk{color:#795da3}.diff .file{color:#57606a;font-weight:bold}\
</style>\n";

/// Render the page body: description, optional commit body, links to the touched
/// article(s), and the colorized diff.
pub fn body(description: &str, commit_body: &str, touched: &[&Article], diff: &str) -> String {
    let mut out = String::new();
    out.push_str(STYLE);
    out.push_str(&format!("<h1>update: {}</h1>\n", escape(description)));

    if !commit_body.trim().is_empty() {
        out.push_str(&format!("<p>{}</p>\n", escape(commit_body.trim())));
    }

    for a in touched {
        let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
        out.push_str(&format!("<p>article: <a href=\"{}\">{}</a></p>\n", url(&a.slug), escape(&title)));
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
    use crate::git::{FileChange, RawCommit, SigStatus};
    use crate::model::build_model;

    #[test]
    fn renders_description_and_classified_diff() {
        let commits = [RawCommit {
            sha: "s1".into(),
            subject: "create: willow".into(),
            body: String::new(),
            date: "2026-01-01T00:00:00Z".into(),
            sig: SigStatus::Good,
            changed: vec![FileChange::Added("src/willow.md".into())],
        }];
        let model = build_model(&commits, "src");
        let touched: Vec<&Article> = model.articles.iter().collect();

        let diff = "@@ -1 +1 @@\n-old\n+new\n context";
        let html = body("typo fix", "more detail", &touched, diff);

        assert!(html.contains("<h1>update: typo fix</h1>"));
        assert!(html.contains("more detail"));
        assert!(html.contains("href=\"/willow/\""));
        assert!(html.contains("class=\"hunk\">@@ -1 +1 @@"));
        assert!(html.contains("class=\"del\">-old"));
        assert!(html.contains("class=\"add\">+new"));
    }
}
