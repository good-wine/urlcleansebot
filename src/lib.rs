#![deny(rustdoc::all)]
#![deny(unused_crate_dependencies)]

// Crates used via macro expansion, trait resolution, or runtime code path
// not detected by the lint — keep as `use Foo as _` to suppress.
use chrono as _;       // sqlx type conversion
use dptree as _;       // teloxide dependency injection
use futures as _;      // teloxide stream processing
use opentelemetry as _;       // logging.rs tracer provider
use opentelemetry_otlp as _; // logging.rs OTLP exporter
use opentelemetry_sdk as _;  // logging.rs SDK types
use tracing_opentelemetry as _; // logging.rs OpenTelemetryLayer
use uuid as _;               // request IDs (future use)

// Dev-dependency crates — only available when compiling with cfg(test)
#[cfg(test)]
use criterion as _;    // benchmark suite
#[cfg(test)]
use wiremock as _;     // integration tests
#[cfg(test)]
use testcontainers as _; // integration tests

pub mod presentation;
pub mod shared;

pub mod config;
pub mod constants;
pub mod db;
pub mod http_utils;
pub mod i18n;
pub mod logging;
pub mod metrics;
pub mod redirects;
pub mod sanitizer;
