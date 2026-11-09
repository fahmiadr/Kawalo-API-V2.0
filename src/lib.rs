//! Kawalo patrol operations API (Rust port of the ElysiaJS backend).
//!
//! [`build_app`] assembles the router; `core` holds the shared infrastructure and
//! `features/*` the endpoints.

mod app;
pub mod core;
pub mod features;

pub use app::build_app;
#[doc(hidden)]
pub use app::build_app_with;
