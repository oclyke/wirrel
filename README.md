# wirrel

Tool for compiling the [gnostinomicon](https://gnostinomicon.oclyke.dev) — a
body of notes managed as a git history of conventional commits.

Each `create:` commit assigns an article a stable numeric id (its create-order);
`update:` and `move:` commits evolve it. Because the id is a permanent handle,
links survive renames: source files reference articles by `/id/N/`, `build`
renders pretty slug URLs, and `redirects` emits the id permalink map. Slugs are
not permalinks — an address an article has moved away from gets a tombstone page
saying so, without forwarding. Articles nest — `parent/index.md` is `/parent/`,
`parent/other.md` is `/parent/other/`.

## Frontmatter

An article may open with a `---` fenced metadata block of flat `key: value`
lines. It is metadata, not content, so it never reaches the rendered page.

```markdown
---
title: Choosing a Direction
---
```

`title` overrides the first level-1 heading. With no heading in the body, the
title supplants it: the page leads with an `<h1>` built from the frontmatter.
Unrecognized keys are ignored.

## Commands

- `wirrel index` — list articles (id, slug, title).
- `wirrel check` — validate history: subject grammar, signatures, lineage, slug
  collisions, single-article moves, meta commits not changing articles, dangling
  `/id/N/` links, missing titles (no frontmatter `title` and no level-1
  heading), absolute self-links, and non-markdown files under the article root.
- `wirrel link <query>` — interactive fuzzy finder; prints the `/id/N/` link.
- `wirrel build --out dist` — render the site content: article pages (each with a
  created/updated header), a generated index (with recent changes), a full
  update-history page, a diff page per `update:` commit, a tombstone at every
  retired slug, and a `404.html`.
- `wirrel redirects` — print the id permalink map as JSON on stdout, for a
  separate deploy tool to consume.

Global flags: `--repo <path>` (or `wirrel_REPO`, default `.`), `--base-url <url>`,
`--article-root <dir>` (default `.`), and `--no-verify-signatures`.

## Retired slugs

A `move:` commit changes an article's slug. The old address is not redirected:
`build` writes a tombstone there saying the article has moved, and nothing more.
Deliberately — a slug can change hands, and two articles can each have passed
through the same one, so a forwarding address would have to pick a winner. The
id permalink is the handle that always resolves.

A retired slug that a live article has since reclaimed gets no tombstone; the
live page owns that address.

## Layout

- `git.rs` — the only I/O; shells out to `git`, produces `RawCommit` data.
- `commits.rs` — commit-subject grammar (`create | update | move | meta`).
- `model.rs` — pure fold of commits into articles; validation; id permalinks
  and retired slugs.
- `frontmatter.rs` — the leading `---` metadata block (`title`).
- `markdown.rs` — pulldown-cmark rendering; hydrates `/id/N/` links to slug URLs.
- `html.rs` — shared page shell / escaping helpers.
- `index_page.rs`, `history_page.rs`, `update_page.rs` — generated index,
  update-history, and per-update diff pages.
- `gone_page.rs` — the dead ends: retired-slug tombstone and the 404 body.

Commit convention: `<type>: <description>`, type one of create, update, move, meta.
