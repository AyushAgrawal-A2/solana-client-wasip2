//! # solana-client-wasip2
//!
//! A Solana JSON-RPC client that compiles to **WebAssembly components**
//! (`wasm32-wasip2`). The official `solana-client` does not build for that
//! target — it depends on `reqwest`/`tokio` and other host-only machinery — so
//! this crate provides the read/query surface a sandboxed component needs while
//! staying inside what the component model allows.
//!
//! It was written for [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)
//! tool plugins, where TLS and the socket are provided by the host's
//! `wasi:http` and the guest only speaks HTTP.
//!
//! ## How it fits together
//!
//! ```text
//! caller ── RpcClient::get_balance(..)            (rpc::methods)
//!            │  builds params, picks a method name
//!            ▼
//!          RpcClient::call_typed(..)             (rpc::client)  ← the engine
//!            │  frames the JSON-RPC request, sequences ids,
//!            │  extracts `result` / surfaces `error`, deserializes
//!            ▼
//!          RpcTransport::post(url, body)         (transport)    ← the seam
//!            ├─ WakiTransport   → wasi:http      (only on wasm targets)
//!            └─ MockTransport   → canned reply   (host tests)
//! ```
//!
//! ## Module map
//!
//! - [`transport`] — the [`RpcTransport`] trait and its two implementations.
//!   Making the transport a trait is what lets the whole client be exercised on
//!   the host with no network and no wasm toolchain.
//! - [`rpc::client`] — [`RpcClient`], the transport-agnostic JSON-RPC engine.
//! - [`rpc::methods`] — the full official-`RpcClient` method surface, each a
//!   thin wrapper over the engine.
//! - [`rpc::config`] / [`rpc::response`] — request and response types. These are
//!   **re-exported from Anza's official crates** (pinned in `Cargo.toml`), not
//!   hand-rolled, so the shapes are exactly what a real node speaks and any
//!   upstream change surfaces at a deliberate version bump rather than silently.
//! - [`error`] — the [`Error`] taxonomy shared by every layer.
//!
//! A separate `tests/rpc_coverage.rs` diffs our method list against Anza's
//! canonical `RpcRequest` enum, so a newly-added upstream method fails the build
//! until it is implemented or explicitly skipped.

pub mod error;
pub mod rpc;
pub mod transport;

// `CommitmentConfig`/`CommitmentLevel` are used across the config and method
// layers. They come from Anza's dedicated crate (serde + serde_derive only, no
// heavy transitive deps) and are surfaced at the crate root so callers write
// `solana_client_wasip2::CommitmentConfig`.
pub use solana_commitment_config::{CommitmentConfig, CommitmentLevel};

// Flatten the module tree for callers: every public item is reachable directly
// from the crate root (e.g. `solana_client_wasip2::RpcClient`).
pub use error::*;
pub use rpc::*;
pub use transport::*;
