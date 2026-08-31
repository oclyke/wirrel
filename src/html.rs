//! Small HTML helpers shared by the rendered pages.

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Root-relative URL for a route slug (empty slug -> `/`).
pub fn url(slug: &str) -> String {
    if slug.is_empty() {
        "/".into()
    } else {
        format!("/{slug}/")
    }
}

/// The date portion of an RFC 3339 timestamp.
pub fn date(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}

/// Per-article metadata header showing creation and last-updated dates.
pub fn meta_header(created: &str, updated: &str) -> String {
    format!(
        "<header><small>created {} · updated {}</small></header>\n",
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
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n\
         <link rel=\"canonical\" href=\"{canonical}\">\n\
         </head>\n\
         <body>\n{body}</body>\n\
         </html>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_shows_dates() {
        let h = meta_header("2026-01-02T00:00:00Z", "2026-03-04T12:00:00Z");
        assert!(h.contains("created 2026-01-02"));
        assert!(h.contains("updated 2026-03-04"));
    }
}
