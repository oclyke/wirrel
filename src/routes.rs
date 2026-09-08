//! The site's address scheme: the one place URL literals live.
//!
//! Article slugs appear only at the tail of `/article/`, and nothing is
//! generated beneath them. Every other address is keyed by an id, a sha, or a
//! fixed word, so no note can claim a generated address — a `changelog.md`
//! routes to `/article/changelog/`, never `/changelog/`.

use crate::model::Id;
use std::path::{Path, PathBuf};

/// Where assets land in the built site. Which repo directory they come from is
/// a layout question, settled by `--assets`; this is the address they get, the
/// same way `--articles` never shows up in an article's URL.
pub const ASSETS_DIR: &str = "assets";

/// The site-wide stylesheet. Root-relative, because pages sit at every depth —
/// an article three levels down needs the same href as the index.
pub fn stylesheet() -> String {
    format!("/{ASSETS_DIR}/style.css")
}

pub fn index() -> String {
    "/".into()
}

pub fn articles_by_path() -> String {
    "/articles/by-path/".into()
}

pub fn articles_by_id() -> String {
    "/articles/by-id/".into()
}

/// The id permalink: a stub page, and a redirect for hosts that can serve one.
pub fn article_by_id(id: Id) -> String {
    format!("/articles/by-id/{id}/")
}

/// The only address holding a free-form slug. The root `index.md` has an empty
/// slug and owns the prefix itself.
pub fn article(slug: &str) -> String {
    if slug.is_empty() {
        "/article/".into()
    } else {
        format!("/article/{slug}/")
    }
}

pub fn changelog() -> String {
    "/changelog/".into()
}

pub fn changes_for(id: Id) -> String {
    format!("/changes/by-id/{id}/")
}

pub fn change(sha: &str) -> String {
    format!("/changes/by-sha/{sha}/")
}

/// The authoring form written in source markdown. Hydrated away at build; kept
/// in the redirect map so a link pasted out of a source file still resolves.
pub fn id_permalink(id: Id) -> String {
    format!("/id/{id}/")
}

/// The file a route's page is written to: `<out>/<route>/index.html`.
pub fn out_path(out: &Path, route: &str) -> PathBuf {
    out.join(route.trim_matches('/')).join("index.html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_never_reach_the_top_level() {
        // A note named for a generated address still lands under `/article/`.
        assert_eq!(article("changelog"), "/article/changelog/");
        assert_eq!(article("articles/by-id"), "/article/articles/by-id/");
        assert_eq!(article(""), "/article/");
    }

    #[test]
    fn the_stylesheet_is_root_relative() {
        assert!(stylesheet().starts_with('/'));
    }

    #[test]
    fn routes_map_to_index_files() {
        let out = Path::new("dist");
        assert_eq!(out_path(out, &index()), Path::new("dist/index.html"));
        assert_eq!(out_path(out, &article("a/b")), Path::new("dist/article/a/b/index.html"));
        assert_eq!(out_path(out, &changes_for(7)), Path::new("dist/changes/by-id/7/index.html"));
    }
}
