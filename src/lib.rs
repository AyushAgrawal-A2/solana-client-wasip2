//! # solana-client-wasip2
//!
//! A Solana **JSON-RPC client** for **WebAssembly components** (`wasm32-wasip2`).
//! The official `solana-client` / `solana-sdk` stack does not build for that
//! target — it pulls in `reqwest`/`tokio` and other host-only machinery. This
//! crate provides the piece with no upstream equivalent there: a JSON-RPC client
//! that speaks over the host's `wasi:http`, returning Anza's official RPC types.
//!
//! It was written for [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)
//! tool plugins, where TLS and the socket are provided by the host's
//! `wasi:http` and the guest only speaks HTTP.
//!
//! ## What it does
//!
//! - **Query** the chain — the full JSON-RPC method surface ([`rpc`]).
//! - **Submit & confirm** — [`send_and_confirm_transaction`] and the
//!   confirmation pollers, with automatic retry/backoff on transient failures,
//!   a configurable transport timeout, and a client-level default commitment.
//!
//! Building and signing transactions is out of scope — see below. This crate
//! never signs.
//!
//! [`send_and_confirm_transaction`]: rpc::client::RpcClient::send_and_confirm_transaction
//!
//! ## How the RPC client fits together
//!
//! ```text
//! caller ── RpcClient::get_balance(..)            (rpc::methods)
//!            │  builds params, picks a method name
//!            ▼
//!          RpcClient dispatch + retry              (rpc::client)  ← the engine
//!            │  frames the JSON-RPC request, sequences ids,
//!            │  extracts `result` / surfaces `error`, deserializes
//!            ▼
//!          RpcTransport::post(url, body)         (transport)    ← the seam
//!            ├─ WakiTransport   → wasi:http      (only on wasm targets)
//!            └─ MockTransport   → canned reply   (host tests)
//! ```
//!
//! ## Building a transaction
//!
//! This crate is an RPC client; it does not build or sign transactions. The
//! upstream SDK crates do that, and they compile for `wasm32-wasip2`, so add the
//! ones you need directly (they are ordinary dependencies you own and version):
//!
//! ```toml
//! solana-system-interface = { version = "3", features = ["bincode"] } # transfer, etc.
//! solana-keypair = "3"                                                  # signing keys
//! solana-signer = "3"
//! spl-token-interface = "3"                                             # SPL builders
//! ```
//!
//! Compile a v0 [`message`], sign into a
//! [`VersionedTransaction`](transaction::versioned::VersionedTransaction), and
//! hand it to [`RpcClient::send_transaction`] — which takes the typed
//! transaction and serializes it internally, so the caller needs no `bincode`.
//! Building an *unsigned* transaction (to propose for external signing) needs no
//! key: construct a `VersionedTransaction` with default signature placeholders.
//! The `message` and `transaction` types the RPC methods speak are re-exported
//! here (see below) so they match versions with no effort.
//!
//! ## Module map
//!
//! The RPC client (this crate's own code):
//! - [`transport`] — the [`RpcTransport`] trait and its two implementations.
//! - [`rpc::client`] — [`RpcClient`], the transport-agnostic JSON-RPC engine.
//! - [`rpc::methods`] — the JSON-RPC method surface, thin wrappers over the engine.
//! - [`rpc::config`] / [`rpc::response`] — request and response types.
//! - [`error`] — the [`Error`] taxonomy.
//!
//! The upstream types that appear in method signatures, re-exported under their
//! conventional names so callers can name them without version-matching:
//! [`pubkey`], [`hash`], [`signature`], [`message`], [`transaction`],
//! [`commitment_config`].
//!
//! A separate `tests/rpc_coverage.rs` diffs our method list against Anza's
//! canonical `RpcRequest` enum, so a newly-added upstream method fails the build
//! until it is implemented or explicitly skipped.

pub mod error;
pub mod rpc;
pub mod transport;

// ---- Upstream types that appear in this client's method signatures, re-exported
// under their conventional ecosystem names so a caller can name what the RPC
// methods accept and return — a `Pubkey` argument, a returned `Hash`/`Signature`,
// a `VersionedTransaction`/`VersionedMessage` to submit — without matching the
// exact upstream release themselves. Verbatim re-exports: no wrappers, no renamed
// types. Transaction-building crates are not re-exported; add those you need
// directly (see the crate docs).

/// `solana-pubkey` — addresses and PDA derivation.
pub mod pubkey {
    pub use solana_pubkey::*;
}
/// `solana-hash` — 32-byte hashes / blockhashes.
pub mod hash {
    pub use solana_hash::*;
}
/// `solana-signature` — transaction signatures.
pub mod signature {
    pub use solana_signature::*;
}
/// `solana-message` — legacy and v0 messages (the input to `get_fee_for_message`).
pub mod message {
    pub use solana_message::*;
}
/// `solana-transaction` — legacy and versioned transactions (the input to
/// `send_transaction` / `simulate_transaction`).
pub mod transaction {
    pub use solana_transaction::*;
}
/// `solana-commitment-config` — commitment levels.
pub mod commitment_config {
    pub use solana_commitment_config::*;
}

// `CommitmentConfig` / `CommitmentLevel` appear in RPC method signatures, so they
// are also surfaced at the crate root for convenience.
pub use solana_commitment_config::{CommitmentConfig, CommitmentLevel};

// Flatten the RPC layer for callers: `solana_client_wasip2::RpcClient` etc.
pub use error::*;
pub use rpc::*;
pub use transport::*;
