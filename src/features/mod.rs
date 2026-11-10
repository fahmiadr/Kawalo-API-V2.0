//! Feature modules. Each exposes `router()` mounted under `/api/v1`, except `ptt`, whose
//! `ws_router()` lives at the root.

pub mod attendance;
pub mod auth;
pub mod group;
pub mod guest;
pub mod incident;
pub mod information;
pub mod operational;
pub mod ptt;

mod batch;
mod period_query;

#[cfg(test)]
mod typebox_corpus;
