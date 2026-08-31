use clap::{Parser, Subcommand};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use quill::model::{self, Article, Model, Severity};
use quill::{git, markdown};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "quill", about = "tool for compiling the gnostinomicon")]
struct Cli {
    #[arg(long, env = "QUILL_REPO", default_value = ".", global = true)]
    repo: PathBuf,

    #[arg(long, default_value = "https://gnostinomicon.oclyke.dev", global = true)]
    base_url: String,

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
    /// Render the static site (HTML pages + redirect manifest).
    Build {
        #[arg(long, default_value = "dist")]
        out: PathBuf,
    },
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
    };
    if let Err(e) = result {
        tracing::error!("{e}");
        process::exit(2);
    }
}

fn load_model(cli: &Cli) -> Result<Model, Box<dyn Error>> {
    let commits = git::load(&cli.repo)?;
    let mut model = model::build_model(&commits);
    enrich_titles(&cli.repo, &mut model);
    Ok(model)
}

/// Load a model, erroring out if the repo doesn't pass `check`.
fn load_valid_model(cli: &Cli) -> Result<Model, Box<dyn Error>> {
    let commits = git::load(&cli.repo)?;
    let mut model = model::build_model(&commits);
    enrich_titles(&cli.repo, &mut model);

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
        return Err(format!("{errors} validation error(s); run `quill check`").into());
    }
    Ok(model)
}

fn collect_violations(cli: &Cli, commits: &[git::RawCommit], model: &Model) -> Vec<model::Violation> {
    let opts = model::CheckOptions { verify_signatures: !cli.no_verify_signatures };
    let mut violations = model::check(commits, &opts);
    for a in &model.articles {
        if let Ok(content) = fs::read_to_string(cli.repo.join(&a.path)) {
            violations.extend(model::check_content(model, &a.path, &content));
        }
    }
    violations.extend(model::plan_redirects(model).1);
    violations
}

fn enrich_titles(repo: &Path, model: &mut Model) {
    for a in &mut model.articles {
        if let Ok(content) = fs::read_to_string(repo.join(&a.path)) {
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
    let model = model::build_model(&commits);
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
    let model = load_valid_model(cli)?;

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query.unwrap_or_default(), CaseMatching::Ignore, Normalization::Smart);
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
        let text = a.title.clone().unwrap_or_else(|| a.slug.clone());
        println!("[{}](/{}/)   (id:{})", text, a.slug, a.id);
    }
    Ok(())
}

fn cmd_build(cli: &Cli, out: &Path) -> Result<(), Box<dyn Error>> {
    let model = load_valid_model(cli)?;
    fs::create_dir_all(out)?;

    for a in &model.articles {
        let content = fs::read_to_string(cli.repo.join(&a.path))?;
        let body = markdown::render(&content, &model);
        let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
        let canonical = format!("{}/{}/", cli.base_url.trim_end_matches('/'), a.slug);

        let dir = out.join(&a.slug);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("index.html"), page(&title, &canonical, &body))?;
    }

    let (redirects, _) = model::plan_redirects(&model);
    let manifest: String = redirects
        .iter()
        .map(|r| format!("{}  {}  301\n", r.from, r.to))
        .collect();
    fs::write(out.join("_redirects"), manifest)?;

    println!(
        "built {} articles, {} redirects -> {}",
        model.articles.len(),
        redirects.len(),
        out.display()
    );
    Ok(())
}

fn page(title: &str, canonical: &str, body: &str) -> String {
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

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
