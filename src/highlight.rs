//! Syntax highlighting with tree-sitter grammars. Nothing here touches the
//! disk: grammars arrive already loaded (`grammars.rs` does that), so a test can
//! hand one in from a grammar crate.

use crate::html::escape;
use regex::Regex;
use std::fmt::Write;
use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// The capture names that get a span, each as a `ts-` class with dots turned to
/// hyphens. A capture not listed takes its longest listed match
/// (`string.special.key` renders as `ts-string-special`), or no span at all —
/// which is why plain `variable` and `punctuation` are left out: they'd wrap
/// nearly every token for no colour.
pub const CAPTURE_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "character",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "escape",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "label",
    "module",
    "number",
    "property",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable.builtin",
    "variable.parameter",
];

pub struct Grammar {
    name: String,
    aliases: Option<Regex>,
    config: HighlightConfiguration,
}

impl Grammar {
    /// `aliases` matches the other fence names the grammar answers to, as a
    /// grammar's `injection-regex` does (`^(js|javascript)$`).
    pub fn new(
        name: &str,
        language: Language,
        aliases: Option<Regex>,
        highlights: &str,
        injections: &str,
        locals: &str,
    ) -> Result<Self, String> {
        let mut config = HighlightConfiguration::new(language, name, highlights, injections, locals)
            .map_err(|e| format!("bad query: {e}"))?;
        config.configure(CAPTURE_NAMES);
        Ok(Self { name: name.to_string(), aliases, config })
    }
}

#[derive(Default)]
pub struct Grammars(Vec<Grammar>);

impl Grammars {
    pub fn new(grammars: Vec<Grammar>) -> Self {
        Self(grammars)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn knows(&self, lang: &str) -> bool {
        self.find(lang).is_some()
    }

    /// A name match beats an alias, so one grammar's loose regex can't shadow
    /// another grammar by its own name.
    fn find(&self, lang: &str) -> Option<&Grammar> {
        self.0.iter().find(|g| g.name == lang).or_else(|| {
            self.0.iter().find(|g| g.aliases.as_ref().is_some_and(|re| re.is_match(lang)))
        })
    }

    /// `code` as escaped HTML with highlight spans, or `None` when no grammar
    /// covers `lang` or highlighting fails — the caller renders it plain.
    pub fn highlight(&self, lang: &str, code: &str) -> Option<String> {
        let grammar = self.find(lang)?;
        let mut highlighter = Highlighter::new();
        let events = highlighter
            .highlight(&grammar.config, code.as_bytes(), None, None, |injected| {
                self.find(injected).map(|g| &g.config)
            })
            .ok()?;

        let mut out = String::new();
        for event in events {
            match event.ok()? {
                HighlightEvent::Source { start, end } => out.push_str(&escape(code.get(start..end)?)),
                HighlightEvent::HighlightStart(h) => {
                    let class = CAPTURE_NAMES[h.0].replace('.', "-");
                    write!(out, "<span class=\"ts-{class}\">").unwrap();
                }
                HighlightEvent::HighlightEnd => out.push_str("</span>"),
            }
        }
        Some(out)
    }
}

/// tree-sitter-json, compiled in for tests only.
#[cfg(test)]
pub fn json(aliases: Option<&str>) -> Grammars {
    let grammar = Grammar::new(
        "json",
        tree_sitter_json::LANGUAGE.into(),
        aliases.map(|a| Regex::new(a).unwrap()),
        tree_sitter_json::HIGHLIGHTS_QUERY,
        "",
        "",
    )
    .unwrap();
    Grammars::new(vec![grammar])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_into_escaped_spans() {
        let html = json(None).highlight("json", r#"{"k": "<b>", "n": 1}"#).unwrap();
        assert_eq!(
            html,
            "{<span class=\"ts-string\">\"k\"</span>: \
             <span class=\"ts-string\">\"&lt;b&gt;\"</span>, \
             <span class=\"ts-string\">\"n\"</span>: \
             <span class=\"ts-number\">1</span>}"
        );
    }

    #[test]
    fn finds_grammars_by_name_or_alias() {
        let grammars = json(Some("^jsonc?$"));
        assert!(grammars.knows("json"));
        assert!(grammars.knows("jsonc"));
        assert!(!grammars.knows("yaml"));
        assert!(Grammars::default().highlight("json", "1").is_none());
    }
}
