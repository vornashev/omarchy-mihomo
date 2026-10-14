//! Minimal async client for the mihomo external controller.
//!
//! Talks HTTP/1.1 over the unix socket configured with
//! `external-controller-unix`, which mihomo serves without a secret.
//! Streaming endpoints (`/logs`, `/traffic`, `/memory`) are read as
//! newline-delimited JSON over a plain chunked response, so no WebSocket
//! stack is needed.

mod client;
mod error;
pub mod model;

pub use client::{Client, LineStream};
pub use error::{Error, Result};
