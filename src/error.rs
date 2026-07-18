//! The single error type returned throughout the crate.
//!
//! The variants distinguish *where* a request went wrong, which matters to a
//! caller deciding whether to retry: a [`Transport`](Error::Transport) failure
//! may be transient, while [`Rpc`](Error::Rpc) and [`Parse`](Error::Parse) are
//! deterministic and will recur. [`Error::is_retryable`] encodes that judgement
//! and drives the client's automatic retry loop.

use serde_json::Value;

/// Everything that can go wrong between issuing a request and returning a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The HTTP round-trip itself failed (DNS, connection, timeout, an HTTP
    /// 429/5xx status, or a non-UTF-8 body). Raised by the
    /// [`RpcTransport`](crate::RpcTransport) implementation. Treated as
    /// transient and retried.
    Transport(String),
    /// The response body was not valid JSON.
    Decode(String),
    /// The node returned a well-formed JSON-RPC `error` object. `code` is the
    /// JSON-RPC error code, `message` its text, and `data` any structured
    /// payload the node attached — for a failed `sendTransaction`/
    /// `simulateTransaction` preflight this carries the program logs.
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    /// The reply was valid JSON but not the shape we expected — e.g. a missing
    /// `result` field, or a `result` that did not deserialize into the method's
    /// return type. Usually signals upstream schema drift.
    UnexpectedResponse(String),
    /// A field that should hold an encoded value (a base58 pubkey, hash, or
    /// signature) could not be parsed into its typed form.
    Parse(String),
    /// A polling operation (confirmation, waiting for a new blockhash) ran out
    /// of attempts before the condition was met.
    Timeout(String),
}

impl Error {
    /// Whether retrying the same request could plausibly succeed.
    ///
    /// Transport failures (connection/timeout/429/5xx) are retryable, as are the
    /// handful of JSON-RPC codes that mean "the node is momentarily behind"
    /// rather than "your request is wrong".
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Transport(_) => true,
            // -32005 node unhealthy/behind, -32004 block not available,
            // -32014 block status not yet available, -32016 min context slot
            // not reached — all transient node states.
            Error::Rpc { code, .. } => matches!(code, -32005 | -32004 | -32014 | -32016),
            _ => false,
        }
    }

    /// The JSON-RPC error code, if this is an [`Error::Rpc`].
    pub fn rpc_code(&self) -> Option<i64> {
        match self {
            Error::Rpc { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// The structured `data` payload of a JSON-RPC error, if present. For a
    /// preflight failure this is where `{ err, logs, unitsConsumed }` lives.
    pub fn rpc_data(&self) -> Option<&Value> {
        match self {
            Error::Rpc { data, .. } => data.as_ref(),
            _ => None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Transport(e) => write!(f, "transport error: {e}"),
            Error::Decode(e) => write!(f, "decode error: {e}"),
            Error::Rpc { code, message, .. } => write!(f, "rpc error: {code}: {message}"),
            Error::UnexpectedResponse(e) => write!(f, "unexpected response: {e}"),
            Error::Parse(e) => write!(f, "parse error: {e}"),
            Error::Timeout(e) => write!(f, "timeout: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Crate-wide result alias: every fallible operation returns this.
pub type Result<T> = std::result::Result<T, Error>;
