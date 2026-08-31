//! Leading `---` metadata block on an article file.
//!
//! Deliberately not a full YAML parser: flat `key: value` lines, which is all
//! the article format admits.

use serde::Serialize;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Frontmatter {
    pub title: Option<String>,
}

/// Split a source file into its frontmatter and the markdown body. A file
/// without a well-formed opening/closing fence is all body.
pub fn split(src: &str) -> (Frontmatter, &str) {
    let Some(rest) = src.strip_prefix("---").and_then(after_line_break) else {
        return (Frontmatter::default(), src);
    };

    let mut fields = Frontmatter::default();
    let mut cursor = rest;
    loop {
        let (line, tail) = match cursor.find('\n') {
            Some(i) => (&cursor[..i], &cursor[i + 1..]),
            // Unterminated: the fence was never closed, so it isn't frontmatter.
            None => return (Frontmatter::default(), src),
        };
        let trimmed = line.trim_end_matches('\r');
        if trimmed == "---" || trimmed == "..." {
            return (fields, tail);
        }
        assign(&mut fields, trimmed);
        cursor = tail;
    }
}

fn assign(fields: &mut Frontmatter, line: &str) {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return;
    }
    let Some((key, value)) = line.split_once(':') else { return };
    let value = unquote(value.trim());
    if key.trim() == "title" && !value.is_empty() {
        fields.title = Some(value.to_string());
    }
}

fn unquote(value: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
            return inner;
        }
    }
    value
}

/// The text after the line break that must follow an opening fence.
fn after_line_break(s: &str) -> Option<&str> {
    let s = s.strip_prefix('\r').unwrap_or(s);
    s.strip_prefix('\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_fields_from_body() {
        let (fm, body) = split("---\ntitle: A Willow\n---\n# heading\ntext\n");
        assert_eq!(fm.title.as_deref(), Some("A Willow"));
        assert_eq!(body, "# heading\ntext\n");
    }

    #[test]
    fn quotes_comments_and_unknown_keys() {
        let (fm, body) = split("---\n# note\nlayout: post\ntitle: \"Quoted: colon\"\n---\nbody");
        assert_eq!(fm.title.as_deref(), Some("Quoted: colon"));
        assert_eq!(body, "body");
    }

    #[test]
    fn no_fence_or_unterminated_is_all_body() {
        let plain = "# heading\n---\nrule\n";
        assert_eq!(split(plain), (Frontmatter::default(), plain));

        let open = "---\ntitle: x\nnever closed\n";
        assert_eq!(split(open), (Frontmatter::default(), open));
    }

    #[test]
    fn empty_title_is_absent() {
        let (fm, _) = split("---\ntitle:\n---\n# heading\n");
        assert_eq!(fm.title, None);
    }
}
