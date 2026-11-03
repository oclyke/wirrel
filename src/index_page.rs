//! The front door: the way in to everything else, the latest few changes, and
//! the article tree itself.
//!
//! The id listing and the full changelog own their own addresses, so the index
//! only links them; the tree has no other home.

use crate::model::Model;
use crate::routes;
use crate::{articles_page, changelog_page};

pub fn body(model: &Model, recent: usize) -> String {
    format!(
        "<h1>index</h1>\n\
         <ul>\n\
         <li><a href=\"{}\">articles by id</a></li>\n\
         <li><a href=\"{}\">changelog</a></li>\n\
         </ul>\n\
         <h2>recent changes</h2>\n{}\
         <h2>articles by path</h2>\n{}",
        routes::articles_by_id(),
        routes::changelog(),
        changelog_page::events(model, Some(recent)),
        articles_page::tree(model),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, RawCommit};
    use crate::model::build_model;

    #[test]
    fn links_the_listings_and_shows_recent_changes_and_the_tree() {
        let commits = [RawCommit {
            sha: "s1".into(),
            subject: "create: willow".into(),
            body: String::new(),
            date: "2026-01-02T00:00:00Z".into(),
            changed: vec![FileChange::Added("src/willow.md".into())],
            stats: Vec::new(),
        }];
        let mut model = build_model(&commits, "src");
        model.articles[0].frontmatter.title = Some("The Willow".into());

        let html = body(&model, 5);
        assert!(html.contains("href=\"/articles/by-id/\""));
        assert!(html.contains("href=\"/changelog/\""));
        assert!(html.contains("recent changes"));
        assert!(html.contains("The Willow"));
        // The tree sits below the stream and names files, not titles.
        assert!(html.find("<ul class=\"stream\">").unwrap() < html.find("<ul class=\"tree\">").unwrap());
        assert!(!html.contains("src/"));
        assert!(html.contains(">willow.md</a>"));
    }
}
