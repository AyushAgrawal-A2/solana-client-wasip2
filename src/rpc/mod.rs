//! The RPC layer, split into four parts:
//!
//! - [`client`] — `RpcClient`, the JSON-RPC engine that frames requests and
//!   extracts results. It is generic over a transport and knows nothing about
//!   individual Solana methods.
//! - [`methods`] — inherent methods on `RpcClient`, one per JSON-RPC method,
//!   layered on top of the engine. This is where new endpoints are added.
//! - [`config`] — request/config types (re-exported from Anza's crates).
//! - [`response`] — response payload types (re-exported from Anza's crates).
//!
//! `methods` adds only inherent `impl` blocks, so it has nothing to re-export;
//! bringing `RpcClient` into scope makes every method available.

pub mod client;
pub mod config;
pub mod methods;
pub mod response;

pub use client::*;
pub use config::*;
pub use response::*;
