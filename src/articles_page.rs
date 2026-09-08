//! The two exhaustive article listings: by current path, and by id.

use crate::html::{date, escape};
use crate::model::{Article, Model};
use crate::routes;

pub fn by_path(model: &Model) -> String {
    let mut articles: Vec<&Article> = model.articles.iter().collect();
    articles.sort_by(|a, b| a.slug.cmp(&b.slug));
    list("articles by path", &articles, |_| String::new())
}

pub fn by_id(model: &Model) -> String {
    let mut articles: Vec<&Article> = model.articles.iter().collect();
    articles.sort_by_key(|a| a.id);
    list("articles by id", &articles, |a| format!("<code>{}</code> ", a.id))
}

fn list(heading: &str, articles: &[&Article], lead: impl Fn(&Article) -> String) -> String {
    let mut out = format!("<h1>{heading}</h1>\n<ul>\n");
    for a in articles {
        let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
        out.push_str(&format!(
            "<li>{}<a href=\"{}\">{}</a> <small>{}</small></li>\n",
            lead(a),
            routes::article(&a.slug),
            escape(&title),
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

    fn model() -> Model {
        let commits = [
            RawCommit {
                sha: "s1".into(),
                subject: "create: willow".into(),
                body: String::new(),
                date: "2026-01-02T00:00:00Z".into(),
                changed: vec![FileChange::Added("src/willow.md".into())],
            },
            RawCommit {
                sha: "s2".into(),
                subject: "create: oak".into(),
                body: String::new(),
                date: "2026-01-03T00:00:00Z".into(),
                changed: vec![FileChange::Added("src/oak.md".into())],
            },
        ];
        build_model(&commits, "src")
    }

    #[test]
    fn orderings_differ_and_both_link_articles() {
        let m = model();
        let path = by_path(&m);
        let id = by_id(&m);

        assert!(path.contains("href=\"/article/oak/\""));
        // `oak` was created second, so the two orderings disagree.
        assert!(path.find("oak").unwrap() < path.find("willow").unwrap());
        assert!(id.find("willow").unwrap() < id.find("oak").unwrap());
        assert!(id.contains("<code>1</code>"));
    }
}
