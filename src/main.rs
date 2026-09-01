use clap::{Parser, Subcommand};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use wirrel::commits::{parse_subject, CommitKind};
use wirrel::model::{self, Article, Model, Severity};
use wirrel::{frontmatter, git, gone_page, history_page, html, index_page, markdown, update_page};
use std::collections::HashMap;
use skim::prelude::{unbounded, Skim, SkimItem, SkimItemReceiver, SkimItemSender, SkimOptionsBuilder};
use std::borrow::Cow;
use std::error::Error;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "wirrel", about = "wirrel away nuts for the winter")]
struct Cli {
    #[arg(long, env = "WIRREL_REPO", default_value = ".", global = true)]
    repo: PathBuf,

    /// Absolute site origin for canonical tags (e.g. https://example.com).
    #[arg(long, default_value = "", global = true)]
    base_url: String,

    /// Directory holding articles, relative to the repo root.
    #[arg(long, default_value = ".", global = true)]
    article_root: String,

    /// Skip commit signature checks.
    #[arg(long, global = true)]
    no_verify_signatures: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List articles (id, slug, title).
    Index,
    /// Validate commit history and article content.
    Check,
    /// Fuzzy-find an article and print its markdown link.
    Link { query: Option<String> },
    /// Render the static site content (HTML pages) into a directory.
    Build {
        #[arg(long, default_value = "dist")]
        out: PathBuf,
    },
    /// Emit the redirect map (id permalinks) as JSON.
    Redirects,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let result = match &cli.command {
        Commands::Index => cmd_index(&cli),
        Commands::Check => cmd_check(&cli),
        Commands::Link { query } => cmd_link(&cli, query.as_deref()),
        Commands::Build { out } => cmd_build(&cli, out),
        Commands::Redirects => cmd_redirects(&cli),
    };
    if let Err(e) = result {
        tracing::error!("{e}");
        process::exit(2);
    }
}

fn load_model(cli: &Cli) -> Result<Model, Box<dyn Error>> {
    let commits = git::load(&cli.repo)?;
    let mut model = model::build_model(&commits, &cli.article_root);
    enrich_from_files(&cli.repo, &mut model);
    Ok(model)
}

/// Load commits + model, erroring out if the repo doesn't pass `check`.
fn load_valid_model(cli: &Cli) -> Result<(Vec<git::RawCommit>, Model), Box<dyn Error>> {
    let commits = git::load(&cli.repo)?;
    let mut model = model::build_model(&commits, &cli.article_root);
    enrich_from_files(&cli.repo, &mut model);

    let violations = collect_violations(cli, &commits, &model);
    let mut errors = 0;
    for v in &violations {
        match v.severity() {
            Severity::Error => {
                errors += 1;
                tracing::error!("{}", v.message());
            }
            Severity::Warning => tracing::warn!("{}", v.message()),
        }
    }
    if errors > 0 {
        return Err(format!("{errors} validation error(s); run `wirrel check`").into());
    }
    Ok((commits, model))
}

fn collect_violations(cli: &Cli, commits: &[git::RawCommit], model: &Model) -> Vec<model::Violation> {
    let opts = model::CheckOptions { verify_signatures: !cli.no_verify_signatures };
    let mut violations = model::check(commits, &cli.article_root, &opts);
    for a in &model.articles {
        if let Ok(content) = fs::read_to_string(cli.repo.join(&a.path)) {
            violations.extend(model::check_content(model, &cli.base_url, &a.path, &content));
        }
    }
    if let Ok(files) = git::tracked_files(&cli.repo) {
        violations.extend(model::check_tracked_files(&cli.article_root, &files));
    }
    violations
}

/// The one place file-derived article state is filled in; everything else about
/// an `Article` comes from the commit stream.
fn enrich_from_files(repo: &Path, model: &mut Model) {
    for a in &mut model.articles {
        if let Ok(content) = fs::read_to_string(repo.join(&a.path)) {
            a.frontmatter = frontmatter::split(&content).0;
            a.title = markdown::extract_title(&content);
        }
    }
}

fn cmd_index(cli: &Cli) -> Result<(), Box<dyn Error>> {
    for a in &load_model(cli)?.articles {
        println!("{:>3}  {:<34}  {}", a.id, a.slug, a.title.as_deref().unwrap_or(""));
    }
    Ok(())
}

fn cmd_check(cli: &Cli) -> Result<(), Box<dyn Error>> {
    let commits = git::load(&cli.repo)?;
    let model = model::build_model(&commits, &cli.article_root);
    let violations = collect_violations(cli, &commits, &model);

    let (errors, warnings): (Vec<_>, Vec<_>) =
        violations.iter().partition(|v| v.severity() == Severity::Error);

    for w in &warnings {
        tracing::warn!("{}", w.message());
    }
    for e in &errors {
        tracing::error!("{}", e.message());
    }

    if errors.is_empty() {
        tracing::info!(
            "ok: {} articles, {} warning(s)",
            model.articles.len(),
            warnings.len()
        );
        return Ok(());
    }
    tracing::error!("{} error(s), {} warning(s)", errors.len(), warnings.len());
    process::exit(1);
}

fn cmd_link(cli: &Cli, query: Option<&str>) -> Result<(), Box<dyn Error>> {
    let (_, model) = load_valid_model(cli)?;
    if std::io::stdin().is_terminal() {
        link_interactive(&model, query)
    } else {
        link_filtered(&model, query.unwrap_or_default());
        Ok(())
    }
}

/// The authoring form to paste into source: `build` hydrates `/id/N/` to the
/// slug, and it also resolves via the 301 redirect when hydration doesn't run.
fn link(a: &Article) -> String {
    let text = a.title.clone().unwrap_or_else(|| a.slug.clone());
    format!("[{}](/id/{}/)", text, a.id)
}

struct ArticleItem {
    search: String,
    link: String,
}

impl SkimItem for ArticleItem {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.search)
    }
    fn output(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.link)
    }
}

/// fzf-style live picker; prints the chosen article's markdown link.
fn link_interactive(model: &Model, query: Option<&str>) -> Result<(), Box<dyn Error>> {
    let options = SkimOptionsBuilder::default()
        .prompt(Some("article> "))
        .query(query)
        .multi(false)
        .build()?;

    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
    for a in &model.articles {
        tx.send(Arc::new(ArticleItem {
            search: format!("{}  {}", a.slug, a.title.as_deref().unwrap_or("")),
            link: link(a),
        }))?;
    }
    drop(tx);

    if let Some(out) = Skim::run_with(&options, Some(rx)) {
        if !out.is_abort {
            for item in out.selected_items {
                println!("{}", item.output());
            }
        }
    }
    Ok(())
}

/// Non-interactive fallback: print matches ranked by fuzzy score.
fn link_filtered(model: &Model, query: &str) {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, &Article)> = model
        .articles
        .iter()
        .filter_map(|a| {
            let hay = format!("{} {}", a.slug, a.title.as_deref().unwrap_or(""));
            let score = pattern.score(Utf32Str::new(&hay, &mut buf), &mut matcher)?;
            Some((score, a))
        })
        .collect();
    scored.sort_by(|x, y| y.0.cmp(&x.0));

    for (_, a) in scored {
        println!("{}", link(a));
    }
}

/// Recent events shown on the full history page and on the index.
const HISTORY_LIMIT: usize = 25;
const INDEX_RECENT: usize = 5;

fn cmd_redirects(cli: &Cli) -> Result<(), Box<dyn Error>> {
    let (_, model) = load_valid_model(cli)?;
    let redirects = model::plan_redirects(&model);
    println!("{}", serde_json::to_string_pretty(&redirects)?);
    Ok(())
}

fn cmd_build(cli: &Cli, out: &Path) -> Result<(), Box<dyn Error>> {
    let (commits, model) = load_valid_model(cli)?;
    let base = cli.base_url.trim_end_matches('/');
    fs::create_dir_all(out)?;

    for a in &model.articles {
        let content = fs::read_to_string(cli.repo.join(&a.path))?;
        let header = html::meta_header(&a.created, &a.updated);
        let rendered = markdown::render(&content, &model);
        let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
        let canonical = format!("{base}{}", html::url(&a.slug));
        // A frontmatter title with no heading in the body has to lead the page
        // itself, or the article renders with no visible title at all.
        let lead = a
            .frontmatter
            .title
            .as_deref()
            .filter(|_| !markdown::has_heading(&content))
            .map(|t| format!("<h1>{}</h1>\n", html::escape(t)))
            .unwrap_or_default();

        let dir = out.join(&a.slug);
        fs::create_dir_all(&dir)?;
        let body = format!("{header}{lead}{rendered}");
        fs::write(dir.join("index.html"), html::page(&title, &canonical, &body))?;
    }

    // Generated pages: site index (with recent changes) and full history.
    let index = html::page("index", &format!("{base}/"), &index_page::body(&commits, &model, INDEX_RECENT));
    fs::write(out.join("index.html"), index)?;

    let history = html::page(
        "update history",
        &format!("{base}/history/"),
        &history_page::body(&commits, &model, HISTORY_LIMIT),
    );
    let history_dir = out.join("history");
    fs::create_dir_all(&history_dir)?;
    fs::write(history_dir.join("index.html"), history)?;

    // Dead ends. A slug an article moved away from keeps a tombstone so the
    // address stays honest without forwarding; `404.html` covers the rest.
    let retired = model::retired_slugs(&model);
    for slug in &retired {
        let dir = out.join(slug);
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("index.html"),
            html::dead_end_page("this article has moved", &gone_page::moved_body()),
        )?;
    }
    fs::write(
        out.join("404.html"),
        html::dead_end_page("no such article", &gone_page::missing_body()),
    )?;

    // A dedicated diff page per `update:` commit.
    let mut by_sha: HashMap<&str, Vec<&Article>> = HashMap::new();
    for a in &model.articles {
        for sha in &a.history {
            by_sha.entry(sha.as_str()).or_default().push(a);
        }
    }
    let mut updates = 0;
    for c in &commits {
        let Ok(subject) = parse_subject(&c.subject) else { continue };
        if !matches!(subject.kind, CommitKind::Update) {
            continue;
        }
        let diff = git::commit_diff(&cli.repo, &c.sha)?;
        let touched = by_sha.get(c.sha.as_str()).cloned().unwrap_or_default();
        let page = html::page(
            &format!("update: {}", subject.description),
            &format!("{base}/updates/{}/", c.sha),
            &update_page::body(&subject.description, &c.body, &touched, &diff),
        );
        let dir = out.join("updates").join(&c.sha);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("index.html"), page)?;
        updates += 1;
    }

    println!(
        "built {} articles + index + history + {} update pages + {} tombstones -> {}",
        model.articles.len(),
        updates,
        retired.len(),
        out.display()
    );
    Ok(())
}
