# wirrel

Tool for compiling the [gnostinomicon](https://gnostinomicon.oclyke.dev) — a
body of notes managed as a git history of conventional commits.

Each `create:` commit assigns an article a stable numeric id (its create-order);
`update:` and `move:` commits evolve it. An article is exactly one markdown file,
so a `create:` commit adds exactly one. Because the id is a permanent handle,
links survive renames: source files reference articles by `/id/N/`, `build`
renders pretty slug URLs, and `redirects` emits the id permalink map. Slugs are
not permalinks — an address an article has moved away from gets a tombstone page
saying so, without forwarding. Articles nest — `parent/index.md` is
`/article/parent/`, `parent/other.md` is `/article/parent/other/`.

## Addresses

| address | what |
| --- | --- |
| `/` | the index: recent changes, and the way in to the rest |
| `/article/<slug>/` | an article, or a tombstone at a slug one moved away from |
| `/articles/by-path/` | every article, slug order |
| `/articles/by-id/` | every article, id order |
| `/articles/by-id/<n>/` | the id permalink; a stub pointing at the article |
| `/changelog/` | every create, update and move |
| `/changes/by-id/<n>/` | one article's history |
| `/changes/by-sha/<sha>/` | one commit: its description and diff |
| `/assets/…` | site assets, `style.css` among them |
| `/404.html` | everything else |

A free-form slug appears in exactly one place — the tail of `/article/` — and
nothing is generated beneath it. Every other address is keyed by an id, a sha,
or a fixed word, so no article can contest a generated one: a `changelog.md`
routes to `/article/changelog/`, never `/changelog/`. That leaves no reserved
names for an author to avoid.

The scheme carries no version segment. Changing it again would break inbound
links rather than run two schemes side by side — a deliberate trade, taken
because `routes.rs` holds every URL the site emits, so a re-cut is one file.

`build` writes a stub page at each id permalink, so ids resolve on a host that
can't be told about redirects; `redirects` emits the same mapping for hosts that
can serve a real one.

An article's changes page is keyed by id rather than hung under the article, and
has to be: `/article/<slug>/changes/` would be ambiguous with an article whose
slug is `<slug>/changes`, and articles nest.

## Assets

The assets directory (`--assets`, default `./assets`) is copied verbatim into
the site, so `style.css` there becomes `/assets/style.css`. Every page links that
address — root-relative, so it resolves the same from an article nested at any
depth. Where the files come from is a layout question and where they land is an
addressing one, so `--assets ./static` still publishes to `/assets/`, exactly as
`--articles` never appears in an article's URL.

The default layout keeps assets out of the article root entirely. Point the two
at overlapping directories and the assets are still exempt from the non-markdown
check — that check is about stray files among the articles.

With no `assets/style.css` of its own, the repo gets wirrel's, which carries the
`.diff` rules that colorize commit pages. Adding one replaces that file entirely,
so carry those rules over to keep diffs readable.

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
  collisions, commits doing only their own kind of work (a `create:` adds
  exactly one article and changes nothing; an `update:` adds none; a `move:`
  touches one article; a `meta:` changes no article), dangling `/id/N/` links,
  missing titles (no frontmatter `title` and no level-1 heading), absolute
  self-links, and non-markdown files under the article root.
- `wirrel link <query>` — interactive fuzzy finder; prints the `/id/N/` link.
- `wirrel build --out dist` — render the site content: article pages (each with a
  created/updated header linking its changes), the index, both article listings,
  the changelog, a change list and id-permalink stub per article, a diff page per
  commit, the assets, a tombstone at every retired slug, and a `404.html`.
- `wirrel redirects` — print the id permalink map as JSON on stdout, for a
  separate deploy tool to consume.

Global flags: `--repo <path>` (or `WIRREL_REPO`, default `.`), `--base-url <url>`,
`--articles <dir>` (default `./articles`), `--assets <dir>` (default
`./assets`), and `--no-verify-signatures`.

## Retired slugs

A `move:` commit changes an article's slug. The old address is not redirected:
`build` writes a tombstone there saying the article has moved, and nothing more.
Deliberately — a slug can change hands, and two articles can each have passed
through the same one, so a forwarding address would have to pick a winner. The
id permalink is the handle that always resolves.

A retired slug that a live article has since reclaimed gets no tombstone; the
live page owns that address.

The changelog and an article's changes page do name the slugs a `move:` went
between, as plain text — never as links. The record is history; a slug an article
has left is not an address, and it may since have changed hands.

## Layout

- `git.rs` — the only I/O; shells out to `git`, produces `RawCommit` data.
- `commits.rs` — commit-subject grammar (`create | update | move | meta`).
- `routes.rs` — the address scheme; the only module holding URL literals.
- `model.rs` — pure fold of commits into articles; validation; id permalinks
  and retired slugs.
- `frontmatter.rs` — the leading `---` metadata block (`title`).
- `markdown.rs` — pulldown-cmark rendering; hydrates `/id/N/` links to slug URLs.
- `html.rs` — shared page shell / escaping helpers.
- `index_page.rs`, `articles_page.rs`, `changelog_page.rs`, `changes_page.rs`,
  `change_page.rs` — the generated index, the two article listings, the
  changelog, one article's history, and the per-commit diff page.
- `default.css` — the stylesheet used when the repo supplies none.
- `gone_page.rs` — the dead ends: retired-slug tombstone and the 404 body.

Commit convention: `<type>: <description>`, type one of create, update, move, meta.
