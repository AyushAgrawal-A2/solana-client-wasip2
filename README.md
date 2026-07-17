# solana-client-wasip2

A Solana RPC client for **`wasm32-wasip2`** components: JSON-RPC over
`wasi:http` (via [`waki`](https://crates.io/crates/waki)), building on the
official Anza primitive crates rather than forking them.

> **Status:** early development — the `0.0.x` releases are placeholders while
> the client is built out. APIs will change.

## Why this exists

The existing WASM Solana clients (`solana-client-wasm`, `wasm_client_solana`)
target `wasm32-unknown-unknown` (the browser, wasm-bindgen). None target the
**WASI Preview 2 component model** (`wasm32-wasip2`), where HTTP is the typed
`wasi:http` interface. This crate fills that gap.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Not affiliated with Anza or Solana Labs.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
