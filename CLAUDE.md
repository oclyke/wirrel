# Working in quill

## Code style
- Keep comments minimal. Prefer clear names over prose; comment only the
  non-obvious "why", not the "what".
- Keep unit tests minimal. Cover core behavior and interesting edge cases;
  don't enumerate trivial variations.
- `git.rs` is the only I/O boundary. `model`, `commits`, and `markdown` stay
  pure so they're testable with hand-built fixtures.
