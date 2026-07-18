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

- **Full HTTP method surface** — the 50 HTTP JSON-RPC methods in Anza's
  `RpcRequest` enum are all implemented (the remaining variants — internal
  validator RPCs like `registerNode`/`signVote`, long-removed archiver/storage
  RPCs, and the non-endpoint `Custom` escape hatch — are explicitly skipped; see
  `tests/rpc_coverage.rs`), each with `_with_config` / `_with_commitment`
  variants where the node accepts one.
- **Official types, not hand-rolled** — request/response shapes are re-exported
  from Anza's pinned crates, so they can't silently drift from the real API.
- **Pluggable transport** — a small `RpcTransport` trait separates the JSON-RPC
  engine from HTTP, so the whole client runs on the host: `MockTransport` for
  hermetic unit tests (no network, no wasm toolchain), or a real HTTP shim
  against a live validator in the integration tests.
- **Upstream-change guard** — a test diffs the implemented methods against
  Anza's canonical `RpcRequest` enum; a new or renamed upstream method fails the
  build until it's handled.
- **Lean, RPC-only surface** — transaction *building* is out of scope; add the
  upstream SDK crates you need directly (they compile for wasip2). The types the
  RPC methods speak (`Pubkey`, `Hash`, `Signature`, `VersionedTransaction`,
  `VersionedMessage`, `CommitmentConfig`) are re-exported so you can name them
  without version-matching.
- **No async runtime** — no `tokio` in the guest. The RPC client never signs;
  signing is opt-in, with keys supplied and guarded by the caller.
- **Production-grade** — automatic retry with exponential backoff on rate-limits
  (429) and 5xx, a configurable connect timeout, a client-level default
  commitment, `send_and_confirm_transaction` + confirmation polling, and
  structured RPC errors that surface preflight logs.

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
use solana_client_wasip2::{RpcClient, pubkey::Pubkey};
use std::str::FromStr;

// The RPC URL (and any API key) is the caller's — read it from plugin config,
// never hard-code it. `new` supplies the wasi:http transport, matching the
// official client's constructor.
let client = RpcClient::new(rpc_url);

let blockhash = client.get_latest_blockhash()?;
let pubkey = Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM")?;
let lamports = client.get_balance(&pubkey)?;
```

On the host (tests, tooling), swap in `MockTransport` — same client, no network.
It lives behind the `test` feature (`solana-client-wasip2 = { version = "…",
features = ["test"] }`, typically a dev-dependency):

```rust
use solana_client_wasip2::{RpcClient, MockTransport};

let mock = MockTransport::success(
    r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":42}}"#,
);
let client = RpcClient::new_with_transport("http://localhost:8899", mock);
assert_eq!(client.get_balance(&pubkey)?, 42);
```

Response types are Anza's own (`UiAccount`, `RpcBlockhash`, `UiTokenAmount`,
`EncodedConfirmedTransactionWithStatusMeta`, …), re-exported at the crate root.

### Building and submitting a transaction

Building and signing transactions is **out of scope** — this is an RPC client. The
upstream SDK crates do that and compile for `wasm32-wasip2`, so add the ones you
need directly (they are ordinary dependencies you own and version):

```toml
solana-system-interface = { version = "3", features = ["bincode"] } # transfer, etc.
solana-keypair = "3"                                                 # signing keys
solana-signer = "3"
# spl-token-interface, spl-associated-token-account-interface, … as needed
```

The `message` and `transaction` types the RPC methods speak are re-exported by
this crate, so they match versions with no effort. `send_transaction` takes the
typed transaction and serializes it internally — no `bincode` on your side.

```rust
use solana_client_wasip2::{
    RpcClient,
    message::{v0, VersionedMessage},
    transaction::versioned::VersionedTransaction,
};
use solana_system_interface::instruction as system_instruction;
use solana_signer::Signer;

let client = RpcClient::new(rpc_url);
let blockhash = client.get_latest_blockhash()?; // -> Hash, like the official client

let ix = system_instruction::transfer(&payer.pubkey(), &recipient, 1_000_000);
let msg = VersionedMessage::V0(
    v0::Message::try_compile(&payer.pubkey(), &[ix], &[], blockhash)?,
);

// Sign with a caller-held key. `send_transaction` takes the typed transaction
// (like the official client) and serializes it internally.
let tx = VersionedTransaction::try_new(msg, &[&payer])?;
let sig = client.send_transaction(&tx)?;
```

## Architecture

```text
caller ── RpcClient::get_balance(..)        rpc::methods   one fn per JSON-RPC method
           │
           ▼
         RpcClient dispatch + retry           rpc::client    the JSON-RPC engine
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
| `test`  |    | Exposes `MockTransport` for host testing. Off by default so a normal build never ships it; enable it to test code that drives this client. |

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

# Tests — the full suite, including the live-validator integration tests
cargo test

# Lints
cargo clippy --all-targets

# API documentation
cargo doc --no-deps --open
```

`cargo test` runs **everything** in one command. Most of it is hermetic (host
only, no network, mocked RPC); the two live-validator tests below additionally
require **`solana-test-validator` on `PATH`** (and the wasm smoke test needs
**`wasmtime`**, else it skips). The layers:

- `src/rpc/client.rs` — engine internals: `result` / `error` split, retry/backoff,
  structured error data.
- `tests/methods.rs` — response parsing + `Option`/parse paths; doubles as an
  upstream-shape drift guard.
- `tests/requests.rs` — request construction (method names, default encodings,
  optional config, id sequencing).
- `tests/lifecycle.rs` — confirmation polling, typed submit, default commitment.
- `tests/tx.rs` — transaction construction with the upstream SDK crates (dev-only),
  proving they interoperate with the re-exported `message`/`transaction` types.
- `tests/rpc_coverage.rs` — method-coverage guard vs Anza's `RpcRequest`.
- `tests/integration_methods.rs` — **live**: spawns a throwaway
  `solana-test-validator`, runs every method through a host HTTP transport shim,
  and compares results to `solana-rpc-client` (the official native client, a
  dev-dependency). Stable methods are diffed for exact structural equality;
  volatile ones for a shared invariant. Real state is set up on-chain (airdrop, a
  transfer, an SPL mint + ATA + minted supply) so account/token/transaction
  methods return live data. Both clients parse into the *same* upstream types, so
  the diff is exact — this is what caught the token-account default-encoding bug.
- `tests/wasm_smoke.rs` — **live**: compiles the client to a `wasm32-wasip2`
  component (the `wasm-smoke/` crate) and runs it under
  `wasmtime run -S http -S inherit-network`, so the **real `WakiTransport`** makes
  actual `wasi:http` calls to the validator — the one path the host shim cannot
  cover.

The native client, `ureq`, and the validator harness are **dev-dependencies**,
so they are compiled only by `cargo test` — `cargo build --target wasm32-wasip2`
never pulls them (the native client does not build for wasip2, which is the whole
reason this crate exists).

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
