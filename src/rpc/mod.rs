use serde_json::{json, Value};

use crate::{Error, Result, RpcTransport};

pub struct RpcClient<T: RpcTransport> {
    url: String,
    transport: T,
    id: std::cell::Cell<u64>,
}

impl<T: RpcTransport> RpcClient<T> {
    pub fn new(url: impl Into<String>, transport: T) -> Self {
        Self {
            url: url.into(),
            transport,
            id: std::cell::Cell::new(1),
        }
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    #[test]
    fn extracts_result() {
        let mock = MockTransport::pass(r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#);
        let client = RpcClient::new("http://unused", mock);
        let out = client.call("getHealth", json!([])).unwrap();
        assert_eq!(out, json!("ok"));
    }

    #[test]
    fn surfaces_rpc_error() {
        let mock = MockTransport::pass(
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
        let client = RpcClient::new("http://unused", MockTransport::fail("dns"));
        let err = client.call("getHealth", json!([])).unwrap_err();
        assert!(matches!(err, Error::Transport(_)));
    }

    #[test]
    fn garbage_body_is_decode_error() {
        let client = RpcClient::new("http://unused", MockTransport::pass("not json"));
        let err = client.call("getHealth", json!([])).unwrap_err();
        assert!(matches!(err, Error::Decode(_)));
    }
}
