//! # solana-client-wasip2
//!
//! A Solana RPC client for `wasm32-wasip2` components — JSON-RPC over
//! `wasi:http` via [`waki`], reusing the official Anza primitive crates
//! (`solana-pubkey`, `solana-instruction`, `solana-message`,
//! `solana-transaction`, `solana-hash`).
//!
//! **Status: early development.** The `0.0.x` line is a placeholder while the
//! API is built out. Not affiliated with Anza or Solana Labs.

/// Crate version, exposed so the stub does something real.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
