//! A wasm32-wasip2 command component exercising the real transport.
//!
//! Built and run by the `wasm_smoke` integration test under
//! `wasmtime run -S http -S inherit-network`, which supplies the `wasi:http`
//! the guest's `WakiTransport` calls. Usage: `wasm-smoke <rpc-url> [pubkey]`.
//! Each successful call prints a line; the final `SMOKE_OK` marks the end.

use solana_client_wasip2::{pubkey::Pubkey, RpcClient};

fn main() {
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("usage: wasm-smoke <rpc-url> [pubkey]");

    let client = RpcClient::new(url);

    let version = client.get_version().expect("getVersion");
    println!("version={}", version.solana_core);

    let blockhash = client.get_latest_blockhash().expect("getLatestBlockhash");
    println!("blockhash={blockhash}");

    if let Some(pk) = args.next() {
        let pubkey: Pubkey = pk.parse().expect("parse pubkey");
        let balance = client.get_balance(&pubkey).expect("getBalance");
        println!("balance={balance}");
    }

    println!("SMOKE_OK");
}
