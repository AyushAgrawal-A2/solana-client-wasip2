//! The HTTP seam between the JSON-RPC engine and the outside world.
//!
//! `RpcClient` never performs I/O directly; it hands a request body to an
//! [`RpcTransport`] and gets a response body back. That one indirection is what
//! makes the crate both wasm-native and host-testable:
//!
//! - `WakiTransport` — the real transport, compiled only for wasm targets. It
//!   speaks HTTP through the host's `wasi:http` (via the `waki` crate), so TLS
//!   happens host-side and no sockets are linked into the guest.
//! - [`MockTransport`] — a host-only fake that returns a canned reply and
//!   records outgoing requests, so the entire client can be tested with
//!   `cargo test` — no network, no wasm toolchain.

use crate::{Error, Result};

/// Sends a JSON-RPC request body to `url` and returns the response body.
///
/// Implementors own the HTTP concern entirely; the engine above them stays
/// pure. Any I/O failure should be reported as [`Error::Transport`].
pub trait RpcTransport {
    fn post(&self, url: &str, body: &str) -> Result<String>;
}

/// The production transport: blocking HTTP over the host's `wasi:http`.
///
/// Only exists on wasm targets — on the host there is no `wasi:http` to call,
/// and tests use [`MockTransport`] instead.
#[cfg(target_family = "wasm")]
pub struct WakiTransport;

#[cfg(target_family = "wasm")]
impl RpcTransport for WakiTransport {
    fn post(&self, url: &str, body: &str) -> Result<String> {
        let response = waki::Client::new()
            .post(url)
            .header("content-type", "application/json")
            .body(body.as_bytes().to_vec())
            .send()
            .map_err(|e| Error::Transport(e.to_string()))?;

        let bytes = response
            .body()
            .map_err(|e| Error::Transport(e.to_string()))?;

        String::from_utf8(bytes).map_err(|e| Error::Decode(e.to_string()))
    }
}

/// A transport for host tests: returns a canned body and records every request
/// so tests can assert what the client actually sent. Cloning shares the same
/// request log (via `Rc`), so clone one before moving it into an `RpcClient`
/// to inspect requests afterwards.
#[cfg(feature = "test")]
#[derive(Debug, Clone, Default)]
pub struct MockTransport {
    success: bool,
    message: String,
    requests: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}
#[cfg(feature = "test")]
impl MockTransport {
    /// Always return `response` as a successful body.
    pub fn success(response: impl Into<String>) -> Self {
        Self {
            success: true,
            message: response.into(),
            requests: Default::default(),
        }
    }
    /// Always fail with a transport error carrying `error`.
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            message: error.into(),
            requests: Default::default(),
        }
    }
    /// The `n`-th captured request body, parsed as JSON. Panics if absent.
    pub fn request(&self, n: usize) -> serde_json::Value {
        serde_json::from_str(&self.requests.borrow()[n]).expect("request body is valid JSON")
    }
    /// How many requests have been sent through this transport.
    pub fn request_count(&self) -> usize {
        self.requests.borrow().len()
    }
}
#[cfg(feature = "test")]
impl RpcTransport for MockTransport {
    fn post(&self, _url: &str, body: &str) -> Result<String> {
        self.requests.borrow_mut().push(body.to_string());
        if self.success {
            Ok(self.message.clone())
        } else {
            Err(Error::Transport(self.message.clone()))
        }
    }
}
