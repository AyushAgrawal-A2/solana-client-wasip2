//! Response-parsing tests: feed each method a representative, node-shaped
//! JSON-RPC reply and assert it decodes into the expected value. No network —
//! `MockTransport` returns a canned body.
//!
//! Because the response types are Anza's official ones, these double as
//! **upstream-drift guards**: if a pinned version bump changes a response shape,
//! the relevant test fails, which is the intended signal for the manual-update
//! workflow. They also cover the paths this crate genuinely owns — envelope
//! unwrapping, `null` → `None`, `Vec<Option<_>>`, and the base58 parse helpers
//! (including their failure mode). Request *construction* is tested separately
//! in `requests.rs`; the engine internals in `src/rpc/client.rs`.

use solana_client_wasip2::{
    message::v0, message::VersionedMessage, transaction::versioned::VersionedTransaction, Error,
    MockTransport, RpcClient, TransactionConfirmationStatus,
};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use std::str::FromStr;

/// Build a client whose transport always returns `body`.
fn client(body: &str) -> RpcClient<MockTransport> {
    RpcClient::new_with_transport("http://unused", MockTransport::success(body))
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

fn pk() -> Pubkey {
    Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM").unwrap()
}

#[test]
fn get_version_parses() {
    let v =
        client(r#"{"jsonrpc":"2.0","id":1,"result":{"solana-core":"2.1.0","feature-set":123}}"#)
            .get_version()
            .unwrap();
    assert_eq!(v.solana_core, "2.1.0");
    assert_eq!(v.feature_set, Some(123));
}

#[test]
fn get_health_parses() {
    client(r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#)
        .get_health()
        .unwrap(); // Ok(()) when healthy
}

#[test]
fn get_balance_unwraps_envelope() {
    let lamports = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"apiVersion":"2.1.0","slot":312},"value":1000000000}}"#,
    )
    .get_balance(&pk())
    .unwrap();
    assert_eq!(lamports, 1_000_000_000);
}

#[test]
fn latest_blockhash_parses() {
    let (hash, last_valid) = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":{"blockhash":"EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k","lastValidBlockHeight":301234567}}}"#,
    )
    .get_latest_blockhash_with_commitment(solana_client_wasip2::CommitmentConfig::finalized())
    .unwrap();
    assert_eq!(
        hash.to_string(),
        "EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k"
    );
    assert_eq!(last_valid, 301_234_567);
}

#[test]
fn rent_exemption_no_envelope() {
    let lamports = client(r#"{"jsonrpc":"2.0","id":1,"result":2039280}"#)
        .get_minimum_balance_for_rent_exemption(165)
        .unwrap();
    assert_eq!(lamports, 2_039_280);
}

#[test]
fn get_account_decodes_to_native_account() {
    let acct = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":{"data":["AQIDBA==","base64"],"executable":false,"lamports":388127550439,"owner":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA","rentEpoch":18446744073709551615,"space":82}}}"#,
    )
    .get_account(&pk())
    .unwrap();
    assert_eq!(
        acct.owner,
        Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap()
    );
    assert_eq!(acct.data, vec![1, 2, 3, 4]);
    assert_eq!(acct.lamports, 388_127_550_439);
}

#[test]
fn account_absent_value_is_none() {
    let r = client(r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#)
        .get_account_with_commitment(&pk(), solana_client_wasip2::CommitmentConfig::finalized())
        .unwrap();
    assert!(r.value.is_none());
}

#[test]
fn token_account_balance_parses() {
    let amt = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":{"amount":"25000000","decimals":6,"uiAmount":25.0,"uiAmountString":"25"}}}"#,
    )
    .get_token_account_balance(&pk())
    .unwrap();
    assert_eq!(amt.amount, "25000000");
    assert_eq!(amt.decimals, 6);
    assert_eq!(amt.ui_amount, Some(25.0));
}

#[test]
fn epoch_info_parses() {
    let e = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"absoluteSlot":312,"blockHeight":300,"epoch":42,"slotIndex":100,"slotsInEpoch":432000,"transactionCount":999}}"#,
    )
    .get_epoch_info()
    .unwrap();
    assert_eq!(e.epoch, 42);
    assert_eq!(e.slots_in_epoch, 432_000);
    assert_eq!(e.transaction_count, Some(999));
}

#[test]
fn signature_statuses_with_null_entry() {
    // Real node shape includes the legacy `status` field required by the
    // official `TransactionStatus` type.
    let statuses = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":[{"slot":100,"confirmations":10,"status":{"Ok":null},"err":null,"confirmationStatus":"confirmed"},null]}}"#,
    )
    .get_signature_statuses(&[Signature::default(), Signature::default()])
    .unwrap()
    .value;
    assert_eq!(statuses.len(), 2);
    assert_eq!(
        statuses[0].as_ref().unwrap().confirmation_status,
        Some(TransactionConfirmationStatus::Confirmed)
    );
    assert!(statuses[1].is_none());
}

#[test]
fn simulate_transaction_parses_logs_and_err() {
    let sim = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":{"err":null,"logs":["Program 11111111111111111111111111111111 invoke [1]","Program 11111111111111111111111111111111 success"],"accounts":null,"unitsConsumed":150,"returnData":null,"innerInstructions":null}}}"#,
    )
    .simulate_transaction(&dummy_tx())
    .unwrap()
    .value;
    assert!(sim.err.is_none());
    assert_eq!(sim.units_consumed, Some(150));
    assert_eq!(sim.logs.unwrap().len(), 2);
}

#[test]
fn send_transaction_returns_signature() {
    // Signature::default() is 64 zero bytes; its base58 is a valid signature.
    let sig_str = Signature::default().to_string();
    let body = format!(r#"{{"jsonrpc":"2.0","id":1,"result":"{sig_str}"}}"#);
    let sig = client(&body).send_transaction(&dummy_tx()).unwrap();
    assert_eq!(sig, Signature::default());
}

#[test]
fn program_accounts_plain_array() {
    let accts = client(
        r#"{"jsonrpc":"2.0","id":1,"result":[{"pubkey":"9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM","account":{"data":["","base64"],"executable":false,"lamports":10,"owner":"11111111111111111111111111111111","rentEpoch":0,"space":0}}]}"#,
    )
    .get_program_accounts(&pk())
    .unwrap();
    assert_eq!(accts.len(), 1);
    assert_eq!(accts[0].0, pk()); // (Pubkey, Account)
    assert_eq!(accts[0].1.lamports, 10);
}

#[test]
fn program_accounts_with_context_envelope() {
    // withContext:true wraps the array in an envelope — must still parse.
    let accts = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":[{"pubkey":"9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM","account":{"data":["","base64"],"executable":false,"lamports":10,"owner":"11111111111111111111111111111111","rentEpoch":0,"space":0}}]}}"#,
    )
    .get_program_accounts(&pk())
    .unwrap();
    assert_eq!(accts.len(), 1);
}

#[test]
fn get_transaction_absent_errors() {
    let err = client(r#"{"jsonrpc":"2.0","id":1,"result":null}"#)
        .get_transaction(
            &Signature::default(),
            solana_client_wasip2::UiTransactionEncoding::Base64,
        )
        .unwrap_err();
    assert!(matches!(err, Error::UnexpectedResponse(_)));
}

#[test]
fn identity_parses_result_string_to_pubkey() {
    let id = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"identity":"11111111111111111111111111111111"}}"#,
    )
    .get_identity()
    .unwrap();
    assert_eq!(
        id,
        Pubkey::from_str("11111111111111111111111111111111").unwrap()
    );
}

#[test]
fn genesis_hash_parses_to_hash() {
    let h = client(
        r#"{"jsonrpc":"2.0","id":1,"result":"EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k"}"#,
    )
    .get_genesis_hash()
    .unwrap();
    assert_eq!(
        h.to_string(),
        "EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k"
    );
}

#[test]
fn slot_leaders_parses_vec_of_pubkeys() {
    let leaders = client(
        r#"{"jsonrpc":"2.0","id":1,"result":["11111111111111111111111111111111","9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"]}"#,
    )
    .get_slot_leaders(0, 2)
    .unwrap();
    assert_eq!(leaders.len(), 2);
    assert_eq!(leaders[1], pk());
}

#[test]
fn unparseable_pubkey_is_parse_error() {
    // A well-formed JSON-RPC reply carrying a bogus base58 pubkey must surface
    // as a typed parse error, not a panic.
    let err = client(r#"{"jsonrpc":"2.0","id":1,"result":{"identity":"not-a-valid-pubkey!"}}"#)
        .get_identity()
        .unwrap_err();
    assert!(matches!(err, Error::Parse(_)), "got {err:?}");
}

#[test]
fn fee_for_message_null_errors() {
    // `value: null` (blockhash expired) → error, matching the official client.
    let msg = solana_client_wasip2::message::VersionedMessage::V0(
        solana_client_wasip2::message::v0::Message::default(),
    );
    let err = client(r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#)
        .get_fee_for_message(&msg)
        .unwrap_err();
    assert!(matches!(err, Error::UnexpectedResponse(_)));
}

#[test]
fn multiple_accounts_parses_mixed_presence() {
    let accts = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":[null,{"data":["","base64"],"executable":false,"lamports":5,"owner":"11111111111111111111111111111111","rentEpoch":0,"space":0}]}}"#,
    )
    .get_multiple_accounts(&[pk(), pk()])
    .unwrap();
    assert_eq!(accts.len(), 2);
    assert!(accts[0].is_none());
    assert_eq!(accts[1].as_ref().unwrap().lamports, 5);
}
