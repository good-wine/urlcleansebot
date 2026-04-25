//! Frontend redirect lookup module.
//!
//! Given a user-supplied URL, this module returns one or more privacy-friendly
//! alternative frontends sourced from the public LibRedirect and Farside
//! catalogues. Both sources are fetched lazily, parsed once, and cached in
//! memory with a TTL.
//!
//! # Public API
//!
//! * [`RedirectService`] — the orchestrator. Construct with [`RedirectService::new`]
//!   and inject as a Telegram dispatcher dependency.
//! * [`format_hit_html`] — renders a [`LookupHit`] into a Telegram HTML message.
//!
//! Most callers only need the two items above; the lower-level types are
//! re-exported for testing and advanced use.

mod cache;
mod models;
mod service;

pub use models::{Frontend, FrontendSource, LookupHit};
pub use service::{format_hit_html, extract_host, RedirectService, FARSIDE_URL, LIBREDIRECT_URL};

#[cfg(test)]
pub use cache::SingleEntryCache;
