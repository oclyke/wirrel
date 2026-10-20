//! Markdown rendering and `id:` link hydration via pulldown-cmark.

use crate::frontmatter;
use crate::highlight::Grammars;
use crate::model::{Id, Model};
use crate::routes;
use pulldown_cmark::{html, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::sync::OnceLock;

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
}

/// Render markdown to HTML, rewriting `id:` link/image targets to slug URLs,
/// highlighting fenced code a grammar covers, and wrapping `copy`-flagged code
/// blocks for the copy button.
/// Any frontmatter block is metadata, not content, so it is not rendered.
pub fn render(md: &str, model: &Model, grammars: &Grammars) -> String {
    let mut events = Vec::new();
    let mut in_copyable = false;
    // A fenced block with a language: its language and the code gathered so far.
    let mut fence: Option<(String, String)> = None;
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
                fence = fence_language(&info).map(|lang| (lang.to_string(), String::new()));
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
            Event::Text(text) if fence.is_some() => {
                fence.as_mut().unwrap().1.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((lang, code)) = fence.take() {
                    events.push(match grammars.highlight(&lang, &code) {
                        Some(html) => Event::Html(html.into()),
                        None => Event::Text(code.into()),
                    });
                }
                events.push(Event::End(TagEnd::CodeBlock));
                if in_copyable {
                    events.push(Event::Html("</div>\n".into()));
                    in_copyable = false;
                }
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

fn fence_language(info: &str) -> Option<&str> {
    info.split_whitespace().find(|w| *w != "copy")
}

/// The language of every fenced code block that names one, deduplicated.
pub fn fence_languages(md: &str) -> Vec<String> {
    let mut langs = Vec::new();
    for ev in parser(md) {
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = ev {
            if let Some(lang) = fence_language(&info).filter(|l| !langs.iter().any(|x| x == l)) {
                langs.push(lang.to_string());
            }
        }
    }
    langs
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
    fn frontmatter_is_metadata_not_content() {
        let m = Model::default();
        let html = render("---\ntitle: t\n---\nprose\n", &m, &Grammars::default());
        assert!(!html.contains("title: t"), "{html}");
        assert!(html.contains("prose"));
    }

    #[test]
    fn copy_flag_wraps_the_block_and_leaves_the_language() {
        let (m, g) = (Model::default(), Grammars::default());
        let html = render("```sh copy\nls\n```\n", &m, &g);
        assert_eq!(
            html,
            "<div class=\"copyable\">\n<pre><code class=\"language-sh\">ls\n</code></pre>\n</div>\n"
        );
        let bare = render("```copy\nls\n```\n", &m, &g);
        assert!(bare.contains("<pre><code>ls"), "{bare}");
    }

    #[test]
    fn unflagged_blocks_are_not_copyable() {
        let html = render("```sh\nls\n```\n", &Model::default(), &Grammars::default());
        assert!(!html.contains("copyable"), "{html}");
    }

    #[test]
    fn known_languages_are_highlighted_inside_the_usual_markup() {
        let (m, g) = (Model::default(), crate::highlight::json(None));
        let html = render("```json copy\n1\n```\n\n```yaml\na: <b>\n```\n", &m, &g);
        assert_eq!(
            html,
            "<div class=\"copyable\">\n<pre><code class=\"language-json\">\
             <span class=\"ts-number\">1</span>\n</code></pre>\n</div>\n\
             <pre><code class=\"language-yaml\">a: &lt;b&gt;\n</code></pre>\n"
        );
    }
}
