//! The JSON-RPC engine.
//!
//! [`RpcClient`] is the one place that speaks the JSON-RPC 2.0 envelope. The
//! typed methods in [`crate::rpc::methods`] all bottom out in the internal
//! `call`/`call_typed`; the public generic escape hatch is
//! [`send`](RpcClient::send). It is generic over [`RpcTransport`] so it can run
//! against `wasi:http` in production and a mock in tests.
//!
//! The engine also owns two cross-cutting concerns: **automatic retry** of
//! transient failures with exponential backoff (like the official client, this
//! is internal), and a **default commitment** the method layer applies when a
//! caller does not specify one.

use core::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use solana_rpc_client_types::request::RpcRequest;

use crate::{CommitmentConfig, Error, Result, RpcTransport};

/// Internal retry policy: how transient failures (see [`Error::is_retryable`])
/// are re-attempted with exponential backoff. Not part of the public API — the
/// official client's retry is likewise internal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    /// 3 retries, 500 ms base, doubling up to 8 s.
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 8_000,
        }
    }
}

impl RetryConfig {
    /// Backoff delay before the retry numbered `attempt` (0-based).
    fn delay_for(&self, attempt: u32) -> Duration {
        let shifted = self
            .base_delay_ms
            .saturating_mul(1u64.checked_shl(attempt).unwrap_or(u64::MAX));
        Duration::from_millis(shifted.min(self.max_delay_ms))
    }
}

/// A JSON-RPC client bound to one endpoint URL and one transport.
///
/// On wasm, construct with `RpcClient::new(url)` (and the `new_with_*` variants),
/// which supply the `wasi:http` transport — matching the official client's
/// constructors. On the host (tests, custom transports) use
/// [`new_with_transport`](Self::new_with_transport). Set the default commitment
/// with [`with_commitment`](Self::with_commitment), then call the typed methods
/// from [`crate::rpc::methods`]. Not `Sync` — the
/// request-id counter is a `Cell`, which suits the single-threaded wasm
/// component model.
pub struct RpcClient<T: RpcTransport> {
    url: String,
    transport: T,
    id: std::cell::Cell<u64>,
    retry: RetryConfig,
    commitment: CommitmentConfig,
}

impl<T: RpcTransport> RpcClient<T> {
    /// Create a client for `url` over an explicit `transport`, with default
    /// retry and commitment. The generic constructor (analogous to the official
    /// client's `new_sender`) — used for a custom transport or, in tests, the
    /// [`MockTransport`](crate::MockTransport). On wasm, prefer
    /// `RpcClient::new(url)` which supplies the `wasi:http` transport for you.
    ///
    /// The URL (and any embedded API key) belongs to the caller — read it from
    /// plugin config, never hard-code it.
    pub fn new_with_transport(url: impl ToString, transport: T) -> Self {
        Self {
            url: url.to_string(),
            transport,
            id: std::cell::Cell::new(1),
            retry: RetryConfig::default(),
            commitment: CommitmentConfig::default(),
        }
    }

    /// Set the default commitment applied by methods that take one implicitly.
    pub fn with_commitment(mut self, commitment_config: CommitmentConfig) -> Self {
        self.commitment = commitment_config;
        self
    }

    /// The client's default commitment.
    pub fn commitment(&self) -> CommitmentConfig {
        self.commitment
    }

    /// The endpoint URL this client posts to.
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Sleep via the transport (real on wasm, no-op under test). Used by the
    /// polling methods in [`crate::rpc::methods`], which cannot reach the
    /// private transport field directly.
    pub(crate) fn sleep(&self, duration: Duration) {
        self.transport.sleep(duration);
    }

    /// Issue `method` with `params` and return the raw `result` value, retrying
    /// transient failures with backoff.
    ///
    /// Frames the JSON-RPC request, sends it through the transport, then splits
    /// the reply: a JSON-RPC `error` object becomes [`Error::Rpc`] (carrying any
    /// `data` payload), a missing `result` becomes [`Error::UnexpectedResponse`],
    /// and a non-JSON body becomes [`Error::Decode`]. Public callers use the
    /// typed [`send`](Self::send).
    pub(crate) fn call(&self, method: &str, params: Value) -> Result<Value> {
        let mut attempt = 0;
        loop {
            match self.call_once(method, &params) {
                Err(e) if e.is_retryable() && attempt < self.retry.max_retries => {
                    self.transport.sleep(self.retry.delay_for(attempt));
                    attempt += 1;
                }
                other => return other,
            }
        }
    }

    /// A single request/response round-trip with no retry.
    fn call_once(&self, method: &str, params: &Value) -> Result<Value> {
        let id = self.id.get();
        self.id.set(id + 1);

        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        })
        .to_string();

        let response = self.transport.post(&self.url, &body)?;

        let response: Value =
            serde_json::from_str(&response).map_err(|e| Error::Decode(e.to_string()))?;

        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let data = error.get("data").cloned();
            Err(Error::Rpc {
                code,
                message,
                data,
            })
        } else {
            response
                .get("result")
                .cloned()
                .ok_or_else(|| Error::UnexpectedResponse(format!("no result field: {response}")))
        }
    }

    /// Issue `request` with `params` and deserialize the `result` into `R` — the
    /// generic escape hatch for any JSON-RPC method, including custom ones via
    /// [`RpcRequest::Custom`]. Matches the official client's `send`.
    ///
    /// A `result` that does not match `R` becomes [`Error::UnexpectedResponse`].
    pub fn send<R: DeserializeOwned>(&self, request: RpcRequest, params: Value) -> Result<R> {
        self.call_typed(request.as_str(), params)
    }

    /// Internal typed dispatch used by every method wrapper.
    pub(crate) fn call_typed<R: DeserializeOwned>(&self, method: &str, params: Value) -> Result<R> {
        let response = self.call(method, params)?;
        serde_json::from_value(response)
            .map_err(|e| Error::UnexpectedResponse(format!("decoding {method} response: {e}")))
    }
}

/// URL-based constructors matching the official `RpcClient`. These use the
/// `wasi:http` transport ([`WakiTransport`](crate::transport::WakiTransport)),
/// so a plugin writes `RpcClient::new(url)` with no transport to pass. Available
/// only on wasm, where `wasi:http` exists; host tests use
/// [`new_with_transport`](RpcClient::new_with_transport) with a mock.
#[cfg(target_family = "wasm")]
impl RpcClient<crate::transport::WakiTransport> {
    /// Create a client for `url` (default retry, `finalized` commitment).
    pub fn new(url: impl ToString) -> Self {
        Self::new_with_transport(url, crate::transport::WakiTransport::default())
    }

    /// Create a client for `url` with an explicit default commitment.
    pub fn new_with_commitment(url: impl ToString, commitment_config: CommitmentConfig) -> Self {
        Self::new(url).with_commitment(commitment_config)
    }

    /// Create a client for `url` with a connection timeout.
    pub fn new_with_timeout(url: impl ToString, timeout: Duration) -> Self {
        Self::new_with_transport(
            url,
            crate::transport::WakiTransport::with_connect_timeout(timeout),
        )
    }

    /// Create a client for `url` with a connection timeout and default commitment.
    pub fn new_with_timeout_and_commitment(
        url: impl ToString,
        timeout: Duration,
        commitment_config: CommitmentConfig,
    ) -> Self {
        Self::new_with_timeout(url, timeout).with_commitment(commitment_config)
    }

    /// Create a client for `url` with a connection timeout and default
    /// commitment. `confirm_transaction_initial_timeout` matches the official
    /// signature; confirmation here uses a fixed poll window, so it is unused.
    pub fn new_with_timeouts_and_commitment(
        url: impl ToString,
        timeout: Duration,
        commitment_config: CommitmentConfig,
        confirm_transaction_initial_timeout: Duration,
    ) -> Self {
        let _ = confirm_transaction_initial_timeout;
        Self::new_with_timeout_and_commitment(url, timeout, commitment_config)
    }
}

/// Engine-level tests: retry/backoff, the `result`/`error`/malformed-body split,
/// independent of any particular Solana method. Method-specific parsing lives in
/// `tests/methods.rs`; request framing in `tests/requests.rs`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    #[test]
    fn extracts_result() {
        let mock = MockTransport::success(r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#);
        let client = RpcClient::new_with_transport("http://unused", mock);
        let out = client.call("getHealth", json!([])).unwrap();
        assert_eq!(out, json!("ok"));
    }

    #[test]
    fn surfaces_rpc_error() {
        let mock = MockTransport::success(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#,
        );
        let client = RpcClient::new_with_transport("http://unused", mock);
        let err = client.call("nope", json!([])).unwrap_err();
        assert_eq!(
            err,
            Error::Rpc {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            }
        );
        assert!(!err.is_retryable()); // method-not-found is permanent
    }

    #[test]
    fn transport_failure_propagates() {
        // Retries (no-op sleep under test), then propagates the transport error.
        let client = RpcClient::new_with_transport("http://unused", MockTransport::failure("dns"));
        let err = client.call("getHealth", json!([])).unwrap_err();
        assert!(matches!(err, Error::Transport(_)));
    }

    #[test]
    fn garbage_body_is_decode_error() {
        let client =
            RpcClient::new_with_transport("http://unused", MockTransport::success("not json"));
        let err = client.call("getHealth", json!([])).unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }

    #[test]
    fn retries_transient_then_succeeds() {
        // Fails twice (retryable), succeeds on the third attempt.
        let mock = MockTransport::flaky(2, r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#);
        let spy = mock.clone();
        let client = RpcClient::new_with_transport("http://unused", mock); // default: 3 retries
        let out = client.call("getHealth", json!([])).unwrap();
        assert_eq!(out, json!("ok"));
        assert_eq!(spy.request_count(), 3); // 2 failures + 1 success
    }

    #[test]
    fn gives_up_after_max_retries() {
        // Fails more times than the default retry budget (3), so it gives up.
        let mock = MockTransport::flaky(10, r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#);
        let spy = mock.clone();
        let client = RpcClient::new_with_transport("http://unused", mock);
        assert!(client.call("getHealth", json!([])).is_err());
        assert_eq!(spy.request_count(), 4); // 1 attempt + 3 retries, then gives up
    }

    #[test]
    fn rpc_error_data_is_captured() {
        let mock = MockTransport::success(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32002,"message":"preflight failed","data":{"logs":["Program log: boom"]}}}"#,
        );
        let client = RpcClient::new_with_transport("http://unused", mock);
        let err = client.call("sendTransaction", json!([])).unwrap_err();
        assert_eq!(err.rpc_code(), Some(-32002));
        assert!(err.rpc_data().unwrap()["logs"].is_array());
    }
}
