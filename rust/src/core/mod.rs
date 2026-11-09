//! Infrastructure shared by every feature. Never depends on `features`.

pub mod clean;
pub mod client_ip;
pub mod config;
pub mod constants;
pub mod db;
pub mod error;
pub mod extract;
pub mod health;
pub mod ids;
pub mod logging;
pub mod middleware;
pub mod pagination;
pub mod redis;
pub mod response;
pub mod security;
pub mod state;
