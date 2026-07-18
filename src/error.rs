//! The single error type returned throughout the crate.
//!
//! The variants distinguish *where* a request went wrong, which matters to a
//! caller deciding whether to retry: a [`Transport`](Error::Transport) failure
//! may be transient, while [`Rpc`](Error::Rpc) and
//! [`Parse`](Error::Parse) are deterministic and will recur.

/// Everything that can go wrong between issuing a request and returning a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The HTTP round-trip itself failed (DNS, connection, non-UTF-8 body).
    /// Raised by the [`RpcTransport`](crate::RpcTransport) implementation.
    Transport(String),
    /// The response body was not valid JSON.
    Decode(String),
    /// The node returned a well-formed JSON-RPC `error` object. `code` is the
    /// JSON-RPC error code; `message` its human-readable text.
    Rpc { code: i64, message: String },
    /// The reply was valid JSON but not the shape we expected — e.g. a missing
    /// `result` field, or a `result` that did not deserialize into the method's
    /// return type. Usually signals upstream schema drift.
    UnexpectedResponse(String),
    /// A field that should hold an encoded value (a base58 pubkey, hash, or
    /// signature) could not be parsed into its typed form.
    Parse(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Transport(e) => write!(f, "transport error: {e}"),
            Error::Decode(e) => write!(f, "decode error: {e}"),
            Error::Rpc { code, message } => write!(f, "rpc error: {code}: {message}"),
            Error::UnexpectedResponse(e) => write!(f, "unexpected response: {e}"),
            Error::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Crate-wide result alias: every fallible operation returns this.
pub type Result<T> = std::result::Result<T, Error>;
