//! Small HTML helpers shared by the rendered pages.

use crate::model::Id;
use crate::routes;

/// wirrel's default stylesheet, written to the assets directory when the repo
/// supplies none of its own.
pub const DEFAULT_STYLESHEET: &str = include_str!("default.css");

/// The head every shell shares, stylesheet link included — that link is why a
/// page never has to know how deep it sits.
fn head_common() -> String {
    format!(
        "<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <link rel=\"stylesheet\" href=\"{}\">\n",
        routes::stylesheet()
    )
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The date portion of an RFC 3339 timestamp.
pub fn date(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}

/// A move's endpoints, as plain text. Deliberately never linked: a slug an
/// article has left is not an address — it may have changed hands, or nothing
/// may answer there at all. The changelog records history; it doesn't offer it
/// as navigation.
pub fn slug_change(from: &str, to: &str) -> String {
    if from == to {
        return String::new();
    }
    format!(" <code>{}</code> → <code>{}</code>", slug_text(from), slug_text(to))
}

fn slug_text(slug: &str) -> String {
    if slug.is_empty() {
        "(root)".into()
    } else {
        escape(slug)
    }
}

/// Per-article metadata header: ways back to the index and to the article's own
/// change history, plus creation and last-updated dates.
pub fn meta_header(id: Id, created: &str, updated: &str) -> String {
    format!(
        "<header><a href=\"{}\">index</a> · <a href=\"{}\">changes</a> \
         <small>created {} · updated {}</small></header>\n",
        routes::index(),
        routes::changes_for(id),
        date(created),
        date(updated)
    )
}

/// Minimal HTML shell with a self-referential canonical tag.
pub fn page(title: &str, canonical: &str, body: &str) -> String {
    let title = escape(title);
    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         {head}\
         <title>{title}</title>\n\
         <link rel=\"canonical\" href=\"{canonical}\">\n\
         </head>\n\
         <body>\n{body}</body>\n\
         </html>\n",
        head = head_common()
    )
}

/// Shell for a page that shouldn't be indexed and has no canonical address of
/// its own: the dead ends.
pub fn dead_end_page(title: &str, body: &str) -> String {
    let title = escape(title);
    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         {head}\
         <meta name=\"robots\" content=\"noindex\">\n\
         <title>{title}</title>\n\
         </head>\n\
         <body>\n{body}</body>\n\
         </html>\n",
        head = head_common()
    )
}

/// A page whose only job is to name another address: canonical there, a refresh
/// for browsers, and a visible link for everything else. Lets the id permalink
/// resolve on a host that can't be told about redirects.
pub fn stub_page(title: &str, target: &str) -> String {
    let title = escape(title);
    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         {head}\
         <meta http-equiv=\"refresh\" content=\"0; url={target}\">\n\
         <title>{title}</title>\n\
         <link rel=\"canonical\" href=\"{target}\">\n\
         </head>\n\
         <body>\n<p><a href=\"{target}\">{target}</a></p>\n</body>\n\
         </html>\n",
        head = head_common()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_links_the_stylesheet() {
        let link = "<link rel=\"stylesheet\" href=\"/assets/style.css\">";
        assert!(page("t", "/c/", "b").contains(link));
        assert!(dead_end_page("t", "b").contains(link));
        assert!(stub_page("t", "/x/").contains(link));
    }

    #[test]
    fn stub_points_at_its_target() {
        let html = stub_page("id 3", "/article/willow/");
        assert!(html.contains("url=/article/willow/"));
        assert!(html.contains("rel=\"canonical\" href=\"/article/willow/\""));
        assert!(html.contains("<a href=\"/article/willow/\">"));
    }

    #[test]
    fn header_links_index_and_changes_and_shows_dates() {
        let h = meta_header(3, "2026-01-02T00:00:00Z", "2026-03-04T12:00:00Z");
        assert!(h.contains("href=\"/\">index</a>"));
        assert!(h.contains("href=\"/changes/by-id/3/\">changes</a>"));
        assert!(h.contains("created 2026-01-02"));
        assert!(h.contains("updated 2026-03-04"));
    }
}
