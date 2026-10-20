//! Loads tree-sitter grammars from the grammars directory — the one place wirrel
//! runs native code it didn't compile in. Each subdirectory is a grammar, named
//! for its directory:
//!
//! ```text
//! <name>/parser.{dylib,so,dll}    from `tree-sitter build`
//! <name>/queries/highlights.scm   (+ injections.scm, locals.scm if any)
//! <name>/tree-sitter.json         optional; its `injection-regex` adds aliases
//! ```

use crate::highlight::{Grammar, Grammars};
use libloading::Library;
use regex::Regex;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use tree_sitter::{Language, LANGUAGE_VERSION, MIN_COMPATIBLE_LANGUAGE_VERSION};
use tree_sitter_language::LanguageFn;

/// Every grammar under `dir` that loads. One that doesn't is warned about and
/// skipped: its code blocks render plain, which is no reason to fail a build.
pub fn load(dir: &Path) -> Grammars {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Grammars::default(),
        Err(e) => {
            tracing::warn!("grammars {}: {e}", dir.display());
            return Grammars::default();
        }
    };
    let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();

    let mut grammars = Vec::new();
    for dir in dirs {
        let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        match load_one(&dir, &name) {
            Ok(grammar) => grammars.push(grammar),
            Err(e) => tracing::warn!("grammar {name}: {e}; its code blocks render plain"),
        }
    }
    Grammars::new(grammars)
}

fn load_one(dir: &Path, name: &str) -> Result<Grammar, String> {
    let parser = dir.join(format!("parser.{}", std::env::consts::DLL_EXTENSION));
    if !parser.is_file() {
        return Err(format!("no {}", parser.display()));
    }
    let language = load_language(&parser, name)?;
    let abi = language.abi_version();
    if !(MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION).contains(&abi) {
        return Err(format!(
            "parser ABI {abi} outside the supported {MIN_COMPATIBLE_LANGUAGE_VERSION}..={LANGUAGE_VERSION}; \
             regenerate it with a matching tree-sitter"
        ));
    }

    let queries = dir.join("queries");
    let highlights = fs::read_to_string(queries.join("highlights.scm"))
        .map_err(|e| format!("queries/highlights.scm: {e}"))?;
    let injections = optional(&queries.join("injections.scm"))?;
    let locals = optional(&queries.join("locals.scm"))?;

    Grammar::new(name, language, aliases(dir, name)?, &highlights, &injections, &locals)
}

fn load_language(path: &Path, name: &str) -> Result<Language, String> {
    let symbol = format!("tree_sitter_{}", name.replace('-', "_"));
    // SAFETY: loading runs the library's initialisers, and the symbol is trusted
    // to be a tree-sitter language function. Both are the price of a grammar
    // plugin; the grammars directory is as trusted as the binary itself.
    unsafe {
        let library = Library::new(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let func = *library
            .get::<unsafe extern "C" fn() -> *const ()>(symbol.as_bytes())
            .map_err(|e| format!("{}: no `{symbol}`: {e}", path.display()))?;
        // The language points into the library, so it stays loaded for the
        // rest of the process.
        std::mem::forget(library);
        Ok(Language::new(LanguageFn::from_raw(func)))
    }
}

fn optional(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// The `injection-regex` of this grammar's entry in `tree-sitter.json`, as
/// copied from the grammar's repo. A repo of several grammars lists each, so
/// the entry is picked by name.
fn aliases(dir: &Path, name: &str) -> Result<Option<Regex>, String> {
    let text = optional(&dir.join("tree-sitter.json"))?;
    if text.is_empty() {
        return Ok(None);
    }
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("tree-sitter.json: {e}"))?;
    let entry = json["grammars"]
        .as_array()
        .and_then(|gs| gs.iter().find(|g| g["name"] == name));
    let Some(re) = entry.and_then(|g| g["injection-regex"].as_str()) else {
        return Ok(None);
    };
    Regex::new(re).map(Some).map_err(|e| format!("tree-sitter.json injection-regex: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_grammar_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("rust")).unwrap();
        assert!(load(dir.path()).is_empty());
        assert!(load(&dir.path().join("absent")).is_empty());
    }
}
