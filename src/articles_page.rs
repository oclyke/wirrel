//! The two exhaustive article listings: the source tree, and id order.

use crate::html::{date, escape};
use crate::model::{Article, Model};
use crate::routes;
use std::collections::BTreeMap;

/// Articles laid out as the tree of files they are, rooted at the article
/// root: each directory a foldable `<details>`, each file a link to its article.
pub fn tree(model: &Model) -> String {
    let mut root = Dir::default();
    for a in &model.articles {
        let path = a.path.strip_prefix(&model.root).unwrap_or(&a.path);
        root.insert(path.split('/').collect::<Vec<_>>().as_slice(), a);
    }
    let mut out = String::from("<ul class=\"tree\">\n");
    root.render(&mut out);
    out.push_str("</ul>\n");
    out
}

/// A directory being folded up from article paths. `BTreeMap` keeps each
/// level in name order; rendering lists the directories before the files, the
/// way an editor's explorer does.
#[derive(Default)]
struct Dir<'a> {
    entries: BTreeMap<&'a str, Entry<'a>>,
}

enum Entry<'a> {
    Dir(Dir<'a>),
    File(&'a Article),
}

impl<'a> Dir<'a> {
    fn insert(&mut self, parts: &[&'a str], a: &'a Article) {
        match parts {
            [] => {}
            [file] => {
                self.entries.insert(file, Entry::File(a));
            }
            [dir, rest @ ..] => {
                let entry = self.entries.entry(dir).or_insert_with(|| Entry::Dir(Dir::default()));
                if let Entry::Dir(d) = entry {
                    d.insert(rest, a);
                }
            }
        }
    }

    fn render(&self, out: &mut String) {
        for (name, entry) in &self.entries {
            if let Entry::Dir(d) = entry {
                out.push_str(&format!(
                    "<li class=\"dir\"><details open><summary>{}</summary><ul>\n",
                    escape(name)
                ));
                d.render(out);
                out.push_str("</ul></details></li>\n");
            }
        }
        for (name, entry) in &self.entries {
            if let Entry::File(a) = entry {
                out.push_str(&format!(
                    "<li class=\"file\"><a href=\"{}\">{}</a> <small>{}</small></li>\n",
                    routes::article(&a.slug),
                    escape(name),
                    date(&a.updated),
                ));
            }
        }
    }
}

pub fn by_id(model: &Model) -> String {
    let mut articles: Vec<&Article> = model.articles.iter().collect();
    articles.sort_by_key(|a| a.id);
    let mut out = String::from("<h1>articles by id</h1>\n<ul>\n");
    for a in articles {
        out.push_str(&format!(
            "<li><code>{}</code> <a href=\"{}\">{}</a> <small>{}</small></li>\n",
            a.id,
            routes::article(&a.slug),
            escape(a.title()),
            date(&a.updated),
        ));
    }
    out.push_str("</ul>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileChange, RawCommit};
    use crate::model::build_model;

    fn create(sha: &str, path: &str) -> RawCommit {
        RawCommit {
            sha: sha.into(),
            subject: format!("create: {path}"),
            body: String::new(),
            date: "2026-01-02T00:00:00Z".into(),
            changed: vec![FileChange::Added(path.into())],
            stats: Vec::new(),
        }
    }

    #[test]
    fn tree_folds_directories_first_and_names_files_not_titles() {
        let commits = [
            create("s1", "src/alder.md"),
            create("s2", "src/trees/oak.md"),
            create("s3", "src/trees/index.md"),
            create("s4", "src/willow.md"),
        ];
        let mut model = build_model(&commits, "src");
        model.articles[0].frontmatter.title = Some("The Willow".into());
        let html = tree(&model);

        // Rooted at the article root, so `src/` itself never appears; `trees/`
        // sorts ahead of `alder.md` because directories come first.
        assert_eq!(
            html,
            "<ul class=\"tree\">\n\
             <li class=\"dir\"><details open><summary>trees</summary><ul>\n\
             <li class=\"file\"><a href=\"/article/trees/\">index.md</a> <small>2026-01-02</small></li>\n\
             <li class=\"file\"><a href=\"/article/trees/oak/\">oak.md</a> <small>2026-01-02</small></li>\n\
             </ul></details></li>\n\
             <li class=\"file\"><a href=\"/article/alder/\">alder.md</a> <small>2026-01-02</small></li>\n\
             <li class=\"file\"><a href=\"/article/willow/\">willow.md</a> <small>2026-01-02</small></li>\n\
             </ul>\n"
        );
        assert!(!html.contains("The Willow"));
    }

    #[test]
    fn by_id_orders_by_id_and_shows_titles() {
        let commits = [create("s1", "src/willow.md"), create("s2", "src/oak.md")];
        let mut model = build_model(&commits, "src");
        model.articles[1].frontmatter.title = Some("Mighty Oak".into());
        let html = by_id(&model);

        assert!(html.find("willow").unwrap() < html.find("Mighty Oak").unwrap());
        assert!(html.contains("<code>1</code>"));
        assert!(html.contains("href=\"/article/oak/\""));
    }
}
