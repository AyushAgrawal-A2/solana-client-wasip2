//! Shared harness for the live-validator integration tests
//! (`integration_methods.rs`, `wasm_smoke.rs`).
//!
//! Provides:
//!
//! - [`TestValidator`] — spawns `solana-test-validator` in a throwaway ledger
//!   directory, waits for it to report healthy, and kills it on drop.
//! - [`UreqTransport`] — a host-side [`RpcTransport`](solana_client_wasip2::RpcTransport)
//!   (blocking HTTP via `ureq`) so the crate's own `RpcClient` engine + method
//!   layer run unmodified on the host against a real node. The production
//!   `WakiTransport` is wasm-only; this shim exercises everything above the
//!   transport seam (params, wire encoding, response parsing into official types).
//! - Constructors for the client under test and the reference native client,
//!   plus [`to_value`] for structural comparison that sidesteps type identity.

// Each test binary includes this module but uses only a subset of it.
#![allow(dead_code)]

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use solana_client_wasip2::{Error, RpcClient, RpcTransport};

/// A `solana-test-validator` child process bound to the default RPC port,
/// backed by a temporary ledger that is removed on drop.
pub struct TestValidator {
    child: Child,
    _ledger: tempfile::TempDir,
    rpc_url: String,
}

impl TestValidator {
    /// Spawn a validator and block until it is healthy (or panic after 90 s).
    pub fn start() -> Self {
        // A previous validator (another test binary) may still be releasing the
        // port; wait for it to be free before spawning so the bind succeeds.
        let free_by = Instant::now() + Duration::from_secs(20);
        while std::net::TcpStream::connect(("127.0.0.1", 8899)).is_ok() {
            assert!(Instant::now() < free_by, "RPC port 8899 never freed");
            std::thread::sleep(Duration::from_millis(300));
        }

        let ledger = tempfile::tempdir().expect("create temp ledger dir");
        let child = Command::new("solana-test-validator")
            .arg("--ledger")
            .arg(ledger.path())
            .arg("--reset")
            .arg("--quiet")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn solana-test-validator — is it on PATH?");

        let rpc_url = "http://127.0.0.1:8899".to_string();
        let validator = Self {
            child,
            _ledger: ledger,
            rpc_url,
        };
        validator.wait_healthy(Duration::from_secs(90));
        validator
    }

    /// The RPC endpoint both clients point at.
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    fn wait_healthy(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"getHealth"}"#;
        while Instant::now() < deadline {
            if let Ok(resp) = ureq::post(&self.rpc_url)
                .set("content-type", "application/json")
                .send_string(body)
            {
                if let Ok(text) = resp.into_string() {
                    if text.contains("\"result\":\"ok\"") {
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        panic!("validator did not become healthy within {timeout:?}");
    }
}

impl Drop for TestValidator {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Host-side blocking HTTP transport for the client under test.
#[derive(Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .build(),
        }
    }
}

impl RpcTransport for UreqTransport {
    fn post(&self, url: &str, body: &str) -> Result<String, Error> {
        match self
            .agent
            .post(url)
            .set("content-type", "application/json")
            .send_string(body)
        {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| Error::Transport(e.to_string())),
            // JSON-RPC errors arrive as HTTP 200 with an `error` field; a non-2xx
            // status is a transport-level failure. Mirror WakiTransport and mark
            // 429/5xx retryable.
            Err(ureq::Error::Status(code, _)) => {
                Err(Error::Transport(format!("http status {code}")))
            }
            Err(e) => Err(Error::Transport(e.to_string())),
        }
    }
}

/// The client under test, driven through the host transport shim.
pub fn our_client(url: &str) -> RpcClient<UreqTransport> {
    RpcClient::new_with_transport(url, UreqTransport::default())
}

/// The reference implementation: the official native blocking client.
pub fn native_client(url: &str) -> solana_rpc_client::rpc_client::RpcClient {
    solana_rpc_client::rpc_client::RpcClient::new(url.to_string())
}

/// Serialize any result to a [`serde_json::Value`] so two clients' typed results
/// can be compared structurally without their Rust types being identical.
pub fn to_value<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).expect("serialize to json value")
}
