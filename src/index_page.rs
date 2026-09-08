//! The front door: the way in to everything else, and the latest few changes.
//!
//! Deliberately thin — the exhaustive article listings own their own addresses,
//! so nothing here duplicates them.

use crate::changelog_page;
use crate::model::Model;
use crate::routes;

pub fn body(model: &Model, recent: usize) -> String {
    format!(
        "<h1>index</h1>\n\
         <ul>\n\
         <li><a href=\"{}\">articles by path</a></li>\n\
         <li><a href=\"{}\">articles by id</a></li>\n\
         <li><a href=\"{}\">changelog</a></li>\n\
         </ul>\n\
         <h2>recent changes</h2>\n{}",
        routes::articles_by_path(),
        routes::articles_by_id(),
        routes::changelog(),
        changelog_page::events(model, Some(recent)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, RawCommit, SigStatus};
    use crate::model::build_model;

    #[test]
    fn links_the_listings_and_shows_recent_changes() {
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

        let html = body(&model, 5);
        assert!(html.contains("href=\"/articles/by-path/\""));
        assert!(html.contains("href=\"/articles/by-id/\""));
        assert!(html.contains("href=\"/changelog/\""));
        assert!(html.contains("recent changes"));
        assert!(html.contains("The Willow"));
    }
}
