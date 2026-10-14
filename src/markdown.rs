//! Markdown rendering and `id:` link hydration via pulldown-cmark.

use crate::frontmatter;
use crate::model::{Id, Model};
use crate::routes;
use pulldown_cmark::{html, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::sync::OnceLock;

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
}

/// Render markdown to HTML, rewriting `id:` link/image targets to slug URLs
/// and wrapping `copy`-flagged code blocks for the copy button.
/// Any frontmatter block is metadata, not content, so it is not rendered.
pub fn render(md: &str, model: &Model) -> String {
    let mut events = Vec::new();
    let mut in_copyable = false;
    for ev in parser(md) {
        match ev {
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                let dest_url = rewrite(dest_url, model);
                events.push(Event::Start(Tag::Link { link_type, dest_url, title, id }));
            }
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
                let dest_url = rewrite(dest_url, model);
                events.push(Event::Start(Tag::Image { link_type, dest_url, title, id }));
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let info = match take_copy_flag(&info) {
                    Some(rest) => {
                        events.push(Event::Html("<div class=\"copyable\">".into()));
                        in_copyable = true;
                        CowStr::from(rest)
                    }
                    None => info,
                };
                events.push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))));
            }
            Event::End(TagEnd::CodeBlock) if in_copyable => {
                events.push(Event::End(TagEnd::CodeBlock));
                events.push(Event::Html("</div>\n".into()));
                in_copyable = false;
            }
            other => events.push(other),
        }
    }

    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

/// The fence's info string with its `copy` flag removed, if it had one. The
/// language stays first, where the renderer looks for it.
fn take_copy_flag(info: &str) -> Option<String> {
    let words: Vec<&str> = info.split_whitespace().collect();
    if !words.contains(&"copy") {
        return None;
    }
    Some(words.into_iter().filter(|w| *w != "copy").collect::<Vec<_>>().join(" "))
}

/// Parser over the markdown body, with any frontmatter block stripped.
fn parser(md: &str) -> Parser<'_> {
    Parser::new_ext(frontmatter::split(md).1, options())
}

fn rewrite<'a>(dest: CowStr<'a>, model: &Model) -> CowStr<'a> {
    match parse_id_ref(&dest).and_then(|id| model.by_id(id)) {
        Some(article) => CowStr::from(routes::article(&article.slug)),
        None => dest,
    }
}

/// The article's title: the frontmatter `title` if given, else the first
/// level-1 heading's text.
pub fn extract_title(md: &str) -> Option<String> {
    let (fields, body) = frontmatter::split(md);
    fields.title.or_else(|| heading_title(body))
}

/// Whether the body carries a level-1 heading of its own.
pub fn has_heading(md: &str) -> bool {
    heading_title(frontmatter::split(md).1).is_some()
}

/// The first level-1 heading's text.
fn heading_title(md: &str) -> Option<String> {
    let mut in_h1 = false;
    let mut title = String::new();
    for ev in Parser::new_ext(md, options()) {
        match ev {
            Event::Start(Tag::Heading { level: HeadingLevel::H1, .. }) => in_h1 = true,
            Event::End(TagEnd::Heading(HeadingLevel::H1)) => {
                let t = title.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
                in_h1 = false;
            }
            Event::Text(t) | Event::Code(t) if in_h1 => title.push_str(&t),
            _ => {}
        }
    }
    None
}

/// Every article id referenced by a link/image in the document.
pub fn id_refs(md: &str) -> Vec<Id> {
    let mut ids = Vec::new();
    for ev in parser(md) {
        if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = ev {
            if let Some(id) = parse_id_ref(&dest_url) {
                ids.push(id);
            }
        }
    }
    ids
}

/// Extract an article id from an internal ref: `id:2`, `/id/2`, or `/id/2/`.
/// Absolute/foreign URLs (e.g. `https://other.site/id/2`) are left untouched.
pub fn parse_id_ref(dest: &str) -> Option<Id> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(?:id:|/id/)(\d+)/?$").unwrap());
    re.captures(dest)?.get(1)?.as_str().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_internal_id_refs_only() {
        assert_eq!(parse_id_ref("id:2"), Some(2));
        assert_eq!(parse_id_ref("/id/2"), Some(2));
        assert_eq!(parse_id_ref("/id/2/"), Some(2));
        assert_eq!(parse_id_ref("https://other.site/id/2"), None);
        assert_eq!(parse_id_ref("/other/"), None);
    }

    #[test]
    fn title_is_first_h1() {
        assert_eq!(extract_title("# hello\ntext").as_deref(), Some("hello"));
        assert_eq!(extract_title("no heading"), None);
    }

    #[test]
    fn frontmatter_title_wins_over_heading() {
        let md = "---\ntitle: real title\n---\n# heading\n";
        assert_eq!(extract_title(md).as_deref(), Some("real title"));
        assert!(has_heading(md), "the body still carries its own heading");
    }

    #[test]
    fn frontmatter_title_supplants_missing_heading() {
        let md = "---\ntitle: only title\n---\njust prose\n";
        assert_eq!(extract_title(md).as_deref(), Some("only title"));
        assert!(!has_heading(md));
    }

    #[test]
    fn frontmatter_is_metadata_not_content() {
        let m = Model::default();
        let html = render("---\ntitle: t\n---\nprose\n", &m);
        assert!(!html.contains("title: t"), "{html}");
        assert!(html.contains("prose"));
    }

    #[test]
    fn copy_flag_wraps_the_block_and_leaves_the_language() {
        let m = Model::default();
        let html = render("```sh copy\nls\n```\n", &m);
        assert_eq!(
            html,
            "<div class=\"copyable\">\n<pre><code class=\"language-sh\">ls\n</code></pre>\n</div>\n"
        );
        let bare = render("```copy\nls\n```\n", &m);
        assert!(bare.contains("<pre><code>ls"), "{bare}");
    }

    #[test]
    fn unflagged_blocks_are_not_copyable() {
        let html = render("```sh\nls\n```\n", &Model::default());
        assert!(!html.contains("copyable"), "{html}");
    }
}
