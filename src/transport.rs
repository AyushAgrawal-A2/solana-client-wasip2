//! The HTTP seam between the JSON-RPC engine and the outside world.
//!
//! `RpcClient` never performs I/O directly; it hands a request body to an
//! [`RpcTransport`] and gets a response body back. That one indirection is what
//! makes the crate both wasm-native and host-testable:
//!
//! - `WakiTransport` — the real transport, compiled only for wasm targets. It
//!   speaks HTTP through the host's `wasi:http` (via the `waki` crate), so TLS
//!   happens host-side and no sockets are linked into the guest. It maps HTTP
//!   429/5xx to a retryable [`Error::Transport`] and supports a connect timeout.
//! - [`MockTransport`] — a host-only fake that returns canned replies (including
//!   a "flaky" mode for exercising retries) and records outgoing requests, so
//!   the entire client can be tested with `cargo test` — no network, no wasm.

use core::time::Duration;

use crate::Result;

/// Sends a JSON-RPC request body to `url` and returns the response body.
///
/// Implementors own the HTTP concern entirely; the engine above them stays
/// pure. Any I/O failure should be reported as [`Error::Transport`].
pub trait RpcTransport {
    fn post(&self, url: &str, body: &str) -> Result<String>;

    /// Pause between retry attempts. The default sleeps the current thread
    /// (which works on `wasm32-wasip2` via the WASI clock); test transports
    /// override it to a no-op so the suite never actually waits.
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// The production transport: blocking HTTP over the host's `wasi:http`.
///
/// Only exists on wasm targets — on the host there is no `wasi:http` to call,
/// and tests use [`MockTransport`] instead.
#[cfg(target_family = "wasm")]
#[derive(Debug, Clone, Default)]
pub struct WakiTransport {
    /// Timeout for establishing the connection. `None` uses the host default.
    pub connect_timeout: Option<Duration>,
}

#[cfg(target_family = "wasm")]
impl WakiTransport {
    /// A transport with the host's default connection behaviour.
    pub fn new() -> Self {
        Self::default()
    }

    /// A transport that gives up if the connection is not established within
    /// `timeout`.
    pub fn with_connect_timeout(timeout: Duration) -> Self {
        Self {
            connect_timeout: Some(timeout),
        }
    }
}

#[cfg(target_family = "wasm")]
impl RpcTransport for WakiTransport {
    fn post(&self, url: &str, body: &str) -> Result<String> {
        use crate::Error;

        let mut req = waki::Client::new()
            .post(url)
            .header("content-type", "application/json")
            .body(body.as_bytes().to_vec());
        if let Some(timeout) = self.connect_timeout {
            req = req.connect_timeout(timeout);
        }

        let response = req.send().map_err(|e| Error::Transport(e.to_string()))?;
        let status = response.status_code();
        let bytes = response
            .body()
            .map_err(|e| Error::Transport(e.to_string()))?;

        // 429 (rate limited) and 5xx (server-side) are transient — surface them
        // as transport errors so the engine retries.
        if status == 429 || status >= 500 {
            return Err(Error::Transport(format!("http status {status}")));
        }

        String::from_utf8(bytes).map_err(|e| Error::Decode(e.to_string()))
    }
}

/// A transport for host tests: returns canned replies and records every request
/// so tests can assert what the client actually sent. Cloning shares the same
/// state (via `Rc`), so clone one before moving it into an `RpcClient` to
/// inspect requests afterwards. Its [`sleep`](RpcTransport::sleep) is a no-op,
/// so retry tests run instantly.
#[cfg(any(test, feature = "test"))]
#[derive(Debug, Clone, Default)]
pub struct MockTransport {
    success: bool,
    message: String,
    remaining_failures: std::rc::Rc<std::cell::RefCell<u32>>,
    requests: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}
#[cfg(any(test, feature = "test"))]
impl MockTransport {
    /// Always return `response` as a successful body.
    pub fn success(response: impl Into<String>) -> Self {
        Self {
            success: true,
            message: response.into(),
            ..Default::default()
        }
    }
    /// Always fail with a (retryable) transport error carrying `error`.
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            message: error.into(),
            ..Default::default()
        }
    }
    /// Fail with a retryable transport error `fail_times` times, then return
    /// `response`. For exercising the retry loop.
    pub fn flaky(fail_times: u32, response: impl Into<String>) -> Self {
        Self {
            success: true,
            message: response.into(),
            remaining_failures: std::rc::Rc::new(std::cell::RefCell::new(fail_times)),
            ..Default::default()
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
#[cfg(any(test, feature = "test"))]
impl RpcTransport for MockTransport {
    fn post(&self, _url: &str, body: &str) -> Result<String> {
        use crate::Error;

        self.requests.borrow_mut().push(body.to_string());
        {
            let mut remaining = self.remaining_failures.borrow_mut();
            if *remaining > 0 {
                *remaining -= 1;
                return Err(Error::Transport("flaky".into()));
            }
        }
        if self.success {
            Ok(self.message.clone())
        } else {
            Err(Error::Transport(self.message.clone()))
        }
    }

    fn sleep(&self, _duration: Duration) {}
}
