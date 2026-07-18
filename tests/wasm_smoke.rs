//! End-to-end smoke test of the **real** transport: the client is compiled to a
//! `wasm32-wasip2` component and run under `wasmtime`, so its `WakiTransport`
//! makes actual `wasi:http` calls to a live validator — the one path the
//! host-transport comparison in `methods.rs` cannot reach.
//!
//! Skipped (not failed) when `wasmtime` is not on `PATH`.

mod harness;
use harness::{native_client, TestValidator};

use solana_keypair::Keypair;
use solana_signer::Signer;

use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn wasm_component_hits_validator_over_wasi_http() {
    if Command::new("wasmtime").arg("--version").output().is_err() {
        eprintln!("skip: wasmtime not on PATH");
        return;
    }

    // Build the smoke component (its own target dir — it is excluded from the
    // host workspace).
    let smoke_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/wasm-smoke");
    let built = Command::new(env!("CARGO"))
        .args(["build", "--release", "--target", "wasm32-wasip2", "--manifest-path"])
        .arg(format!("{smoke_dir}/Cargo.toml"))
        .status()
        .expect("run cargo build for wasm-smoke");
    assert!(built.success(), "wasm-smoke failed to build");
    let wasm = format!("{smoke_dir}/target/wasm32-wasip2/release/wasm-smoke.wasm");

    // Spawn a validator and fund an account so getBalance returns non-zero.
    let validator = TestValidator::start();
    let url = validator.rpc_url();
    let funded = Keypair::new();
    let native = native_client(url);
    native
        .request_airdrop(&funded.pubkey(), 1_000_000_000)
        .expect("airdrop");
    let deadline = Instant::now() + Duration::from_secs(30);
    while native.get_balance(&funded.pubkey()).unwrap_or(0) == 0 {
        assert!(Instant::now() < deadline, "airdrop did not settle");
        std::thread::sleep(Duration::from_millis(300));
    }

    // Run the component: wasmtime provides wasi:http (-S http) and host network
    // access (-S inherit-network) that the guest's WakiTransport needs.
    let out = Command::new("wasmtime")
        .args(["run", "-S", "http=y", "-S", "inherit-network=y"])
        .arg(&wasm)
        .arg(url)
        .arg(funded.pubkey().to_string())
        .output()
        .expect("run wasmtime");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!("--- wasm-smoke stdout ---\n{stdout}\n--- stderr ---\n{stderr}");

    assert!(out.status.success(), "wasm component exited with error");
    assert!(stdout.contains("version="), "missing getVersion output");
    assert!(stdout.contains("blockhash="), "missing getLatestBlockhash output");
    assert!(stdout.contains("balance=1000000000"), "wrong/missing getBalance output");
    assert!(stdout.contains("SMOKE_OK"), "component did not finish cleanly");
}
