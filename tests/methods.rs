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

use solana_client_wasip2::{Error, MockTransport, RpcClient, TransactionConfirmationStatus};
use solana_pubkey::Pubkey;
use solana_signature::Signature;
use std::str::FromStr;

/// Build a client whose transport always returns `body`.
fn client(body: &str) -> RpcClient<MockTransport> {
    RpcClient::new("http://unused", MockTransport::success(body))
}

fn pk() -> Pubkey {
    Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM").unwrap()
}

#[test]
fn get_version_parses() {
    let v = client(r#"{"jsonrpc":"2.0","id":1,"result":{"solana-core":"2.1.0","feature-set":123}}"#)
        .get_version()
        .unwrap();
    assert_eq!(v.solana_core, "2.1.0");
    assert_eq!(v.feature_set, Some(123));
}

#[test]
fn get_health_parses() {
    let h = client(r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#)
        .get_health()
        .unwrap();
    assert_eq!(h, "ok");
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
    let bh = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":{"blockhash":"EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k","lastValidBlockHeight":301234567}}}"#,
    )
    .get_latest_blockhash()
    .unwrap();
    assert_eq!(bh.blockhash, "EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k");
    assert_eq!(bh.last_valid_block_height, 301_234_567);
}

#[test]
fn rent_exemption_no_envelope() {
    let lamports = client(r#"{"jsonrpc":"2.0","id":1,"result":2039280}"#)
        .get_minimum_balance_for_rent_exemption(165)
        .unwrap();
    assert_eq!(lamports, 2_039_280);
}

#[test]
fn account_info_present_and_decodes_base64() {
    let info = client(
        r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":312},"value":{"data":["AQIDBA==","base64"],"executable":false,"lamports":388127550439,"owner":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA","rentEpoch":18446744073709551615,"space":82}}}"#,
    )
    .get_account_info(&pk())
    .unwrap()
    .expect("account present");
    assert_eq!(info.owner, "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    assert_eq!(info.data.decode(), Some(vec![1, 2, 3, 4]));
}

#[test]
fn account_info_absent_is_none() {
    let info = client(r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#)
        .get_account_info(&pk())
        .unwrap();
    assert!(info.is_none());
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
    .get_signature_statuses(&[Signature::default(), Signature::default()], false)
    .unwrap();
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
    .simulate_transaction(&[1, 2, 3])
    .unwrap();
    assert!(sim.err.is_none());
    assert_eq!(sim.units_consumed, Some(150));
    assert_eq!(sim.logs.unwrap().len(), 2);
}

#[test]
fn send_transaction_returns_signature() {
    // Signature::default() is 64 zero bytes; its base58 is a valid signature.
    let sig_str = Signature::default().to_string();
    let body = format!(r#"{{"jsonrpc":"2.0","id":1,"result":"{sig_str}"}}"#);
    let sig = client(&body).send_transaction(&[1, 2, 3, 4]).unwrap();
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
    assert_eq!(accts[0].pubkey, "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM");
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
fn get_transaction_absent_is_none() {
    let tx = client(r#"{"jsonrpc":"2.0","id":1,"result":null}"#)
        .get_transaction(&Signature::default())
        .unwrap();
    assert!(tx.is_none());
}

#[test]
fn identity_parses_result_string_to_pubkey() {
    let id = client(r#"{"jsonrpc":"2.0","id":1,"result":{"identity":"11111111111111111111111111111111"}}"#)
        .get_identity()
        .unwrap();
    assert_eq!(id, Pubkey::from_str("11111111111111111111111111111111").unwrap());
}

#[test]
fn genesis_hash_parses_to_hash() {
    let h = client(r#"{"jsonrpc":"2.0","id":1,"result":"EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k"}"#)
        .get_genesis_hash()
        .unwrap();
    assert_eq!(h.to_string(), "EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k");
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
fn fee_for_message_null_is_none() {
    // `value: null` (blockhash expired) must map to None, not an error.
    let fee = client(r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#)
        .get_fee_for_message("base64msg")
        .unwrap();
    assert!(fee.is_none());
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
