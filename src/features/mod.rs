//! Feature modules. Each exposes `router()` mounted under `/api/v1`, except `ptt`, whose
//! `ws_router()` lives at the root, as does the public file router of `uploads`.

pub mod attendance;
pub mod auth;
pub mod group;
pub mod guest;
pub mod incident;
pub mod information;
pub mod operational;
pub mod ptt;
pub mod schedule;
pub mod uploads;

mod batch;
mod period_query;

#[cfg(test)]
mod typebox_corpus;
