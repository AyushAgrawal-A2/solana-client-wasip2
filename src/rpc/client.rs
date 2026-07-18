//! The JSON-RPC engine.
//!
//! [`RpcClient`] is the one place that speaks the JSON-RPC 2.0 envelope. The
//! typed methods in [`crate::rpc::methods`] all bottom out in [`RpcClient::call`]
//! or [`RpcClient::call_typed`]; nothing else frames requests or inspects the
//! `result`/`error` split. It is generic over [`RpcTransport`] so it can run
//! against `wasi:http` in production and a mock in tests.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::{Error, Result, RpcTransport};

/// A JSON-RPC client bound to one endpoint URL and one transport.
///
/// Construct with [`RpcClient::new`], then call the typed methods from
/// [`crate::rpc::methods`]. Cheap to hold; a single instance can serve many
/// calls. Not `Sync` — the request-id counter is a `Cell`, which suits the
/// single-threaded wasm component model.
pub struct RpcClient<T: RpcTransport> {
    url: String,
    transport: T,
    /// Monotonic JSON-RPC request id, incremented per call. Its only job is to
    /// let a caller correlate a reply with its request; the value is not
    /// otherwise meaningful.
    id: std::cell::Cell<u64>,
}

impl<T: RpcTransport> RpcClient<T> {
    /// Create a client for `url` using `transport`.
    ///
    /// The URL (and any embedded API key) belongs to the caller — read it from
    /// plugin config, never hard-code it.
    pub fn new(url: impl Into<String>, transport: T) -> Self {
        Self {
            url: url.into(),
            transport,
            id: std::cell::Cell::new(1),
        }
    }

    /// Issue `method` with `params` and return the raw `result` value.
    ///
    /// Frames the JSON-RPC request, sends it through the transport, then splits
    /// the reply: a JSON-RPC `error` object becomes [`Error::Rpc`], a missing
    /// `result` becomes [`Error::UnexpectedResponse`], and a non-JSON body
    /// becomes [`Error::Decode`]. Prefer [`call_typed`](Self::call_typed) unless
    /// you deliberately want the untyped [`Value`] (e.g. to handle a response
    /// whose shape varies).
    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.id.get();
        self.id.set(id + 1);

        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method":method,
            "params":params
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
            Err(Error::Rpc { code, message })
        } else {
            response
                .get("result")
                .cloned()
                .ok_or_else(|| Error::UnexpectedResponse(format!("no result field: {response}")))
        }
    }

    /// Like [`call`](Self::call), but deserialize the `result` into `R`.
    ///
    /// A `result` that does not match `R` becomes [`Error::UnexpectedResponse`]
    /// naming the method — the signal to check for upstream schema drift.
    pub fn call_typed<R: DeserializeOwned>(&self, method: &str, params: Value) -> Result<R> {
        let response = self.call(method, params)?;
        serde_json::from_value(response)
            .map_err(|e| Error::UnexpectedResponse(format!("decoding {method} response: {e}")))
    }
}

/// Engine-level tests: the `result`/`error`/malformed-body split, independent
/// of any particular Solana method. Method-specific parsing lives in
/// `tests/methods.rs`; request framing in `tests/requests.rs`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    #[test]
    fn extracts_result() {
        let mock = MockTransport::success(r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#);
        let client = RpcClient::new("http://unused", mock);
        let out = client.call("getHealth", json!([])).unwrap();
        assert_eq!(out, json!("ok"));
    }

    #[test]
    fn surfaces_rpc_error() {
        let mock = MockTransport::success(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#,
        );
        let client = RpcClient::new("http://unused", mock);
        let err = client.call("nope", json!([])).unwrap_err();
        assert_eq!(
            err,
            Error::Rpc {
                code: -32601,
                message: "Method not found".into()
            }
        );
    }

    #[test]
    fn transport_failure_propagates() {
        let client = RpcClient::new("http://unused", MockTransport::failure("dns"));
        let err = client.call("getHealth", json!([])).unwrap_err();
        assert!(matches!(err, Error::Transport(_)));
    }

    #[test]
    fn garbage_body_is_decode_error() {
        let client = RpcClient::new("http://unused", MockTransport::success("not json"));
        let err = client.call("getHealth", json!([])).unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }
}
