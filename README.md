# wirrel

Tool for compiling the [gnostinomicon](https://notes.oclyke.dev) — a
body of notes managed as a git history of conventional commits.

I designed this tool to solve a big problem for myself; the fear of publishing in-progress work.

Now I can write in the moment, see how my understanding changes over time, link articles together, and never worry about organization.

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
| `/assets/…` | site assets, `wirrel.css` and `style.css` among them |
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

Every page links two stylesheets there, in order: `wirrel.css`, then the site's
`style.css`. The first is wirrel's own, written on every build. It carries only
the rules the generated HTML depends on — the `.diff` colors on commit pages and
the copy button's placement on `.copyable` code blocks — and the site's sheet,
loading after it, overrides any of them by cascade. A repo with no `style.css`
gets a small default. `wirrel.css` is reserved: a repo asset by that name is
overwritten on build, and `wirrel check` warns about one.

## Frontmatter

An article may open with a `---` fenced metadata block of flat `key: value`
lines. It is metadata, not content, so it never reaches the rendered page.

```markdown
---
title: Choosing a Direction
---
```

`title` identifies the article: the page's `<title>`, the listings, the
changelog, and `wirrel link`. Without one, the slug (from the file path) stands
in. It is not content: the article body renders exactly as written, with no
heading injected, and headings in the body are never read as the title.
Unrecognized keys are ignored.

## Code blocks

Add `copy` to a fence's info string and the block gets a copy button:

````markdown
```sh copy
cargo install wirrel
```
````

The flag is stripped before rendering, so the language stays first and GitHub
still highlights the fence. The button comes from a small script inlined at the
end of every page. It only appears where the browser exposes a clipboard —
secure origins: https, `localhost`, `127.0.0.1`, `[::1]` — and logs a console
warning on a page that has flagged blocks but no clipboard, since `[::]` or a
LAN address silently fails that test. The copied text drops the fence's
trailing newline so a pasted command doesn't run on its own.

The block renders as `<div class="copyable"><pre>…</pre></div>`, and the script
appends a `<button>` to the div once it has a clipboard — so a built page on
disk has no button in it. `wirrel.css` lays the row out with flexbox — the code scrolls, the button keeps
the far right — and draws the icon as a CSS mask over the text colour, with
`data-state="copied"` and `"failed"` swapping it briefly after a click. A custom
stylesheet themes the row as its code tray (`.copyable` for the surface,
`.copyable pre` to drop the pre's own) and may replace the icon by pointing
`.copyable button::before { mask-image: url(/assets/your.svg) }` at an asset of
its own.

## Footnotes

Footnotes use the usual syntax — `[^name]` in the text, `[^name]: …` anywhere
in the article — and render GitHub-style: numbered in order of first mention,
gathered under a rule at the end of the article in that order, each linking
back to where it was mentioned. A definition nothing mentions is dropped.

A reference renders as `<sup class="footnote-reference" id="fnref-name">` around
a link to the note (two on one word are comma-separated, so they don't read as
one number); the notes are an `<ol>` in `<section class="footnotes">`, after an
`<hr>`, each `<li id="fn-name">` ending in a `.footnote-backref` link per
reference. None of it needs `wirrel.css`; the look is the stylesheet's.

## Syntax highlighting

Fenced code is highlighted at build time with
[tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammars loaded from
the grammars directory (`--grammars`, default `./grammars`). Languages are
plugins: wirrel carries the tree-sitter runtime but no grammars, so it doesn't
grow with each one. A fence whose language no grammar covers renders plain, as
does every fence when the directory is absent.

Each grammar is a directory named for it:

```text
grammars/rust/
  parser.dylib            # parser.so on Linux, parser.dll on Windows
  queries/highlights.scm  # plus injections.scm and locals.scm, if it has them
  tree-sitter.json        # optional; its injection-regex adds aliases
```

The directory name is the grammar's name: it's what a fence matches first, and
the parser must export `tree_sitter_<name>`. To add one, from a checkout of the
grammar's repo:

```sh
tree-sitter build -o ../site/grammars/rust/parser.dylib
mkdir -p ../site/grammars/rust/queries
cp queries/{highlights,injections}.scm ../site/grammars/rust/queries/
cp tree-sitter.json ../site/grammars/rust/
```

`tree-sitter.json`'s `injection-regex` lets a grammar answer to other names —
JavaScript's `^(js|javascript)$` covers ```` ```js ```` — and also resolves
languages embedded in others, like the script in an HTML block. Use the queries
from the grammar's own repo; editor collections such as nvim-treesitter's use
predicates and capture names of their own.

The parser is native code, built for one platform: one built on macOS won't
load on a Linux CI runner. Commit the queries and `tree-sitter.json`, and build
the parser wherever the site is built. A grammar that fails to load (missing
parser, ABI too old or new for wirrel's tree-sitter, bad query) is warned about
and skipped; it never fails the build. Loading a grammar runs its code, so the
directory is as trusted as wirrel itself.

Highlighted tokens are `<span class="ts-…">`, named for tree-sitter's capture
names with dots as hyphens (`function.method` is `.ts-function-method`). The
list is fixed in `highlight.rs`; a capture not on it takes its closest listed
name, or no span. `wirrel.css` colours them for a light background; a custom
stylesheet restates the `.ts-` rules to retheme.

Once any grammar loads, `check` warns about fence languages none covers.

## Commands

- `wirrel index` — list articles (id, slug, title).
- `wirrel check` — validate history: subject grammar, lineage, slug
  collisions, commits doing only their own kind of work (a `create:` adds
  exactly one article and changes nothing; an `update:` adds none; a `move:`
  touches one article; a `meta:` changes no article), dangling `/id/N/` links,
  missing titles (no frontmatter `title`; the slug stands in),
  non-markdown files under the article root, and an `assets/wirrel.css` the
  build would overwrite.
- `wirrel link <query>` — interactive fuzzy finder; prints the `/id/N/` link.
- `wirrel build --out dist` — render the site content: article pages (each with a
  created/updated header linking its changes), the index, both article listings,
  the changelog, a change list and id-permalink stub per article, a diff page per
  commit, the assets, a tombstone at every retired slug, and a `404.html`.
- `wirrel redirects` — print the id permalink map as JSON on stdout, for a
  separate deploy tool to consume.

Global flags: `--repo <path>` (default `.`), `--articles <dir>` (default
`./articles`), `--assets <dir>` (default `./assets`), and `--grammars <dir>`
(default `./grammars`).

The path flags each read an environment variable when the flag is absent —
`WIRREL_REPO`, `WIRREL_ARTICLES`, `WIRREL_ASSETS`, `WIRREL_GRAMMARS` — so a repo whose layout
differs from the defaults can export its layout once instead of repeating flags
on every command. An explicit flag still wins over the variable.

Every URL wirrel emits is root-relative — links and the redirect map alike. The
tool never needs to know the origin it will be served from, so the site builds
the same whatever host answers for it, and a preview deploy is a working copy
rather than one pointing at production.

Pages carry no canonical tag. Each is served at a single address and links to it
root-relatively, so a self-referential canonical would only restate the URL the
crawler already followed. The id permalink stubs are the exception: theirs names
the article, so a permalink never competes with the page it stands in for.

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

- `git.rs` — I/O; shells out to `git`, produces `RawCommit` data.
- `grammars.rs` — I/O; loads tree-sitter grammars from the grammars directory.
- `commits.rs` — commit-subject grammar (`create | update | move | meta`).
- `routes.rs` — the address scheme; the only module holding URL literals.
- `model.rs` — pure fold of commits into articles; validation; id permalinks
  and retired slugs.
- `frontmatter.rs` — the leading `---` metadata block (`title`).
- `markdown.rs` — pulldown-cmark rendering; hydrates `/id/N/` links to slug URLs.
- `highlight.rs` — tree-sitter highlighting of code blocks with loaded grammars.
- `html.rs` — shared page shell / escaping helpers.
- `index_page.rs`, `articles_page.rs`, `changelog_page.rs`, `changes_page.rs`,
  `change_page.rs` — the generated index, the two article listings, the
  changelog, one article's history, and the per-commit diff page.
- `base.css` — the rules the generated pages depend on, written to
  `assets/wirrel.css` on every build.
- `default.css` — the `style.css` written when the repo supplies none.
- `copy.js` — the copy-button script inlined at the end of every page.
- `gone_page.rs` — the dead ends: retired-slug tombstone and the 404 body.

Commit convention: `<type>: <description>`, type one of create, update, move, meta.
