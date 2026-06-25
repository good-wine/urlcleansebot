#![deny(rustdoc::all)]
#![deny(unused_crate_dependencies)]

// Suppress false positives from unused_crate_dependencies lint.
// These crates are used via macro expansion or transitive paths not
// detected by the lint (dptree in handlers.rs, chrono for sqlx, futures for teloxide).
use chrono as _;
use dptree as _;
use futures as _;

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
