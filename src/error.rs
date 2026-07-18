#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Transport(String),
    Decode(String),
    Rpc { code: i64, message: String },
    UnexpectedResponse(String),
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

pub type Result<T> = std::result::Result<T, Error>;
