//! Data models for the redirect data sources.
//!
//! Two upstream JSON formats are supported:
//!
//! * **LibRedirect** (`https://raw.githubusercontent.com/libredirect/instances/main/data.json`)
//!   — a map keyed by service slug, where each service exposes one map of
//!   `network kind → list of instance URLs`.
//! * **Farside** (`https://raw.githubusercontent.com/benbusby/farside/refs/heads/main/services-full.json`)
//!   — an array of services, each with a `type` (e.g. `youtube`) and a list of
//!   instance URLs.
//!
//! Both upstream shapes are normalised to a single internal type
//! [`Frontend`] so the rest of the bot does not need to care about which
//! source provided a given alternative.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Source of a normalised [`Frontend`] entry. Useful for diagnostics and for
/// the user-facing message ("from LibRedirect" / "from Farside").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendSource {
    LibRedirect,
    Farside,
}

impl FrontendSource {
    pub fn as_str(self) -> &'static str {
        match self {
            FrontendSource::LibRedirect => "LibRedirect",
            FrontendSource::Farside => "Farside",
        }
    }
}

/// One alternative frontend instance for a service.
///
/// Built from upstream data and intended to be cheap to clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontend {
    /// Service slug (e.g. `"youtube"`, `"twitter"`).
    pub service: String,
    /// Which alternative frontend (e.g. `"invidious"`, `"piped"`). For
    /// Farside this matches `service`.
    pub kind: String,
    /// Public instance URL (already trimmed, kept verbatim from the source).
    pub url: String,
    pub source: FrontendSource,
}

/// Result of a successful lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupHit {
    /// Slug of the source service that matched (e.g. `"youtube"`).
    pub service: String,
    /// All alternative frontends discovered across upstreams.
    pub frontends: Vec<Frontend>,
}

// ---------------------------------------------------------------------------
// LibRedirect upstream shape (deserialized lazily — fields we don't need are
// flattened away with `#[serde(default)]`).
// ---------------------------------------------------------------------------

/// Raw LibRedirect document: `{ "<service>": { ... } }`.
pub(crate) type LibRedirectDoc = BTreeMap<String, LibRedirectService>;

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct LibRedirectService {
    /// `{ "<frontend kind>": { "clearnet": [ "https://..." ], ... } }`
    #[serde(default)]
    pub instances: BTreeMap<String, LibRedirectInstanceSet>,
    /// Domains the service replaces (e.g. `["youtube.com", "youtu.be"]`).
    /// Some entries omit this list, hence the default.
    #[serde(default, rename = "targets")]
    pub targets: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct LibRedirectInstanceSet {
    #[serde(default)]
    pub clearnet: Vec<String>,
    // tor / i2p / loki are intentionally ignored — not useful for
    // a Telegram redirect reply.
}

// ---------------------------------------------------------------------------
// Farside upstream shape: an array of services.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct FarsideService {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub instances: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_libredirect_minimal() {
        let raw = r#"{
            "youtube": {
                "targets": ["youtube.com", "youtu.be"],
                "instances": {
                    "invidious": { "clearnet": ["https://yewtu.be"] },
                    "piped":     { "clearnet": ["https://piped.video"] }
                }
            }
        }"#;
        let parsed: LibRedirectDoc = serde_json::from_str(raw).unwrap();
        let yt = parsed.get("youtube").expect("youtube entry");
        assert_eq!(yt.targets, vec!["youtube.com", "youtu.be"]);
        assert_eq!(yt.instances.len(), 2);
        assert_eq!(
            yt.instances["invidious"].clearnet,
            vec!["https://yewtu.be".to_string()]
        );
    }

    #[test]
    fn parse_libredirect_skips_unknown_fields() {
        // Real upstream documents include many extra keys; deserialization must
        // not fail on unknowns.
        let raw = r#"{
            "x": {
                "name": "X",
                "embeddable": true,
                "instances": { "nitter": { "clearnet": ["https://nitter.net"], "tor": ["x"] } }
            }
        }"#;
        let parsed: LibRedirectDoc = serde_json::from_str(raw).unwrap();
        assert!(parsed.contains_key("x"));
    }

    #[test]
    fn parse_farside_minimal() {
        let raw = r#"[
            { "type": "invidious", "instances": ["https://yewtu.be"], "test_url": "/watch?v=jNQXAC9IVRw" },
            { "type": "nitter",    "instances": ["https://nitter.net"] }
        ]"#;
        let parsed: Vec<FarsideService> = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].kind, "invidious");
        assert_eq!(parsed[0].instances, vec!["https://yewtu.be"]);
        assert_eq!(parsed[1].instances.len(), 1);
    }

    #[test]
    fn frontend_source_str() {
        assert_eq!(FrontendSource::LibRedirect.as_str(), "LibRedirect");
        assert_eq!(FrontendSource::Farside.as_str(), "Farside");
    }
}
