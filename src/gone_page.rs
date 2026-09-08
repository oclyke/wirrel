//! The two dead ends: a tombstone at a slug an article has moved away from,
//! and the catch-all for addresses that never held anything.

use crate::routes;

/// Served at a retired slug. Deliberately silent about the destination — slug
/// history is not a forwarding address.
pub fn moved_body() -> String {
    format!(
        "<h1>this article has moved</h1>\n\
         <p>An article lived at this address and has since moved on. Where it went \
         isn't recorded here; look for it from the <a href=\"{}\">index</a>.</p>\n",
        routes::index()
    )
}

/// Served for everything else.
pub fn missing_body() -> String {
    format!(
        "<h1>no such article</h1>\n\
         <p>Nothing has been published at this address. Try the \
         <a href=\"{}\">index</a>.</p>\n",
        routes::index()
    )
}
