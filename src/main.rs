use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{
    Attribute, Color, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute, queue, terminal};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use wirrel::commits::parse_subject;
use wirrel::model::{self, Article, Model, Severity};
use wirrel::{
    articles_page, change_page, changelog_page, changes_page, frontmatter, git, gone_page, html,
    index_page, markdown, routes,
};
use std::error::Error;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "wirrel", about = "wirrel away nuts for the winter")]
struct Cli {
    #[arg(long, env = "WIRREL_REPO", default_value = ".", global = true)]
    repo: PathBuf,

    /// Directory holding articles, relative to the repo root.
    #[arg(long, env = "WIRREL_ARTICLES", default_value = "./articles", global = true)]
    articles: String,

    /// Directory holding site assets, relative to the repo root. Its contents
    /// are copied verbatim; where they land is the address scheme's business.
    #[arg(long, env = "WIRREL_ASSETS", default_value = "./assets", global = true)]
    assets: String,

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
    let mut model = model::build_model(&commits, &cli.articles);
    enrich_from_files(&cli.repo, &mut model);
    Ok(model)
}

/// Load commits + model, erroring out if the repo doesn't pass `check`.
fn load_valid_model(cli: &Cli) -> Result<(Vec<git::RawCommit>, Model), Box<dyn Error>> {
    let commits = git::load(&cli.repo)?;
    let mut model = model::build_model(&commits, &cli.articles);
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
    let mut violations = model::check(commits, &cli.articles);
    for a in &model.articles {
        if let Ok(content) = fs::read_to_string(cli.repo.join(&a.path)) {
            violations.extend(model::check_content(model, &a.path, &content));
        }
    }
    if let Ok(files) = git::tracked_files(&cli.repo) {
        violations.extend(model::check_tracked_files(&cli.articles, &cli.assets, &files));
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
    let model = model::build_model(&commits, &cli.articles);
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
    format!("[{}]({})", text, routes::id_permalink(a.id))
}

const PROMPT: &str = "article> ";

/// The line shown in the picker, and the text the query is matched against.
fn haystack(a: &Article) -> String {
    match a.title.as_deref() {
        Some(t) => format!("{}  {t}", a.slug),
        None => a.slug.clone(),
    }
}

/// Articles matching `query`, best score first. An empty query keeps them all,
/// in create order; `sort_by` is stable, so ties hold that order too.
fn ranked<'a>(articles: &'a [Article], query: &str) -> Vec<&'a Article> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, &Article)> = articles
        .iter()
        .filter_map(|a| {
            let hay = haystack(a);
            let score = pattern.score(Utf32Str::new(&hay, &mut buf), &mut matcher)?;
            Some((score, a))
        })
        .collect();
    scored.sort_by(|x, y| y.0.cmp(&x.0));
    scored.into_iter().map(|(_, a)| a).collect()
}

/// fzf-style live picker; prints the chosen article's markdown link.
fn link_interactive(model: &Model, query: Option<&str>) -> Result<(), Box<dyn Error>> {
    // The UI goes to stderr so a piped stdout carries only the chosen link.
    let mut ui = std::io::stderr();
    terminal::enable_raw_mode()?;
    execute!(ui, EnterAlternateScreen)?;
    let picked = pick(&mut ui, &model.articles, query.unwrap_or_default());
    execute!(ui, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    if let Some(a) = picked? {
        println!("{}", link(a));
    }
    Ok(())
}

fn pick<'a, W: Write>(
    ui: &mut W,
    articles: &'a [Article],
    query: &str,
) -> Result<Option<&'a Article>, Box<dyn Error>> {
    let mut query = query.to_string();
    let mut matches = ranked(articles, &query);
    let mut cursor = 0usize;

    loop {
        draw(ui, &query, &matches, articles.len(), cursor)?;

        let Event::Key(k) = event::read()? else { continue };
        if k.kind == KeyEventKind::Release {
            continue;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => return Ok(None),
            KeyCode::Char('c') if ctrl => return Ok(None),
            KeyCode::Enter => return Ok(matches.get(cursor).copied()),
            KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Char('p') if ctrl => cursor = cursor.saturating_sub(1),
            KeyCode::Down => cursor = (cursor + 1).min(matches.len().saturating_sub(1)),
            KeyCode::Char('n') if ctrl => cursor = (cursor + 1).min(matches.len().saturating_sub(1)),
            KeyCode::Backspace => {
                query.pop();
                matches = ranked(articles, &query);
                cursor = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                query.push(c);
                matches = ranked(articles, &query);
                cursor = 0;
            }
            _ => {}
        }
    }
}

fn draw<W: Write>(
    ui: &mut W,
    query: &str,
    matches: &[&Article],
    total: usize,
    cursor: usize,
) -> Result<(), Box<dyn Error>> {
    // A detached or degenerate tty can report 0x0; fall back to a sane window.
    let (width, rows) = match terminal::size() {
        Ok((c, r)) if c > 0 && r > 0 => (c as usize, r as usize),
        _ => (80, 24),
    };
    // Two lines go to the prompt and the counter.
    let room = rows.saturating_sub(2);
    // Scroll only once the cursor would fall off the bottom of the window.
    let top = cursor.saturating_sub(room.saturating_sub(1));

    queue!(ui, Clear(ClearType::All), cursor::MoveTo(0, 0))?;
    queue!(ui, SetForegroundColor(Color::Cyan))?;
    write!(ui, "{PROMPT}")?;
    queue!(ui, ResetColor)?;
    write!(ui, "{query}")?;

    queue!(ui, cursor::MoveTo(0, 1), SetForegroundColor(Color::DarkYellow))?;
    write!(ui, "  {}/{total}", matches.len())?;
    queue!(ui, ResetColor)?;

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut hits = Vec::new();

    for (row, (i, a)) in matches.iter().enumerate().skip(top).take(room).enumerate() {
        let text = haystack(a);
        hits.clear();
        pattern.indices(Utf32Str::new(&text, &mut buf), &mut matcher, &mut hits);
        hits.sort_unstable();
        hits.dedup();
        queue!(ui, cursor::MoveTo(0, row as u16 + 2))?;
        draw_row(ui, &text, &hits, i == cursor, width)?;
    }

    let col = (PROMPT.chars().count() + query.chars().count()) as u16;
    queue!(ui, cursor::MoveTo(col, 0))?;
    ui.flush()?;
    Ok(())
}

/// One result line: fzf's red pointer, highlighted row, green matched chars.
fn draw_row<W: Write>(
    ui: &mut W,
    text: &str,
    hits: &[u32],
    selected: bool,
    width: usize,
) -> Result<(), Box<dyn Error>> {
    let base = if selected { Color::White } else { Color::Reset };
    if selected {
        queue!(ui, SetBackgroundColor(Color::DarkGrey))?;
    }
    queue!(ui, SetForegroundColor(if selected { Color::Red } else { Color::Reset }))?;
    write!(ui, "{} ", if selected { ">" } else { " " })?;

    let shown: String = text.chars().take(width.saturating_sub(2)).collect();
    let mut lit = false;
    queue!(ui, SetForegroundColor(base))?;
    for (i, c) in shown.chars().enumerate() {
        let hit = hits.binary_search(&(i as u32)).is_ok();
        if hit != lit {
            if hit {
                queue!(ui, SetForegroundColor(Color::Green), SetAttribute(Attribute::Bold))?;
            } else {
                queue!(ui, SetAttribute(Attribute::NormalIntensity), SetForegroundColor(base))?;
            }
            lit = hit;
        }
        write!(ui, "{c}")?;
    }

    // Pad so the selected row's highlight spans the whole line.
    let used = 2 + shown.chars().count();
    if width > used {
        write!(ui, "{}", " ".repeat(width - used))?;
    }
    queue!(ui, SetAttribute(Attribute::NormalIntensity), ResetColor)?;
    Ok(())
}

/// Non-interactive fallback: print matches ranked by fuzzy score.
fn link_filtered(model: &Model, query: &str) {
    for a in ranked(&model.articles, query) {
        println!("{}", link(a));
    }
}

/// How many recent events the index shows; the changelog is complete.
const INDEX_RECENT: usize = 5;

fn cmd_redirects(cli: &Cli) -> Result<(), Box<dyn Error>> {
    let (_, model) = load_valid_model(cli)?;
    let redirects = model::plan_redirects(&model);
    println!("{}", serde_json::to_string_pretty(&redirects)?);
    Ok(())
}

fn cmd_build(cli: &Cli, out: &Path) -> Result<(), Box<dyn Error>> {
    let (commits, model) = load_valid_model(cli)?;
    fs::create_dir_all(out)?;
    write_assets(&cli.repo.join(&cli.assets), out)?;

    for a in &model.articles {
        let content = fs::read_to_string(cli.repo.join(&a.path))?;
        let header = html::meta_header(a.id, &a.created, &a.updated);
        let rendered = markdown::render(&content, &model);
        let title = a.title.clone().unwrap_or_else(|| a.slug.clone());
        let route = routes::article(&a.slug);
        // A frontmatter title with no heading in the body has to lead the page
        // itself, or the article renders with no visible title at all.
        let lead = a
            .frontmatter
            .title
            .as_deref()
            .filter(|_| !markdown::has_heading(&content))
            .map(|t| format!("<h1>{}</h1>\n", html::escape(t)))
            .unwrap_or_default();

        let body = format!("{header}{lead}{rendered}");
        write_route(out, &route, &html::page(&title, &body))?;

        // The id is the permanent handle; the stub makes it resolve even where
        // the redirect map can't be installed.
        let by_id = routes::article_by_id(a.id);
        write_route(out, &by_id, &html::stub_page(&format!("id {}", a.id), &route))?;

        write_page(
            out,
            &routes::changes_for(a.id),
            &format!("changes: {title}"),
            &changes_page::body(&model, a),
        )?;
    }

    write_page(
        out,
        &routes::index(),
        "index",
        &index_page::body(&model, INDEX_RECENT),
    )?;
    write_page(
        out,
        &routes::articles_by_path(),
        "articles by path",
        &articles_page::by_path(&model),
    )?;
    write_page(
        out,
        &routes::articles_by_id(),
        "articles by id",
        &articles_page::by_id(&model),
    )?;
    write_page(
        out,
        &routes::changelog(),
        "changelog",
        &changelog_page::body(&model),
    )?;

    // Dead ends. A slug an article moved away from keeps a tombstone so the
    // address stays honest without forwarding; `404.html` covers the rest.
    let retired = model::retired_slugs(&model);
    for slug in &retired {
        write_route(
            out,
            &routes::article(slug),
            &html::dead_end_page("this article has moved", &gone_page::moved_body()),
        )?;
    }
    fs::write(
        out.join("404.html"),
        html::dead_end_page("no such article", &gone_page::missing_body()),
    )?;

    // A page per commit that touched an article, so every changelog row lands
    // somewhere: creates and moves, not just updates.
    let mut changes = 0;
    for c in &commits {
        let Ok(subject) = parse_subject(&c.subject) else { continue };
        if model.changes_in(&c.sha).next().is_none() {
            continue;
        }
        let diff = git::commit_diff(&cli.repo, &c.sha)?;
        write_page(
            out,
            &routes::change(&c.sha),
            &format!("{}: {}", subject.kind.label(), subject.description),
            &change_page::body(&model, &subject, &c.body, &c.sha, &diff),
        )?;
        changes += 1;
    }

    println!(
        "built {} articles + index + listings + changelog + {} change pages + {} tombstones -> {}",
        model.articles.len(),
        changes,
        retired.len(),
        out.display()
    );
    Ok(())
}

/// Site assets: whatever the assets directory holds, verbatim. wirrel supplies
/// its own stylesheet only when the repo doesn't, so the diff colouring works
/// out of the box without ever overwriting a stylesheet the author added.
fn write_assets(src: &Path, out: &Path) -> Result<(), Box<dyn Error>> {
    let dir = out.join(routes::ASSETS_DIR);
    fs::create_dir_all(&dir)?;
    copy_tree(src, &dir)?;

    let stylesheet = dir.join("style.css");
    if !stylesheet.exists() {
        fs::write(stylesheet, html::DEFAULT_STYLESHEET)?;
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), Box<dyn Error>> {
    if !src.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&to)?;
            copy_tree(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

/// Render a page into the shell and write it at its route.
fn write_page(
    out: &Path,
    route: &str,
    title: &str,
    body: &str,
) -> Result<(), Box<dyn Error>> {
    write_route(out, route, &html::page(title, body))?;
    Ok(())
}

fn write_route(out: &Path, route: &str, html: &str) -> Result<(), Box<dyn Error>> {
    let path = routes::out_path(out, route);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, html)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wirrel::frontmatter::Frontmatter;
    use wirrel::model::Id;

    fn a(id: Id, slug: &str, title: &str) -> Article {
        Article {
            id,
            slug: slug.into(),
            path: format!("src/{slug}.md"),
            created: "2026-01-01T00:00:00Z".into(),
            updated: "2026-01-01T00:00:00Z".into(),
            past_slugs: Vec::new(),
            frontmatter: Frontmatter::default(),
            title: Some(title.into()),
        }
    }

    #[test]
    fn empty_query_keeps_every_article_in_create_order() {
        let arts = [a(1, "willow", "The Willow"), a(2, "direction", "Choosing a Direction")];
        let got: Vec<_> = ranked(&arts, "").iter().map(|x| x.id).collect();
        assert_eq!(got, [1, 2]);
    }

    #[test]
    fn query_filters_and_ranks_by_score() {
        let arts = [a(1, "willow", "The Willow"), a(2, "direction", "Choosing a Direction")];
        let got = ranked(&arts, "direc");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, 2);
    }

    #[test]
    fn titles_are_searchable_not_just_slugs() {
        let arts = [a(1, "willow", "The Willow"), a(2, "xyz", "Choosing a Direction")];
        let got = ranked(&arts, "choosing");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, 2);
    }
}
