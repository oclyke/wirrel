//! Generated article index, plus the latest few site changes.

use crate::git::RawCommit;
use crate::history_page;
use crate::html::{date, escape, url};
use crate::model::Model;

pub fn body(commits: &[RawCommit], model: &Model, recent: usize) -> String {
    let mut articles: Vec<_> = model.articles.iter().collect();
    articles.sort_by_key(|a| a.id);

    let mut out = String::from("<h1>index</h1>\n<ul>\n");
    for a in articles {
        let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
        out.push_str(&format!(
            "<li><a href=\"{}\">{}</a> <small>{}</small></li>\n",
            url(&a.slug),
            escape(&title),
            date(&a.updated),
        ));
    }
    out.push_str("</ul>\n");

    out.push_str("<h2>recent changes</h2>\n");
    out.push_str(&history_page::events(commits, model, recent));
    out.push_str("<p><a href=\"/history/\">full history</a></p>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, SigStatus};
    use crate::model::build_model;

    #[test]
    fn lists_articles_and_links_history() {
        let commits = [RawCommit {
            sha: "s1".into(),
            subject: "create: willow".into(),
            body: String::new(),
            date: "2026-01-02T00:00:00Z".into(),
            sig: SigStatus::Good,
            changed: vec![FileChange::Added("src/willow.md".into())],
        }];
        let mut model = build_model(&commits, "src");
        model.articles[0].title = Some("The Willow".into());

        let html = body(&commits, &model, 5);
        assert!(html.contains("href=\"/willow/\""));
        assert!(html.contains("The Willow"));
        assert!(html.contains("recent changes"));
        assert!(html.contains("href=\"/history/\""));
    }
}
