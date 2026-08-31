# wirrel

Tool for compiling the [gnostinomicon](https://gnostinomicon.oclyke.dev) — a
body of notes managed as a git history of conventional commits.

Each `create:` commit assigns an article a stable numeric id (its create-order);
`update:` and `move:` commits evolve it. Because the id is a permanent handle,
links survive renames: source files reference articles by `/id/N/`, `build`
renders pretty slug URLs, and `redirects` emits a map from every historical id
and slug to its current URL. Articles nest — `parent/index.md` is `/parent/`,
`parent/other.md` is `/parent/other/`.

## Commands

- `wirrel index` — list articles (id, slug, title).
- `wirrel check` — validate history: subject grammar, signatures, lineage, slug
  collisions, single-article moves, meta commits not changing articles, dangling
  `/id/N/` links, missing titles, absolute self-links, and non-markdown files
  under the article root.
- `wirrel link <query>` — interactive fuzzy finder; prints the `/id/N/` link.
- `wirrel build --out dist` — render the site content: article pages (each with a
  created/updated header), a generated index (with recent changes), a full
  update-history page, and a diff page per `update:` commit.
- `wirrel redirects` — print the redirect map (id permalinks + historical slugs)
  as JSON on stdout, for a separate deploy tool to consume.

Global flags: `--repo <path>` (or `wirrel_REPO`, default `.`), `--base-url <url>`,
`--article-root <dir>` (default `.`), and `--no-verify-signatures`.

## Layout

- `git.rs` — the only I/O; shells out to `git`, produces `RawCommit` data.
- `commits.rs` — commit-subject grammar (`create | update | move | meta`).
- `model.rs` — pure fold of commits into articles; validation; redirect planning.
- `markdown.rs` — pulldown-cmark rendering; hydrates `/id/N/` links to slug URLs.
- `html.rs` — shared page shell / escaping helpers.
- `index_page.rs`, `history_page.rs`, `update_page.rs` — generated index,
  update-history, and per-update diff pages.

Commit convention: `<type>: <description>`, type one of create, update, move, meta.
