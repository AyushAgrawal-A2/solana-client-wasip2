//! Live-validator integration: the RPC method surface is exercised against a
//! real `solana-test-validator`, and checked two ways —
//!
//! 1. **Equality with the native client.** For methods whose result is stable,
//!    the crate-under-test and the official `solana-rpc-client` are both called
//!    and their results compared structurally (as JSON). Because
//!    `solana-rpc-client-api` re-exports `solana-rpc-client-types` (the crate we
//!    depend on), both clients parse into the *same* Rust types, so a structural
//!    diff is exact.
//! 2. **Known-good shape.** For methods whose result changes slot-to-slot, both
//!    calls must succeed (or consistently fail) and satisfy an invariant.
//!
//! One validator is spawned for the whole file; results are collected into a
//! report and asserted at the end, so a single failure does not hide the rest.
//! Requires `solana-test-validator` on `PATH`; run with
//! `cargo test`.

mod harness;
use harness::{native_client, our_client, to_value, TestValidator, UreqTransport};

use solana_client_wasip2::{
    RpcBlockConfig, RpcTransactionConfig, TokenAccountsFilter, UiTransactionEncoding,
};
use solana_keypair::Keypair;
use solana_message::{v0, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use solana_transaction::versioned::VersionedTransaction;

use std::fmt::Debug;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Collects per-method outcomes so the whole surface is reported at once.
#[derive(Default)]
struct Report {
    rows: Vec<(String, Result<String, String>)>,
}

impl Report {
    fn pass(&mut self, name: &str, detail: impl Into<String>) {
        self.rows.push((name.to_string(), Ok(detail.into())));
    }

    fn fail(&mut self, name: &str, why: impl Into<String>) {
        self.rows.push((name.to_string(), Err(why.into())));
    }

    /// Structural equality between the client-under-test and the native client.
    fn eq(&mut self, name: &str, ours: serde_json::Value, native: serde_json::Value) {
        if ours == native {
            self.pass(name, "== native");
        } else {
            self.fail(name, format!("ours={ours}  native={native}"));
        }
    }

    /// Both calls must succeed and their results be structurally equal.
    fn eq_ok<A, B, EA, EB>(&mut self, name: &str, ours: Result<A, EA>, native: Result<B, EB>)
    where
        A: Serialize,
        B: Serialize,
        EA: Debug,
        EB: Debug,
    {
        match (ours, native) {
            (Ok(a), Ok(b)) => self.eq(name, to_value(&a), to_value(&b)),
            (Ok(_), Err(e)) => self.fail(name, format!("native errored: {e:?}")),
            (Err(e), Ok(_)) => self.fail(name, format!("ours errored: {e:?}")),
            (Err(a), Err(b)) => self.fail(name, format!("both errored ours={a:?} native={b:?}")),
        }
    }

    /// A comparison after normalizing each side (e.g. sorting order-independent
    /// lists) so ordering differences between two live calls don't matter.
    fn eq_norm<A, B, EA, EB>(
        &mut self,
        name: &str,
        ours: Result<A, EA>,
        native: Result<B, EB>,
        norm: impl Fn(serde_json::Value) -> serde_json::Value,
    ) where
        A: Serialize,
        B: Serialize,
        EA: Debug,
        EB: Debug,
    {
        match (ours, native) {
            (Ok(a), Ok(b)) => {
                let (a, b) = (norm(to_value(&a)), norm(to_value(&b)));
                self.eq(name, a, b);
            }
            (o, n) => self.fail(
                name,
                format!("ours_ok={} native_ok={}", o.is_ok(), n.is_ok()),
            ),
        }
    }

    /// Volatile method: pass if both reach the same outcome kind (both Ok, or
    /// both Err — some methods legitimately error on a fresh validator).
    fn both_reach<A, B, EA, EB>(&mut self, name: &str, ours: Result<A, EA>, native: Result<B, EB>)
    where
        EA: Debug,
        EB: Debug,
    {
        match (ours, native) {
            (Ok(_), Ok(_)) => self.pass(name, "both ok"),
            (Err(_), Err(_)) => self.pass(name, "both err (consistent)"),
            (Ok(_), Err(e)) => self.fail(name, format!("ours ok, native err: {e:?}")),
            (Err(e), Ok(_)) => self.fail(name, format!("native ok, ours err: {e:?}")),
        }
    }

    fn finish(self) {
        let mut failed = 0;
        eprintln!("\n===== live-validator method report =====");
        for (name, outcome) in &self.rows {
            match outcome {
                Ok(detail) => eprintln!("  PASS  {name:<40} {detail}"),
                Err(why) => {
                    failed += 1;
                    eprintln!("  FAIL  {name:<40} {why}");
                }
            }
        }
        eprintln!("===== {} checks, {failed} failed =====\n", self.rows.len());
        assert_eq!(failed, 0, "{failed} method check(s) failed");
    }
}

/// Sort a JSON array of keyed accounts / tuples by their serialized form, so two
/// clients' lists compare equal regardless of node-returned order.
fn sort_array(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Array(mut items) => {
            items.sort_by_key(|i| i.to_string());
            serde_json::Value::Array(items)
        }
        other => other,
    }
}

/// Airdrop to `pubkey` via the native client and wait until the balance lands.
fn airdrop(url: &str, pubkey: &Pubkey, lamports: u64) {
    let native = native_client(url);
    native.request_airdrop(pubkey, lamports).expect("airdrop");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if native.get_balance(pubkey).unwrap_or(0) >= lamports {
            return;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    panic!("airdrop to {pubkey} did not settle");
}

/// Build, sign, and confirm a v0 transaction with the client under test.
fn submit(
    ours: &solana_client_wasip2::RpcClient<UreqTransport>,
    payer: &Keypair,
    signers: &[&Keypair],
    ixs: &[solana_instruction::Instruction],
) -> solana_signature::Signature {
    let blockhash = ours.get_latest_blockhash().unwrap();
    let msg = VersionedMessage::V0(
        v0::Message::try_compile(&payer.pubkey(), ixs, &[], blockhash).unwrap(),
    );
    let tx = VersionedTransaction::try_new(msg, signers).unwrap();
    ours.send_and_confirm_transaction(&tx).unwrap()
}

/// Create an SPL mint + associated token account and mint a supply into it,
/// returning `(mint, ata, owner)`.
fn setup_spl_token(
    ours: &solana_client_wasip2::RpcClient<UreqTransport>,
    payer: &Keypair,
) -> (Pubkey, Pubkey, Pubkey) {
    let token_program = spl_token_interface::id();
    let mint = Keypair::new();
    let owner = payer.pubkey();
    let rent = ours.get_minimum_balance_for_rent_exemption(82).unwrap();

    // Create + initialize the mint (decimals = 0), create the ATA, mint supply.
    let create = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        82,
        &token_program,
    );
    let init = spl_token_interface::instruction::initialize_mint(
        &token_program,
        &mint.pubkey(),
        &owner,
        None,
        0,
    )
    .unwrap();
    submit(ours, payer, &[payer, &mint], &[create, init]);

    let ata = spl_associated_token_account_interface::address::get_associated_token_address(
        &owner,
        &mint.pubkey(),
    );
    let create_ata =
        spl_associated_token_account_interface::instruction::create_associated_token_account(
            &payer.pubkey(),
            &owner,
            &mint.pubkey(),
            &token_program,
        );
    let mint_to = spl_token_interface::instruction::mint_to(
        &token_program,
        &mint.pubkey(),
        &ata,
        &owner,
        &[],
        1_000_000,
    )
    .unwrap();
    submit(ours, payer, &[payer], &[create_ata, mint_to]);

    (mint.pubkey(), ata, owner)
}

#[test]
fn all_methods_against_live_validator() {
    let validator = TestValidator::start();
    let url = validator.rpc_url();
    let ours = our_client(url);
    let native = native_client(url);
    let mut r = Report::default();

    // ===== Node / cluster info ===============================================
    r.eq_ok("getVersion", ours.get_version(), native.get_version());
    r.eq_ok(
        "getGenesisHash",
        ours.get_genesis_hash(),
        native.get_genesis_hash(),
    );
    r.eq_ok("getIdentity", ours.get_identity(), native.get_identity());
    r.eq_norm(
        "getClusterNodes",
        ours.get_cluster_nodes(),
        native.get_cluster_nodes(),
        sort_array,
    );
    r.both_reach(
        "getVoteAccounts",
        ours.get_vote_accounts(),
        native.get_vote_accounts(),
    );
    match (ours.get_health(), native.get_health()) {
        (Ok(()), Ok(())) => r.pass("getHealth", "both Ok(())"),
        (o, n) => r.fail("getHealth", format!("ours={o:?} native={n:?}")),
    }

    // ===== Slots / epochs ====================================================
    r.both_reach("getSlot", ours.get_slot(), native.get_slot());
    r.both_reach(
        "getBlockHeight",
        ours.get_block_height(),
        native.get_block_height(),
    );
    r.both_reach(
        "getTransactionCount",
        ours.get_transaction_count(),
        native.get_transaction_count(),
    );
    r.eq_ok(
        "getEpochSchedule",
        ours.get_epoch_schedule(),
        native.get_epoch_schedule(),
    );
    r.both_reach(
        "getEpochInfo",
        ours.get_epoch_info(),
        native.get_epoch_info(),
    );
    r.eq_ok(
        "getSlotLeaders",
        ours.get_slot_leaders(0, 10),
        native.get_slot_leaders(0, 10),
    );
    r.both_reach(
        "getFirstAvailableBlock",
        ours.get_first_available_block(),
        native.get_first_available_block(),
    );
    r.both_reach(
        "minimumLedgerSlot",
        ours.minimum_ledger_slot(),
        native.minimum_ledger_slot(),
    );
    r.both_reach(
        "getMaxRetransmitSlot",
        ours.get_max_retransmit_slot(),
        native.get_max_retransmit_slot(),
    );
    r.both_reach(
        "getMaxShredInsertSlot",
        ours.get_max_shred_insert_slot(),
        native.get_max_shred_insert_slot(),
    );
    r.both_reach(
        "getHighestSnapshotSlot",
        ours.get_highest_snapshot_slot(),
        native.get_highest_snapshot_slot(),
    );
    r.eq_ok(
        "getStakeMinimumDelegation",
        ours.get_stake_minimum_delegation(),
        native.get_stake_minimum_delegation(),
    );
    r.eq_ok(
        "getMinimumBalanceForRentExemption",
        ours.get_minimum_balance_for_rent_exemption(165),
        native.get_minimum_balance_for_rent_exemption(165),
    );
    r.both_reach(
        "getLeaderSchedule",
        ours.get_leader_schedule(None),
        native.get_leader_schedule(None),
    );

    // ===== Accounts (state we control) =======================================
    let alice = Keypair::new();
    airdrop(url, &alice.pubkey(), 5_000_000_000);
    let bob = Pubkey::new_unique();
    r.eq_ok(
        "getBalance",
        ours.get_balance(&alice.pubkey()),
        native.get_balance(&alice.pubkey()),
    );
    r.eq_ok(
        "getAccountInfo",
        ours.get_account(&alice.pubkey()),
        native.get_account(&alice.pubkey()),
    );
    r.eq_ok(
        "getAccountData(getAccountInfo bytes)",
        ours.get_account_data(&alice.pubkey()),
        native.get_account_data(&alice.pubkey()),
    );
    r.eq_ok(
        "getMultipleAccounts",
        ours.get_multiple_accounts(&[alice.pubkey(), bob]),
        native.get_multiple_accounts(&[alice.pubkey(), bob]),
    );
    r.both_reach(
        "getLargestAccounts",
        ours.get_largest_accounts_with_config(Default::default()),
        native.get_largest_accounts_with_config(Default::default()),
    );

    // ===== Blockhash / fees ==================================================
    let blockhash = ours.get_latest_blockhash().unwrap();
    r.both_reach(
        "getLatestBlockhash",
        ours.get_latest_blockhash(),
        native.get_latest_blockhash(),
    );
    r.eq_ok(
        "isBlockhashValid",
        ours.is_blockhash_valid(
            &blockhash,
            solana_commitment_config::CommitmentConfig::processed(),
        ),
        native.is_blockhash_valid(
            &blockhash,
            solana_commitment_config::CommitmentConfig::processed(),
        ),
    );

    // ===== Submit + confirm a real transfer, then query it ===================
    let ix = system_instruction::transfer(&alice.pubkey(), &bob, 1_000_000);
    let sig = submit(&ours, &alice, &[&alice], &[ix]);
    r.pass("sendTransaction+confirm", format!("{sig}"));
    r.eq_ok(
        "getBalance(recipient)",
        ours.get_balance(&bob),
        native.get_balance(&bob),
    );

    let tx_slot = ours
        .get_signature_statuses(&[sig])
        .unwrap()
        .value
        .into_iter()
        .next()
        .flatten()
        .map(|s| s.slot)
        .unwrap();

    r.eq_norm(
        "getSignatureStatuses",
        ours.get_signature_statuses(&[sig]).map(|r| r.value),
        native.get_signature_statuses(&[sig]).map(|r| r.value),
        |v| v,
    );
    r.eq_ok(
        "getSignatureStatus",
        ours.get_signature_status(&sig),
        native.get_signature_status(&sig),
    );
    r.eq_ok(
        "confirmTransaction",
        ours.confirm_transaction(&sig),
        native.confirm_transaction(&sig),
    );
    r.eq_norm(
        "getSignaturesForAddress",
        ours.get_signatures_for_address(&bob),
        native.get_signatures_for_address(&bob),
        sort_array,
    );

    let tx_cfg = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        max_supported_transaction_version: Some(0),
        ..Default::default()
    };
    r.eq_ok(
        "getTransaction",
        ours.get_transaction_with_config(&sig, tx_cfg),
        native.get_transaction_with_config(&sig, tx_cfg),
    );

    // ===== Blocks (using the confirmed tx's slot) ============================
    r.both_reach(
        "getBlockTime",
        ours.get_block_time(tx_slot),
        native.get_block_time(tx_slot),
    );
    r.both_reach(
        "getBlocks",
        ours.get_blocks(tx_slot, Some(tx_slot)),
        native.get_blocks(tx_slot, Some(tx_slot)),
    );
    r.both_reach(
        "getBlocksWithLimit",
        ours.get_blocks_with_limit(tx_slot, 1),
        native.get_blocks_with_limit(tx_slot, 1),
    );
    let block_cfg = RpcBlockConfig {
        encoding: Some(UiTransactionEncoding::Json),
        max_supported_transaction_version: Some(0),
        ..Default::default()
    };
    r.eq_ok(
        "getBlock",
        ours.get_block_with_config(tx_slot, block_cfg),
        native.get_block_with_config(tx_slot, block_cfg),
    );
    r.both_reach(
        "getBlockProduction",
        ours.get_block_production(),
        native.get_block_production(),
    );

    // ===== Simulate ==========================================================
    let sim_bh = ours.get_latest_blockhash().unwrap();
    let sim_ix = system_instruction::transfer(&alice.pubkey(), &bob, 1);
    let sim_tx = VersionedTransaction::try_new(
        VersionedMessage::V0(
            v0::Message::try_compile(&alice.pubkey(), &[sim_ix], &[], sim_bh).unwrap(),
        ),
        &[&alice],
    )
    .unwrap();
    match (
        ours.simulate_transaction(&sim_tx),
        native.simulate_transaction(&sim_tx),
    ) {
        (Ok(a), Ok(b)) if a.value.err.is_none() && b.value.err.is_none() => {
            r.pass("simulateTransaction", "both ok, no sim error")
        }
        (o, n) => r.fail(
            "simulateTransaction",
            format!("ours_ok={} native_ok={}", o.is_ok(), n.is_ok()),
        ),
    }

    // ===== SPL token methods (real mint + ATA + minted supply) ===============
    let (mint, ata, owner) = setup_spl_token(&ours, &alice);
    r.eq_ok(
        "getTokenAccountBalance",
        ours.get_token_account_balance(&ata),
        native.get_token_account_balance(&ata),
    );
    r.eq_ok(
        "getTokenSupply",
        ours.get_token_supply(&mint),
        native.get_token_supply(&mint),
    );
    r.eq_norm(
        "getTokenLargestAccounts",
        ours.get_token_largest_accounts(&mint),
        native.get_token_largest_accounts(&mint),
        sort_array,
    );
    r.eq_norm(
        "getTokenAccountsByOwner",
        ours.get_token_accounts_by_owner(&owner, TokenAccountsFilter::Mint(mint)),
        native.get_token_accounts_by_owner(&owner, TokenAccountsFilter::Mint(mint)),
        sort_array,
    );
    r.eq_norm(
        "getTokenAccountsByDelegate",
        ours.get_token_accounts_by_delegate(&owner, TokenAccountsFilter::Mint(mint)),
        native.get_token_accounts_by_delegate(&owner, TokenAccountsFilter::Mint(mint)),
        sort_array,
    );

    // ===== Inflation / supply ================================================
    r.eq_ok(
        "getInflationGovernor",
        ours.get_inflation_governor(),
        native.get_inflation_governor(),
    );
    r.eq_ok(
        "getInflationRate",
        ours.get_inflation_rate(),
        native.get_inflation_rate(),
    );
    r.both_reach(
        "getInflationReward",
        ours.get_inflation_reward(&[alice.pubkey()], None),
        native.get_inflation_reward(&[alice.pubkey()], None),
    );
    r.both_reach("supply", ours.supply(), native.supply());

    // ===== Performance / fees ================================================
    r.both_reach(
        "getRecentPerformanceSamples",
        ours.get_recent_performance_samples(Some(5)),
        native.get_recent_performance_samples(Some(5)),
    );
    r.both_reach(
        "getRecentPrioritizationFees",
        ours.get_recent_prioritization_fees(&[alice.pubkey()]),
        native.get_recent_prioritization_fees(&[alice.pubkey()]),
    );

    r.finish();
}
