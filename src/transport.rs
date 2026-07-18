use crate::{Error, Result};

pub trait RpcTransport {
    fn post(&self, url: &str, body: &str) -> Result<String>;
}

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

#[cfg(feature = "test")]
#[derive(Debug, Clone)]
pub struct MockTransport {
    pub success: bool,
    pub message: String,
}
#[cfg(feature = "test")]
impl MockTransport {
    pub fn pass(response: impl Into<String>) -> Self {
        Self {
            success: true,
            message: response.into(),
        }
    }
    pub fn fail(error: impl Into<String>) -> Self {
        Self {
            success: false,
            message: error.into(),
        }
    }
}
#[cfg(feature = "test")]
impl RpcTransport for MockTransport {
    fn post(&self, _url: &str, _body: &str) -> Result<String> {
        if self.success {
            Ok(self.message.clone())
        } else {
            Err(Error::Transport(self.message.clone()))
        }
    }
}
