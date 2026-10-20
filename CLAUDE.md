# Working in wirrel

## Code style
- Keep comments minimal. Prefer clear names over prose; comment only the
  non-obvious "why", not the "what".
- Keep unit tests minimal. Cover core behavior and interesting edge cases;
  don't enumerate trivial variations.
- `git.rs` and `grammars.rs` are the only I/O boundaries. `model`, `commits`,
  `markdown`, and `highlight` stay pure so they're testable with hand-built
  fixtures (a grammar crate as a dev-dependency stands in for a loaded one).
