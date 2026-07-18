# solana-client-wasip2

A Solana JSON-RPC client that compiles to **WebAssembly components**
(`wasm32-wasip2`).

The official [`solana-client`](https://crates.io/crates/solana-client) does not
build for that target — it pulls in `reqwest`/`tokio` and other host-only
machinery. This crate fills that gap: it speaks JSON-RPC over the host's
`wasi:http` (via [`waki`](https://crates.io/crates/waki), so TLS and the socket
live host-side), and returns Anza's **official** RPC types so what you parse is
exactly what a real node speaks.

It was written for [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) tool
plugins, but it is a plain library — anything targeting `wasm32-wasip2` can use
it.

## At a glance

- **Full HTTP method surface** — all 52 public JSON-RPC methods, as 66 typed
  functions (each with `_with_config` / `_with_commitment` variants where the
  node accepts one).
- **Official types, not hand-rolled** — request/response shapes are re-exported
  from Anza's pinned crates, so they can't silently drift from the real API.
- **Pluggable transport** — a small `RpcTransport` trait separates the JSON-RPC
  engine from HTTP, so the whole client runs on the host under `cargo test` with
  **no network and no wasm toolchain**.
- **Upstream-change guard** — a test diffs the implemented methods against
  Anza's canonical `RpcRequest` enum; a new or renamed upstream method fails the
  build until it's handled.
- **Read-only, no async runtime** — the client is a query surface and never
  signs, so there is no keypair or hot-wallet risk in the plugin, and no `tokio`
  is linked into the guest.

## Install

```sh
cargo add solana-client-wasip2
```

Requires **Rust 1.89+**. Building for wasm needs the target installed (no
`wasi-sdk` or C toolchain — every dependency is pure Rust):

```sh
rustup target add wasm32-wasip2
```

## Usage

Inside a `wasm32-wasip2` component, use the real transport:

```rust
use solana_client_wasip2::{RpcClient, WakiTransport};
use solana_pubkey::Pubkey;
use std::str::FromStr;

// The RPC URL (and any API key) is the caller's — read it from plugin config,
// never hard-code it.
let client = RpcClient::new(rpc_url, WakiTransport);

let blockhash = client.get_latest_blockhash()?;
let pubkey = Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM")?;
let lamports = client.get_balance(&pubkey)?;
```

On the host (tests, tooling), swap in `MockTransport` — same client, no network:

```rust
use solana_client_wasip2::{RpcClient, MockTransport};

let mock = MockTransport::success(
    r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":42}}"#,
);
let client = RpcClient::new("http://localhost:8899", mock);
assert_eq!(client.get_balance(&pubkey)?, 42);
```

Response types are Anza's own (`UiAccount`, `RpcBlockhash`, `UiTokenAmount`,
`EncodedConfirmedTransactionWithStatusMeta`, …), re-exported at the crate root.

## Architecture

```text
caller ── RpcClient::get_balance(..)        rpc::methods   one fn per JSON-RPC method
           │
           ▼
         RpcClient::call_typed(..)          rpc::client    the JSON-RPC engine
           │                                               (framing, ids, result/error, decode)
           ▼
         RpcTransport::post(url, body)      transport      the swappable seam
           ├─ WakiTransport  → wasi:http    (wasm only)
           └─ MockTransport  → canned reply (host tests)
```

| Path | Responsibility |
|------|----------------|
| `src/transport.rs` | `RpcTransport` trait + `WakiTransport` (wasm) / `MockTransport` (test) |
| `src/rpc/client.rs` | `RpcClient` — transport-agnostic JSON-RPC engine |
| `src/rpc/methods.rs` | the typed method surface (add new endpoints here) |
| `src/rpc/config.rs` | request/config types (re-exported from Anza crates) |
| `src/rpc/response.rs` | response payload types (re-exported from Anza crates) |
| `src/error.rs` | the `Error` taxonomy |
| `tests/rpc_coverage.rs` | guards the method surface against upstream `RpcRequest` |

The upstream type crates are gated behind `agave-unstable-api` and **pinned to
exact versions** (`=x.y.z`) in `Cargo.toml`. Updates are deliberate: bump the
pin, rebuild, and fix whatever the compiler and tests flag.

## Feature flags

| Feature | Default | Effect |
|---------|:-------:|--------|
| `curve` | ✅ | Enables `solana-pubkey` ed25519 support (on-curve checks / PDA derivation). |
| `test`  | ✅ | Provides `MockTransport` for host testing. |

A component that wants the smallest possible artifact can opt out:

```toml
solana-client-wasip2 = { git = "...", default-features = false }
```

## Build, test, docs

```sh
# Host build
cargo build

# WebAssembly component (release)
cargo build --target wasm32-wasip2 --release

# Tests — run entirely on the host, no network (mocked RPC)
cargo test

# Lints
cargo clippy --all-targets

# API documentation
cargo doc --no-deps --open
```

The test suite (36 tests) is layered:

- `src/rpc/client.rs` — engine internals (`result` / `error` / malformed body).
- `tests/methods.rs` — response parsing + `Option`/parse paths; doubles as an
  upstream-shape drift guard.
- `tests/requests.rs` — request construction (method names, default encodings,
  optional config, id sequencing).
- `tests/rpc_coverage.rs` — method-coverage guard vs Anza's `RpcRequest`.

## Contributing

To add a method: add the wrapper in `src/rpc/methods.rs`, then update
`tests/rpc_coverage.rs` — it will not compile until the new (or renamed) method
is implemented or explicitly skipped, which keeps the surface honest.

## Status

Pre-1.0 (`0.0.0`), tracking experimental upstreams (`wit/v0`,
`agave-unstable-api`); expect breaking changes on version bumps.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
