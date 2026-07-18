//! Confirmation polling, typed transaction submit, `get_account`, new-blockhash
//! polling, and default-commitment plumbing. `MockTransport`'s `sleep` is a
//! no-op, so the pollers run without waiting.

use solana_client_wasip2::{
    hash::Hash, message::v0, message::VersionedMessage, pubkey::Pubkey, signature::Signature,
    transaction::versioned::VersionedTransaction, CommitmentConfig, MockTransport, RpcClient,
};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;

fn client(body: &str) -> RpcClient<MockTransport> {
    RpcClient::new_with_transport("http://unused", MockTransport::success(body))
}

const CONFIRMED_STATUS: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":[{"slot":100,"confirmations":1,"status":{"Ok":null},"err":null,"confirmationStatus":"confirmed"}]}}"#;

#[test]
fn get_signature_status_returns_first_entry() {
    let s = client(CONFIRMED_STATUS)
        .get_signature_status(&Signature::default())
        .unwrap();
    assert!(s.is_some());
}

#[test]
fn confirm_transaction_true_when_reached() {
    // Default commitment is finalized; a "confirmed" status does NOT reach it,
    // but at `confirmed` commitment it does.
    let c = client(CONFIRMED_STATUS);
    assert!(c
        .confirm_transaction_with_commitment(&Signature::default(), CommitmentConfig::confirmed())
        .unwrap()
        .value);
}

#[test]
fn confirm_transaction_false_when_commitment_not_satisfied() {
    // A single check (like the official client): "confirmed" does not satisfy
    // "finalized", so the value is false.
    let c = client(CONFIRMED_STATUS);
    assert!(!c
        .confirm_transaction_with_commitment(&Signature::default(), CommitmentConfig::finalized())
        .unwrap()
        .value);
}

#[test]
fn confirm_transaction_false_on_onchain_failure() {
    // A failed transaction that reached the commitment reports `false` — confirm
    // answers "committed *and* succeeded?", matching the official client. The
    // failure reason stays reachable via get_signature_status.
    let failed = r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":[{"slot":100,"confirmations":1,"status":{"Err":"AccountNotFound"},"err":"AccountNotFound","confirmationStatus":"confirmed"}]}}"#;
    let c = client(failed);
    assert!(!c
        .confirm_transaction_with_commitment(&Signature::default(), CommitmentConfig::confirmed())
        .unwrap()
        .value);
    let status = c
        .get_signature_status(&Signature::default())
        .unwrap()
        .unwrap();
    assert!(status.is_err()); // the on-chain error is still visible here
}

#[test]
fn signature_status_with_commitment_gates_on_commitment() {
    // The #1 fix: the commitment argument is honored. A "confirmed" status is
    // reported at `confirmed` but hidden at `finalized`.
    assert!(client(CONFIRMED_STATUS)
        .get_signature_status_with_commitment(&Signature::default(), CommitmentConfig::confirmed())
        .unwrap()
        .is_some());
    assert!(client(CONFIRMED_STATUS)
        .get_signature_status_with_commitment(&Signature::default(), CommitmentConfig::finalized())
        .unwrap()
        .is_none());
}

#[test]
fn get_account_errors_when_absent() {
    let c = client(r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#);
    assert!(c.get_account(&Pubkey::new_unique()).is_err());
}

#[test]
fn get_new_latest_blockhash_returns_when_changed() {
    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":{"blockhash":"EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k","lastValidBlockHeight":100}}}"#;
    let new = client(body)
        .get_new_latest_blockhash(&Hash::default())
        .unwrap();
    assert_eq!(new.to_string(), "EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k");
}

#[test]
fn typed_submit_serializes_and_sends() {
    let payer = Keypair::new();
    let ix = system_instruction::transfer(&payer.pubkey(), &Pubkey::new_unique(), 1);
    let msg = VersionedMessage::V0(
        v0::Message::try_compile(&payer.pubkey(), &[ix], &[], Default::default()).unwrap(),
    );
    let tx = VersionedTransaction::try_new(msg, &[&payer]).unwrap();

    let sig = Signature::default().to_string();
    let c = client(&format!(r#"{{"jsonrpc":"2.0","id":1,"result":"{sig}"}}"#));
    let out = c.send_transaction(&tx).unwrap();
    assert_eq!(out, Signature::default());
}

#[test]
fn default_commitment_is_finalized_and_configurable() {
    // Default: get_balance sends finalized.
    let mock = MockTransport::success(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":1}}"#,
    );
    let c = RpcClient::new_with_transport("http://x", mock.clone());
    c.get_balance(&Pubkey::new_unique()).unwrap();
    assert_eq!(mock.request(0)["params"][1]["commitment"], "finalized");

    // Overridden: with_commitment(confirmed) flows into the request.
    let mock2 = MockTransport::success(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":1}}"#,
    );
    let c2 = RpcClient::new_with_transport("http://x", mock2.clone()).with_commitment(CommitmentConfig::confirmed());
    c2.get_balance(&Pubkey::new_unique()).unwrap();
    assert_eq!(mock2.request(0)["params"][1]["commitment"], "confirmed");
}
