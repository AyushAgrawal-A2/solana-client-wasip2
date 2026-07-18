//! The JSON-RPC method surface, mirroring the official `RpcClient`.
//!
//! Every method here is a thin adapter over the engine: build `params`, pick the
//! method name, call [`RpcClient::call_typed`], and shape the result. Three
//! conventions run through the file:
//!
//! - **The envelope.** Many Solana methods wrap their result in
//!   `{ context, value }`. Those deserialize into
//!   [`RpcResponse<T>`](crate::RpcResponse) and return the inner `.value`;
//!   methods that return a bare result deserialize straight into it. The
//!   doc-comment on each method names the exact JSON-RPC method it calls.
//! - **Config variants.** Where the node accepts a trailing config object, a
//!   `*_with_config` (or `*_with_commitment`) variant exposes it, and the plain
//!   method supplies sensible defaults — notably base64 encoding for account and
//!   transaction data, which keeps large payloads parseable.
//! - **Typed keys in, strings out.** Methods take `&Pubkey` / `&Signature` /
//!   `&Hash` and stringify them for the wire; a few methods parse a returned
//!   base58 string back into a typed key via the helpers at the bottom of the
//!   file, which is the only place [`Error::Parse`] originates.
//!
//! ## Adding a method
//!
//! Add the wrapper here, then update `tests/rpc_coverage.rs` — it diffs this
//! surface against Anza's `RpcRequest` enum and will fail to compile until the
//! new (or renamed) method is accounted for.

use serde::Serialize;
use serde_json::{json, Value};
use solana_hash::Hash;
use solana_pubkey::Pubkey;
use solana_signature::Signature;

use crate::error::{Error, Result};
use crate::rpc::config::*;
use crate::rpc::response::*;
use crate::rpc::RpcClient;
use crate::transport::RpcTransport;
use crate::CommitmentConfig;

/// Serialize an optional trailing config object to JSON.
///
/// Returns `None` when no config was supplied, so the caller can omit the
/// trailing param entirely — some nodes reject an explicit `null` there.
fn cfg<C: Serialize>(c: Option<&C>) -> Result<Option<Value>> {
    match c {
        Some(c) => Ok(Some(
            serde_json::to_value(c).map_err(|e| Error::Parse(e.to_string()))?,
        )),
        None => Ok(None),
    }
}

/// Base64-encode a serialized transaction for `sendTransaction` /
/// `simulateTransaction` (the encoding those methods default to here).
fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

impl<T: RpcTransport> RpcClient<T> {
    // ----- Node / cluster info -------------------------------------------

    /// `getVersion`
    pub fn get_version(&self) -> Result<RpcVersionInfo> {
        self.call_typed("getVersion", json!([]))
    }

    /// `getHealth` — returns `"ok"` or an RPC error.
    pub fn get_health(&self) -> Result<String> {
        self.call_typed("getHealth", json!([]))
    }

    /// `getIdentity`
    pub fn get_identity(&self) -> Result<Pubkey> {
        let id: RpcIdentity = self.call_typed("getIdentity", json!([]))?;
        parse_pubkey(&id.identity)
    }

    /// `getGenesisHash`
    pub fn get_genesis_hash(&self) -> Result<Hash> {
        let s: String = self.call_typed("getGenesisHash", json!([]))?;
        parse_hash(&s)
    }

    /// `getClusterNodes`
    pub fn get_cluster_nodes(&self) -> Result<Vec<RpcContactInfo>> {
        self.call_typed("getClusterNodes", json!([]))
    }

    /// `getVoteAccounts`
    pub fn get_vote_accounts(&self) -> Result<RpcVoteAccountStatus> {
        self.call_typed("getVoteAccounts", json!([]))
    }

    /// `getVoteAccounts` with config.
    pub fn get_vote_accounts_with_config(
        &self,
        config: &RpcGetVoteAccountsConfig,
    ) -> Result<RpcVoteAccountStatus> {
        self.call_typed("getVoteAccounts", json!([config]))
    }

    // ----- Slots / epochs / blocks ---------------------------------------

    /// `getSlot`
    pub fn get_slot(&self) -> Result<u64> {
        self.call_typed("getSlot", json!([]))
    }

    /// `getSlot` with commitment.
    pub fn get_slot_with_commitment(&self, commitment: CommitmentConfig) -> Result<u64> {
        self.call_typed("getSlot", json!([commitment]))
    }

    /// `getBlockHeight`
    pub fn get_block_height(&self) -> Result<u64> {
        self.call_typed("getBlockHeight", json!([]))
    }

    /// `getBlockHeight` with commitment.
    pub fn get_block_height_with_commitment(&self, commitment: CommitmentConfig) -> Result<u64> {
        self.call_typed("getBlockHeight", json!([commitment]))
    }

    /// `getSlotLeader`
    pub fn get_slot_leader(&self) -> Result<Pubkey> {
        let s: String = self.call_typed("getSlotLeader", json!([]))?;
        parse_pubkey(&s)
    }

    /// `getSlotLeaders` — `limit` leaders starting at `start_slot`.
    pub fn get_slot_leaders(&self, start_slot: u64, limit: u64) -> Result<Vec<Pubkey>> {
        let v: Vec<String> = self.call_typed("getSlotLeaders", json!([start_slot, limit]))?;
        v.iter().map(|s| parse_pubkey(s)).collect()
    }

    /// `getEpochInfo`
    pub fn get_epoch_info(&self) -> Result<EpochInfo> {
        self.call_typed("getEpochInfo", json!([]))
    }

    /// `getEpochInfo` with config.
    pub fn get_epoch_info_with_config(&self, config: &RpcContextConfig) -> Result<EpochInfo> {
        self.call_typed("getEpochInfo", json!([config]))
    }

    /// `getEpochSchedule`
    pub fn get_epoch_schedule(&self) -> Result<EpochSchedule> {
        self.call_typed("getEpochSchedule", json!([]))
    }

    /// `getTransactionCount`
    pub fn get_transaction_count(&self) -> Result<u64> {
        self.call_typed("getTransactionCount", json!([]))
    }

    /// `getFirstAvailableBlock`
    pub fn get_first_available_block(&self) -> Result<u64> {
        self.call_typed("getFirstAvailableBlock", json!([]))
    }

    /// `minimumLedgerSlot`
    pub fn minimum_ledger_slot(&self) -> Result<u64> {
        self.call_typed("minimumLedgerSlot", json!([]))
    }

    /// `getMaxRetransmitSlot`
    pub fn get_max_retransmit_slot(&self) -> Result<u64> {
        self.call_typed("getMaxRetransmitSlot", json!([]))
    }

    /// `getMaxShredInsertSlot`
    pub fn get_max_shred_insert_slot(&self) -> Result<u64> {
        self.call_typed("getMaxShredInsertSlot", json!([]))
    }

    /// `getHighestSnapshotSlot`
    pub fn get_highest_snapshot_slot(&self) -> Result<RpcSnapshotSlotInfo> {
        self.call_typed("getHighestSnapshotSlot", json!([]))
    }

    /// `getBlockTime` — Unix timestamp of the block at `slot`.
    pub fn get_block_time(&self, slot: u64) -> Result<Option<i64>> {
        self.call_typed("getBlockTime", json!([slot]))
    }

    /// `getBlocks` — confirmed blocks in `[start, end]` (end optional).
    pub fn get_blocks(&self, start_slot: u64, end_slot: Option<u64>) -> Result<Vec<u64>> {
        let params = match end_slot {
            Some(end) => json!([start_slot, end]),
            None => json!([start_slot]),
        };
        self.call_typed("getBlocks", params)
    }

    /// `getBlocksWithLimit` — up to `limit` confirmed blocks from `start_slot`.
    pub fn get_blocks_with_limit(&self, start_slot: u64, limit: u64) -> Result<Vec<u64>> {
        self.call_typed("getBlocksWithLimit", json!([start_slot, limit]))
    }

    /// `getBlock`
    pub fn get_block(&self, slot: u64) -> Result<UiConfirmedBlock> {
        self.call_typed("getBlock", json!([slot]))
    }

    /// `getBlock` with config.
    pub fn get_block_with_config(
        &self,
        slot: u64,
        config: &RpcBlockConfig,
    ) -> Result<UiConfirmedBlock> {
        self.call_typed("getBlock", json!([slot, config]))
    }

    /// `getBlockCommitment` — the commitment array is returned as `Vec<u64>`.
    pub fn get_block_commitment(&self, slot: u64) -> Result<RpcBlockCommitment<Vec<u64>>> {
        self.call_typed("getBlockCommitment", json!([slot]))
    }

    /// `getBlockProduction`
    pub fn get_block_production(&self) -> Result<RpcResponse<RpcBlockProduction>> {
        self.call_typed("getBlockProduction", json!([]))
    }

    /// `getBlockProduction` with config.
    pub fn get_block_production_with_config(
        &self,
        config: &RpcBlockProductionConfig,
    ) -> Result<RpcResponse<RpcBlockProduction>> {
        self.call_typed("getBlockProduction", json!([config]))
    }

    /// `getLeaderSchedule` for the epoch containing `slot` (or current if None).
    pub fn get_leader_schedule(
        &self,
        slot: Option<u64>,
        config: Option<&RpcLeaderScheduleConfig>,
    ) -> Result<Option<RpcLeaderSchedule>> {
        let mut params = vec![json!(slot)];
        if let Some(c) = cfg(config)? {
            params.push(c);
        }
        self.call_typed("getLeaderSchedule", Value::Array(params))
    }

    // ----- Accounts / balances -------------------------------------------

    /// `getBalance` — lamports at `pubkey`.
    pub fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        let r: RpcResponse<u64> = self.call_typed("getBalance", json!([pubkey.to_string()]))?;
        Ok(r.value)
    }

    /// `getBalance` with commitment.
    pub fn get_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment: CommitmentConfig,
    ) -> Result<u64> {
        let r: RpcResponse<u64> =
            self.call_typed("getBalance", json!([pubkey.to_string(), commitment]))?;
        Ok(r.value)
    }

    /// `getAccountInfo` — `None` if the account does not exist. Defaults to
    /// base64 account-data encoding.
    pub fn get_account_info(&self, pubkey: &Pubkey) -> Result<Option<UiAccount>> {
        self.get_account_info_with_config(
            pubkey,
            &RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                ..Default::default()
            },
        )
    }

    /// `getAccountInfo` with config.
    pub fn get_account_info_with_config(
        &self,
        pubkey: &Pubkey,
        config: &RpcAccountInfoConfig,
    ) -> Result<Option<UiAccount>> {
        let r: RpcResponse<Option<UiAccount>> =
            self.call_typed("getAccountInfo", json!([pubkey.to_string(), config]))?;
        Ok(r.value)
    }

    /// `getMultipleAccounts` — one entry per input, `None` where absent.
    /// Defaults to base64 account-data encoding.
    pub fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<UiAccount>>> {
        self.get_multiple_accounts_with_config(
            pubkeys,
            &RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                ..Default::default()
            },
        )
    }

    /// `getMultipleAccounts` with config.
    pub fn get_multiple_accounts_with_config(
        &self,
        pubkeys: &[Pubkey],
        config: &RpcAccountInfoConfig,
    ) -> Result<Vec<Option<UiAccount>>> {
        let keys: Vec<String> = pubkeys.iter().map(|p| p.to_string()).collect();
        let r: RpcResponse<Vec<Option<UiAccount>>> =
            self.call_typed("getMultipleAccounts", json!([keys, config]))?;
        Ok(r.value)
    }

    /// `getProgramAccounts` — defaults to base64 account-data encoding.
    pub fn get_program_accounts(&self, program_id: &Pubkey) -> Result<Vec<RpcKeyedAccount>> {
        self.get_program_accounts_with_config(
            program_id,
            &RpcProgramAccountsConfig {
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
    }

    /// `getProgramAccounts` with config. When `with_context` is set the node
    /// wraps the result in an envelope; this method handles both shapes.
    pub fn get_program_accounts_with_config(
        &self,
        program_id: &Pubkey,
        config: &RpcProgramAccountsConfig,
    ) -> Result<Vec<RpcKeyedAccount>> {
        let raw = self.call(
            "getProgramAccounts",
            json!([program_id.to_string(), config]),
        )?;
        // With `withContext: true` the result is `{ context, value: [...] }`.
        if let Some(value) = raw.get("value") {
            serde_json::from_value(value.clone())
                .map_err(|e| Error::UnexpectedResponse(e.to_string()))
        } else {
            serde_json::from_value(raw).map_err(|e| Error::UnexpectedResponse(e.to_string()))
        }
    }

    /// `getLargestAccounts`
    pub fn get_largest_accounts(
        &self,
        config: Option<&RpcLargestAccountsConfig>,
    ) -> Result<Vec<RpcAccountBalance>> {
        let mut params = vec![];
        if let Some(c) = cfg(config)? {
            params.push(c);
        }
        let r: RpcResponse<Vec<RpcAccountBalance>> =
            self.call_typed("getLargestAccounts", Value::Array(params))?;
        Ok(r.value)
    }

    /// `getMinimumBalanceForRentExemption` — lamports for a `data_len` account.
    pub fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> Result<u64> {
        self.call_typed("getMinimumBalanceForRentExemption", json!([data_len]))
    }

    /// `getStakeMinimumDelegation`
    pub fn get_stake_minimum_delegation(&self) -> Result<u64> {
        let r: RpcResponse<u64> = self.call_typed("getStakeMinimumDelegation", json!([]))?;
        Ok(r.value)
    }

    // ----- Tokens --------------------------------------------------------

    /// `getTokenAccountBalance`
    pub fn get_token_account_balance(&self, pubkey: &Pubkey) -> Result<UiTokenAmount> {
        let r: RpcResponse<UiTokenAmount> =
            self.call_typed("getTokenAccountBalance", json!([pubkey.to_string()]))?;
        Ok(r.value)
    }

    /// `getTokenSupply`
    pub fn get_token_supply(&self, mint: &Pubkey) -> Result<UiTokenAmount> {
        let r: RpcResponse<UiTokenAmount> =
            self.call_typed("getTokenSupply", json!([mint.to_string()]))?;
        Ok(r.value)
    }

    /// `getTokenLargestAccounts`
    pub fn get_token_largest_accounts(&self, mint: &Pubkey) -> Result<Vec<RpcTokenAccountBalance>> {
        let r: RpcResponse<Vec<RpcTokenAccountBalance>> =
            self.call_typed("getTokenLargestAccounts", json!([mint.to_string()]))?;
        Ok(r.value)
    }

    /// `getTokenAccountsByOwner` — defaults to base64 account-data encoding.
    pub fn get_token_accounts_by_owner(
        &self,
        owner: &Pubkey,
        filter: RpcTokenAccountsFilter,
    ) -> Result<Vec<RpcKeyedAccount>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            ..Default::default()
        };
        let r: RpcResponse<Vec<RpcKeyedAccount>> = self.call_typed(
            "getTokenAccountsByOwner",
            json!([owner.to_string(), filter, config]),
        )?;
        Ok(r.value)
    }

    /// `getTokenAccountsByDelegate` — defaults to base64 account-data encoding.
    pub fn get_token_accounts_by_delegate(
        &self,
        delegate: &Pubkey,
        filter: RpcTokenAccountsFilter,
    ) -> Result<Vec<RpcKeyedAccount>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            ..Default::default()
        };
        let r: RpcResponse<Vec<RpcKeyedAccount>> = self.call_typed(
            "getTokenAccountsByDelegate",
            json!([delegate.to_string(), filter, config]),
        )?;
        Ok(r.value)
    }

    // ----- Blockhash / fees ----------------------------------------------

    /// `getLatestBlockhash`
    pub fn get_latest_blockhash(&self) -> Result<RpcBlockhash> {
        let r: RpcResponse<RpcBlockhash> = self.call_typed("getLatestBlockhash", json!([]))?;
        Ok(r.value)
    }

    /// `getLatestBlockhash` with commitment.
    pub fn get_latest_blockhash_with_commitment(
        &self,
        commitment: CommitmentConfig,
    ) -> Result<RpcBlockhash> {
        let r: RpcResponse<RpcBlockhash> =
            self.call_typed("getLatestBlockhash", json!([commitment]))?;
        Ok(r.value)
    }

    /// `isBlockhashValid`
    pub fn is_blockhash_valid(&self, blockhash: &Hash) -> Result<bool> {
        let r: RpcResponse<bool> =
            self.call_typed("isBlockhashValid", json!([blockhash.to_string()]))?;
        Ok(r.value)
    }

    /// `getFeeForMessage` — `message` is base64-encoded wire bytes. `None` if
    /// the blockhash in the message has expired.
    pub fn get_fee_for_message(&self, message_base64: &str) -> Result<Option<u64>> {
        let r: RpcResponse<Option<u64>> =
            self.call_typed("getFeeForMessage", json!([message_base64]))?;
        Ok(r.value)
    }

    // ----- Inflation -----------------------------------------------------

    /// `getInflationGovernor`
    pub fn get_inflation_governor(&self) -> Result<RpcInflationGovernor> {
        self.call_typed("getInflationGovernor", json!([]))
    }

    /// `getInflationRate`
    pub fn get_inflation_rate(&self) -> Result<RpcInflationRate> {
        self.call_typed("getInflationRate", json!([]))
    }

    /// `getInflationReward` — one entry per address (`None` where unavailable).
    pub fn get_inflation_reward(
        &self,
        addresses: &[Pubkey],
        config: Option<&RpcEpochConfig>,
    ) -> Result<Vec<Option<RpcInflationReward>>> {
        let keys: Vec<String> = addresses.iter().map(|p| p.to_string()).collect();
        let mut params = vec![json!(keys)];
        if let Some(c) = cfg(config)? {
            params.push(c);
        }
        self.call_typed("getInflationReward", Value::Array(params))
    }

    // ----- Supply / performance ------------------------------------------

    /// `getSupply`
    pub fn get_supply(&self, config: Option<&RpcSupplyConfig>) -> Result<RpcSupply> {
        let mut params = vec![];
        if let Some(c) = cfg(config)? {
            params.push(c);
        }
        let r: RpcResponse<RpcSupply> = self.call_typed("getSupply", Value::Array(params))?;
        Ok(r.value)
    }

    /// `getRecentPerformanceSamples` — up to `limit` samples (max 720).
    pub fn get_recent_performance_samples(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<RpcPerfSample>> {
        let params = match limit {
            Some(l) => json!([l]),
            None => json!([]),
        };
        self.call_typed("getRecentPerformanceSamples", params)
    }

    /// `getRecentPrioritizationFees` for the given `addresses` (may be empty).
    pub fn get_recent_prioritization_fees(
        &self,
        addresses: &[Pubkey],
    ) -> Result<Vec<RpcPrioritizationFee>> {
        let keys: Vec<String> = addresses.iter().map(|p| p.to_string()).collect();
        self.call_typed("getRecentPrioritizationFees", json!([keys]))
    }

    // ----- Signatures / transactions -------------------------------------

    /// `getSignaturesForAddress`
    pub fn get_signatures_for_address(
        &self,
        address: &Pubkey,
        config: Option<&RpcSignaturesForAddressConfig>,
    ) -> Result<Vec<RpcConfirmedTransactionStatusWithSignature>> {
        let mut params = vec![json!(address.to_string())];
        if let Some(c) = cfg(config)? {
            params.push(c);
        }
        self.call_typed("getSignaturesForAddress", Value::Array(params))
    }

    /// `getSignatureStatuses` — one status per input signature.
    pub fn get_signature_statuses(
        &self,
        signatures: &[Signature],
        search_history: bool,
    ) -> Result<Vec<Option<TransactionStatus>>> {
        let sigs: Vec<String> = signatures.iter().map(|s| s.to_string()).collect();
        let r: RpcResponse<Vec<Option<TransactionStatus>>> = self.call_typed(
            "getSignatureStatuses",
            json!([sigs, { "searchTransactionHistory": search_history }]),
        )?;
        Ok(r.value)
    }

    /// `getTransaction` — `None` if not found. Defaults to base64 encoding and
    /// max transaction version 0.
    pub fn get_transaction(
        &self,
        signature: &Signature,
    ) -> Result<Option<EncodedConfirmedTransactionWithStatusMeta>> {
        self.get_transaction_with_config(
            signature,
            &RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Base64),
                max_supported_transaction_version: Some(0),
                ..Default::default()
            },
        )
    }

    /// `getTransaction` with config.
    pub fn get_transaction_with_config(
        &self,
        signature: &Signature,
        config: &RpcTransactionConfig,
    ) -> Result<Option<EncodedConfirmedTransactionWithStatusMeta>> {
        self.call_typed("getTransaction", json!([signature.to_string(), config]))
    }

    // ----- Sending / simulating / airdrop --------------------------------

    /// `sendTransaction` — `wire` is the serialized transaction. Returns the
    /// signature. This crate never signs; sign before calling.
    pub fn send_transaction(&self, wire: &[u8]) -> Result<Signature> {
        self.send_transaction_with_config(wire, &RpcSendTransactionConfig::default())
    }

    /// `sendTransaction` with config.
    pub fn send_transaction_with_config(
        &self,
        wire: &[u8],
        config: &RpcSendTransactionConfig,
    ) -> Result<Signature> {
        let mut config = *config;
        config.encoding = Some(UiTransactionEncoding::Base64);
        let sig: String = self.call_typed("sendTransaction", json!([b64(wire), config]))?;
        parse_signature(&sig)
    }

    /// `simulateTransaction` — `wire` is the serialized transaction.
    pub fn simulate_transaction(&self, wire: &[u8]) -> Result<RpcSimulateTransactionResult> {
        self.simulate_transaction_with_config(wire, &RpcSimulateTransactionConfig::default())
    }

    /// `simulateTransaction` with config.
    pub fn simulate_transaction_with_config(
        &self,
        wire: &[u8],
        config: &RpcSimulateTransactionConfig,
    ) -> Result<RpcSimulateTransactionResult> {
        let mut config = config.clone();
        config.encoding = Some(UiTransactionEncoding::Base64);
        let r: RpcResponse<RpcSimulateTransactionResult> =
            self.call_typed("simulateTransaction", json!([b64(wire), config]))?;
        Ok(r.value)
    }

    /// `requestAirdrop` — devnet/testnet only. Returns the signature.
    pub fn request_airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<Signature> {
        let sig: String =
            self.call_typed("requestAirdrop", json!([pubkey.to_string(), lamports]))?;
        parse_signature(&sig)
    }
}

// ----- base58 parse helpers -----------------------------------------------
//
// A handful of methods return a bare base58 string (an identity pubkey, a
// genesis hash, an airdrop signature). These turn that string into its typed
// form, mapping a malformed value to `Error::Parse` rather than panicking.

fn parse_pubkey(s: &str) -> Result<Pubkey> {
    s.parse().map_err(|e| Error::Parse(format!("pubkey: {e}")))
}

fn parse_hash(s: &str) -> Result<Hash> {
    s.parse().map_err(|e| Error::Parse(format!("hash: {e}")))
}

fn parse_signature(s: &str) -> Result<Signature> {
    s.parse()
        .map_err(|e| Error::Parse(format!("signature: {e}")))
}
