//! Request-construction tests — the logic this crate genuinely owns.
//!
//! Response *parsing* is delegated to Anza's official types, but *what we send*
//! (method names, default encodings, the nested `account_config`, optional
//! config handling, id sequencing, base64 encoding of wire bytes) is our code.
//! `MockTransport` records each request body so we can assert on it.

use solana_client_wasip2::{
    message::v0, message::VersionedMessage, transaction::versioned::VersionedTransaction,
    CommitmentConfig, MockTransport, RpcClient, RpcSignaturesForAddressConfig,
};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use std::str::FromStr;

fn pk() -> Pubkey {
    Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM").unwrap()
}

/// A minimal signed v0 transaction for exercising submit methods.
fn dummy_tx() -> VersionedTransaction {
    let payer = Keypair::new();
    let ix = system_instruction::transfer(&payer.pubkey(), &Pubkey::new_unique(), 1);
    let msg = VersionedMessage::V0(
        v0::Message::try_compile(&payer.pubkey(), &[ix], &[], Default::default()).unwrap(),
    );
    VersionedTransaction::try_new(msg, &[&payer]).unwrap()
}

/// Client whose transport returns `body` and records requests via the returned spy.
fn spy(body: &str) -> (RpcClient<MockTransport>, MockTransport) {
    let mock = MockTransport::success(body);
    let client = RpcClient::new_with_transport("http://unused", mock.clone());
    (client, mock)
}

#[test]
fn account_info_defaults_to_base64_encoding() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#);
    client
        .get_account_with_commitment(&pk(), CommitmentConfig::finalized())
        .unwrap();
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
    // result is null → the method errors, but the request is still recorded.
    let _ = client.get_transaction(
        &solana_signature::Signature::default(),
        solana_client_wasip2::UiTransactionEncoding::Base64,
    );
    let req = mock.request(0);
    assert_eq!(req["method"], "getTransaction");
    assert_eq!(req["params"][1]["encoding"], "base64");
    assert_eq!(req["params"][1]["maxSupportedTransactionVersion"], 0);
}

#[test]
fn plain_method_omits_config() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":[]}"#);
    client.get_signatures_for_address(&pk()).unwrap();
    // No config => params is just [address].
    assert_eq!(mock.request(0)["params"].as_array().unwrap().len(), 1);
}

#[test]
fn with_config_includes_config() {
    let (client, mock) = spy(r#"{"jsonrpc":"2.0","id":1,"result":[]}"#);
    let cfg = RpcSignaturesForAddressConfig {
        limit: Some(5),
        ..Default::default()
    };
    client.get_signatures_for_address_with_config(&pk(), cfg).unwrap();
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
fn send_transaction_serializes_to_base64() {
    let sig = solana_signature::Signature::default().to_string();
    let (client, mock) = spy(&format!(r#"{{"jsonrpc":"2.0","id":1,"result":"{sig}"}}"#));
    client.send_transaction(&dummy_tx()).unwrap();
    let req = mock.request(0);
    assert_eq!(req["method"], "sendTransaction");
    assert!(!req["params"][0].as_str().unwrap().is_empty()); // base64 wire bytes
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
