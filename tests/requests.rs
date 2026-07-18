//! Request-construction tests — the logic this crate genuinely owns.
//!
//! Response *parsing* is delegated to Anza's official types, but *what we send*
//! (method names, default encodings, the nested `account_config`, optional
//! config handling, id sequencing, base64 encoding of wire bytes) is our code.
//! `MockTransport` records each request body so we can assert on it.

use solana_client_wasip2::{CommitmentConfig, MockTransport, RpcClient, RpcSignaturesForAddressConfig};
use solana_pubkey::Pubkey;
use std::str::FromStr;

fn pk() -> Pubkey {
    Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM").unwrap()
}

/// Client whose transport returns `body` and records requests via the returned spy.
fn spy(body: &str) -> (RpcClient<MockTransport>, MockTransport) {
    let mock = MockTransport::success(body);
    let client = RpcClient::new("http://unused", mock.clone());
    (client, mock)
}

#[test]
fn account_info_defaults_to_base64_encoding() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#);
    client.get_account_info(&pk()).unwrap();
    let req = mock.request(0);
    assert_eq!(req["method"], "getAccountInfo");
    assert_eq!(req["params"][0], pk().to_string());
    assert_eq!(req["params"][1]["encoding"], "base64");
}

#[test]
fn program_accounts_nests_account_config_flattened() {
    // The official RpcProgramAccountsConfig flattens `account_config`, so the
    // encoding must appear at the top level of the config object on the wire.
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":[]}"#);
    client.get_program_accounts(&pk()).unwrap();
    let req = mock.request(0);
    assert_eq!(req["method"], "getProgramAccounts");
    assert_eq!(req["params"][1]["encoding"], "base64");
}

#[test]
fn get_transaction_sends_base64_and_max_version() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":null}"#);
    client
        .get_transaction(&solana_signature::Signature::default())
        .unwrap();
    let req = mock.request(0);
    assert_eq!(req["method"], "getTransaction");
    assert_eq!(req["params"][1]["encoding"], "base64");
    assert_eq!(req["params"][1]["maxSupportedTransactionVersion"], 0);
}

#[test]
fn optional_config_omitted_when_none() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":[]}"#);
    client.get_signatures_for_address(&pk(), None).unwrap();
    // No config => params is just [address].
    assert_eq!(mock.request(0)["params"].as_array().unwrap().len(), 1);
}

#[test]
fn optional_config_included_when_some() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":[]}"#);
    let cfg = RpcSignaturesForAddressConfig {
        limit: Some(5),
        ..Default::default()
    };
    client.get_signatures_for_address(&pk(), Some(&cfg)).unwrap();
    let req = mock.request(0);
    assert_eq!(req["params"].as_array().unwrap().len(), 2);
    assert_eq!(req["params"][1]["limit"], 5);
}

#[test]
fn commitment_is_passed_through() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":1}"#);
    client.get_slot_with_commitment(CommitmentConfig::confirmed()).unwrap();
    assert_eq!(mock.request(0)["params"][0]["commitment"], "confirmed");
}

#[test]
fn send_transaction_base64_encodes_wire_bytes() {
    let sig = solana_signature::Signature::default().to_string();
    let (client, mock) = spy(&format!(r#"{{"jsonrpc":"2.0","id":1,"result":"{sig}"}}"#));
    client.send_transaction(&[1, 2, 3, 4]).unwrap();
    let req = mock.request(0);
    assert_eq!(req["method"], "sendTransaction");
    assert_eq!(req["params"][0], "AQIDBA=="); // base64 of [1,2,3,4]
    assert_eq!(req["params"][1]["encoding"], "base64");
}

#[test]
fn request_ids_increment() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":1}"#);
    client.get_slot().unwrap();
    client.get_slot().unwrap();
    assert_eq!(mock.request_count(), 2);
    assert_eq!(mock.request(0)["id"], 1);
    assert_eq!(mock.request(1)["id"], 2);
}
