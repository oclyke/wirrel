# quill

Tool for compiling the [gnostinomicon](https://gnostinomicon.oclyke.dev) — a
body of notes managed as a git history of conventional commits.

Each `create:` commit assigns an article a stable numeric id (its create-order);
`update:` and `move:` commits evolve it. Because the id is a permanent handle,
links survive renames: source files reference articles by id (`id:2`, `/id/2`),
and quill renders pretty slug URLs while emitting redirects from every
historical id and slug.

## Commands

- `quill index` — list articles (id, slug, title).
- `quill check` — validate history: subject grammar, signatures, lineage, slug
  collisions, dangling `id:` links, and missing titles.
- `quill link <query>` — fuzzy-find an article and print its markdown link.
- `quill build --out dist` — render HTML pages plus a `_redirects` manifest.

Global flags: `--repo <path>` (default `.`) and `--base-url <url>`.

## Layout

- `git.rs` — the only I/O; shells out to `git`, produces `RawCommit` data.
- `commits.rs` — commit-subject grammar (`create | update | move | meta`).
- `model.rs` — pure fold of commits into articles; validation; redirect planning.
- `markdown.rs` — pulldown-cmark rendering; hydrates `id:` links to slug URLs.

Commit convention: `<type>: <description>`, type one of create, update, move, meta.
