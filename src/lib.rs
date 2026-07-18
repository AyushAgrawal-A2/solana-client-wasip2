//! # solana-client-wasip2
//!
//! Unofficial Solana RPC client for `wasm32-wasip2` components — JSON-RPC over
//! `wasi:http` via [`waki`], reusing the official Anza primitive crates
//! (`solana-pubkey`, `solana-instruction`, `solana-message`,
//! `solana-transaction`, `solana-hash`).

pub mod error;
pub mod rpc;
pub mod transport;

pub use error::{Error, Result};
pub use rpc::RpcClient;
pub use transport::RpcTransport;

#[cfg(target_family = "wasm")]
pub use transport::WakiTransport;

#[cfg(feature = "test")]
pub use transport::MockTransport;
