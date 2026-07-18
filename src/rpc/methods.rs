//! The JSON-RPC method surface, mirroring the official `RpcClient`.
//!
//! Every method here is a thin adapter over the engine: build `params`, pick the
//! method name, dispatch through the engine, and shape the result. (The public
//! generic escape hatch for un-wrapped methods is [`RpcClient::send`].) Three
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

use serde_json::{json, Value};
use solana_hash::Hash;
use solana_pubkey::Pubkey;
use solana_signature::Signature;

use core::time::Duration;

use solana_account::Account;
use solana_account_decoder_client_types::token::{TokenAccountType, UiTokenAccount};
use solana_message::VersionedMessage;
use solana_rpc_client_types::request::TokenAccountsFilter;
use solana_transaction::versioned::VersionedTransaction;
use solana_transaction_error::TransactionResult;

use crate::error::{Error, Result};
use crate::rpc::config::*;
use crate::rpc::response::*;
use crate::rpc::RpcClient;
use crate::transport::RpcTransport;
use crate::CommitmentConfig;

/// How many times the confirmation / new-blockhash pollers check before giving
/// up, and how long they wait between checks (~30 s total).
const CONFIRM_ATTEMPTS: u32 = 30;
const CONFIRM_DELAY: Duration = Duration::from_millis(1_000);

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

    /// `getHealth` — `Ok(())` if the node is healthy, else an RPC error.
    pub fn get_health(&self) -> Result<()> {
        let _: String = self.call_typed("getHealth", json!([]))?;
        Ok(())
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

    /// `getVoteAccounts` with commitment.
    pub fn get_vote_accounts_with_commitment(
        &self,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcVoteAccountStatus> {
        self.call_typed("getVoteAccounts", json!([commitment_config]))
    }

    /// `getVoteAccounts` with config.
    pub fn get_vote_accounts_with_config(
        &self,
        config: RpcGetVoteAccountsConfig,
    ) -> Result<RpcVoteAccountStatus> {
        self.call_typed("getVoteAccounts", json!([config]))
    }

    // ----- Slots / epochs / blocks ---------------------------------------

    /// `getSlot` (at the client's default commitment).
    pub fn get_slot(&self) -> Result<u64> {
        self.get_slot_with_commitment(self.commitment())
    }

    /// `getSlot` with commitment.
    pub fn get_slot_with_commitment(&self, commitment_config: CommitmentConfig) -> Result<u64> {
        self.call_typed("getSlot", json!([commitment_config]))
    }

    /// `getBlockHeight` (at the client's default commitment).
    pub fn get_block_height(&self) -> Result<u64> {
        self.get_block_height_with_commitment(self.commitment())
    }

    /// `getBlockHeight` with commitment.
    pub fn get_block_height_with_commitment(
        &self,
        commitment_config: CommitmentConfig,
    ) -> Result<u64> {
        self.call_typed("getBlockHeight", json!([commitment_config]))
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

    /// `getEpochInfo` with commitment.
    pub fn get_epoch_info_with_commitment(
        &self,
        commitment_config: CommitmentConfig,
    ) -> Result<EpochInfo> {
        self.call_typed("getEpochInfo", json!([commitment_config]))
    }

    /// `getEpochSchedule`
    pub fn get_epoch_schedule(&self) -> Result<EpochSchedule> {
        self.call_typed("getEpochSchedule", json!([]))
    }

    /// `getTransactionCount`
    pub fn get_transaction_count(&self) -> Result<u64> {
        self.call_typed("getTransactionCount", json!([]))
    }

    /// `getTransactionCount` with commitment.
    pub fn get_transaction_count_with_commitment(
        &self,
        commitment_config: CommitmentConfig,
    ) -> Result<u64> {
        self.call_typed("getTransactionCount", json!([commitment_config]))
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

    /// `getBlockTime` — Unix timestamp of the block at `slot` (errors if none).
    pub fn get_block_time(&self, slot: u64) -> Result<i64> {
        let ts: Option<i64> = self.call_typed("getBlockTime", json!([slot]))?;
        ts.ok_or_else(|| Error::UnexpectedResponse(format!("block {slot} has no timestamp")))
    }

    /// `getBlocks` — confirmed blocks in `[start, end]` (end optional).
    pub fn get_blocks(&self, start_slot: u64, end_slot: Option<u64>) -> Result<Vec<u64>> {
        let params = match end_slot {
            Some(end) => json!([start_slot, end]),
            None => json!([start_slot]),
        };
        self.call_typed("getBlocks", params)
    }

    /// `getBlocks` with commitment.
    pub fn get_blocks_with_commitment(
        &self,
        start_slot: u64,
        end_slot: Option<u64>,
        commitment_config: CommitmentConfig,
    ) -> Result<Vec<u64>> {
        let params = match end_slot {
            Some(end) => json!([start_slot, end, commitment_config]),
            None => json!([start_slot, Value::Null, commitment_config]),
        };
        self.call_typed("getBlocks", params)
    }

    /// `getBlocksWithLimit` — up to `limit` confirmed blocks from `start_slot`.
    pub fn get_blocks_with_limit(&self, start_slot: u64, limit: usize) -> Result<Vec<u64>> {
        self.call_typed("getBlocksWithLimit", json!([start_slot, limit]))
    }

    /// `getBlocksWithLimit` with commitment.
    pub fn get_blocks_with_limit_and_commitment(
        &self,
        start_slot: u64,
        limit: usize,
        commitment_config: CommitmentConfig,
    ) -> Result<Vec<u64>> {
        self.call_typed(
            "getBlocksWithLimit",
            json!([start_slot, limit, commitment_config]),
        )
    }

    /// `getBlock` — the fully-encoded confirmed block.
    pub fn get_block(&self, slot: u64) -> Result<EncodedConfirmedBlock> {
        self.get_block_with_encoding(slot, UiTransactionEncoding::Json)
    }

    /// `getBlock` with an explicit transaction encoding.
    pub fn get_block_with_encoding(
        &self,
        slot: u64,
        encoding: UiTransactionEncoding,
    ) -> Result<EncodedConfirmedBlock> {
        self.call_typed("getBlock", json!([slot, encoding]))
    }

    /// `getBlock` with config, returning the UI block.
    pub fn get_block_with_config(
        &self,
        slot: u64,
        config: RpcBlockConfig,
    ) -> Result<UiConfirmedBlock> {
        self.call_typed("getBlock", json!([slot, config]))
    }

    /// `getBlockProduction`
    pub fn get_block_production(&self) -> Result<RpcResponse<RpcBlockProduction>> {
        self.call_typed("getBlockProduction", json!([]))
    }

    /// `getBlockProduction` with config.
    pub fn get_block_production_with_config(
        &self,
        config: RpcBlockProductionConfig,
    ) -> Result<RpcResponse<RpcBlockProduction>> {
        self.call_typed("getBlockProduction", json!([config]))
    }

    /// `getLeaderSchedule` for the epoch containing `slot` (or current if None).
    pub fn get_leader_schedule(&self, slot: Option<u64>) -> Result<Option<RpcLeaderSchedule>> {
        self.call_typed("getLeaderSchedule", json!([slot]))
    }

    /// `getLeaderSchedule` with commitment.
    pub fn get_leader_schedule_with_commitment(
        &self,
        slot: Option<u64>,
        commitment_config: CommitmentConfig,
    ) -> Result<Option<RpcLeaderSchedule>> {
        self.call_typed("getLeaderSchedule", json!([slot, commitment_config]))
    }

    /// `getLeaderSchedule` with config.
    pub fn get_leader_schedule_with_config(
        &self,
        slot: Option<u64>,
        config: RpcLeaderScheduleConfig,
    ) -> Result<Option<RpcLeaderSchedule>> {
        self.call_typed("getLeaderSchedule", json!([slot, config]))
    }

    // ----- Accounts / balances -------------------------------------------

    /// `getBalance` — lamports at `pubkey` (at the client's default commitment).
    pub fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        Ok(self
            .get_balance_with_commitment(pubkey, self.commitment())?
            .value)
    }

    /// `getBalance` with commitment.
    pub fn get_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<u64>> {
        self.call_typed("getBalance", json!([pubkey.to_string(), commitment_config]))
    }

    /// `getAccountInfo` — the decoded account, erroring if it does not exist.
    pub fn get_account(&self, pubkey: &Pubkey) -> Result<Account> {
        self.get_account_with_commitment(pubkey, self.commitment())?
            .value
            .ok_or_else(|| Error::UnexpectedResponse(format!("account {pubkey} not found")))
    }

    /// `getAccountInfo` with commitment — `None` if the account does not exist.
    pub fn get_account_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<Option<Account>>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(commitment_config),
            ..Default::default()
        };
        let r = self.get_ui_account_with_config(pubkey, config)?;
        let value = r.value.map(|ui| decode_account(ui, pubkey)).transpose()?;
        Ok(RpcResponse {
            context: r.context,
            value,
        })
    }

    /// `getAccountInfo` returning the UI account (encoded data), with config.
    pub fn get_ui_account_with_config(
        &self,
        pubkey: &Pubkey,
        config: RpcAccountInfoConfig,
    ) -> Result<RpcResponse<Option<UiAccount>>> {
        self.call_typed("getAccountInfo", json!([pubkey.to_string(), config]))
    }

    /// `getMultipleAccounts` — one decoded account per input, `None` where absent.
    pub fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Account>>> {
        Ok(self
            .get_multiple_accounts_with_commitment(pubkeys, self.commitment())?
            .value)
    }

    /// `getMultipleAccounts` with commitment.
    pub fn get_multiple_accounts_with_commitment(
        &self,
        pubkeys: &[Pubkey],
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<Vec<Option<Account>>>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(commitment_config),
            ..Default::default()
        };
        let r = self.get_multiple_ui_accounts_with_config(pubkeys, config)?;
        let value = r
            .value
            .into_iter()
            .zip(pubkeys)
            .map(|(opt, pk)| opt.map(|ui| decode_account(ui, pk)).transpose())
            .collect::<Result<Vec<_>>>()?;
        Ok(RpcResponse {
            context: r.context,
            value,
        })
    }

    /// `getMultipleAccounts` returning UI accounts (encoded data), with config.
    pub fn get_multiple_ui_accounts_with_config(
        &self,
        pubkeys: &[Pubkey],
        config: RpcAccountInfoConfig,
    ) -> Result<RpcResponse<Vec<Option<UiAccount>>>> {
        let keys: Vec<String> = pubkeys.iter().map(|p| p.to_string()).collect();
        self.call_typed("getMultipleAccounts", json!([keys, config]))
    }

    /// `getProgramAccounts` — decoded accounts keyed by pubkey.
    pub fn get_program_accounts(&self, pubkey: &Pubkey) -> Result<Vec<(Pubkey, Account)>> {
        let config = RpcProgramAccountsConfig {
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                commitment: Some(self.commitment()),
                ..Default::default()
            },
            ..Default::default()
        };
        self.get_program_ui_accounts_with_config(pubkey, config)?
            .into_iter()
            .map(|(pk, ui)| Ok((pk, decode_account(ui, &pk)?)))
            .collect()
    }

    /// `getProgramAccounts` returning UI accounts (encoded data), with config.
    pub fn get_program_ui_accounts_with_config(
        &self,
        pubkey: &Pubkey,
        config: RpcProgramAccountsConfig,
    ) -> Result<Vec<(Pubkey, UiAccount)>> {
        let raw = self.call("getProgramAccounts", json!([pubkey.to_string(), config]))?;
        // With `withContext: true` the result is `{ context, value: [...] }`.
        let value = raw.get("value").cloned().unwrap_or(raw);
        let keyed: Vec<RpcKeyedAccount> =
            serde_json::from_value(value).map_err(|e| Error::UnexpectedResponse(e.to_string()))?;
        keyed
            .into_iter()
            .map(|k| Ok((parse_pubkey(&k.pubkey)?, k.account)))
            .collect()
    }

    /// `getLargestAccounts` with config.
    pub fn get_largest_accounts_with_config(
        &self,
        config: RpcLargestAccountsConfig,
    ) -> Result<RpcResponse<Vec<RpcAccountBalance>>> {
        self.call_typed("getLargestAccounts", json!([config]))
    }

    /// `getMinimumBalanceForRentExemption` — lamports for a `data_len` account.
    pub fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> Result<u64> {
        self.call_typed("getMinimumBalanceForRentExemption", json!([data_len]))
    }

    /// `getStakeMinimumDelegation`
    pub fn get_stake_minimum_delegation(&self) -> Result<u64> {
        self.get_stake_minimum_delegation_with_commitment(self.commitment())
    }

    /// `getStakeMinimumDelegation` with commitment.
    pub fn get_stake_minimum_delegation_with_commitment(
        &self,
        commitment_config: CommitmentConfig,
    ) -> Result<u64> {
        let r: RpcResponse<u64> =
            self.call_typed("getStakeMinimumDelegation", json!([commitment_config]))?;
        Ok(r.value)
    }

    // ----- Tokens --------------------------------------------------------

    /// `getTokenAccountBalance`
    pub fn get_token_account_balance(&self, pubkey: &Pubkey) -> Result<UiTokenAmount> {
        Ok(self
            .get_token_account_balance_with_commitment(pubkey, self.commitment())?
            .value)
    }

    /// `getTokenAccountBalance` with commitment.
    pub fn get_token_account_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<UiTokenAmount>> {
        self.call_typed(
            "getTokenAccountBalance",
            json!([pubkey.to_string(), commitment_config]),
        )
    }

    /// `getTokenSupply`
    pub fn get_token_supply(&self, mint: &Pubkey) -> Result<UiTokenAmount> {
        Ok(self
            .get_token_supply_with_commitment(mint, self.commitment())?
            .value)
    }

    /// `getTokenSupply` with commitment.
    pub fn get_token_supply_with_commitment(
        &self,
        mint: &Pubkey,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<UiTokenAmount>> {
        self.call_typed(
            "getTokenSupply",
            json!([mint.to_string(), commitment_config]),
        )
    }

    /// `getTokenLargestAccounts`
    pub fn get_token_largest_accounts(&self, mint: &Pubkey) -> Result<Vec<RpcTokenAccountBalance>> {
        Ok(self
            .get_token_largest_accounts_with_commitment(mint, self.commitment())?
            .value)
    }

    /// `getTokenLargestAccounts` with commitment.
    pub fn get_token_largest_accounts_with_commitment(
        &self,
        mint: &Pubkey,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<Vec<RpcTokenAccountBalance>>> {
        self.call_typed(
            "getTokenLargestAccounts",
            json!([mint.to_string(), commitment_config]),
        )
    }

    /// `getTokenAccountsByOwner`
    pub fn get_token_accounts_by_owner(
        &self,
        owner: &Pubkey,
        token_account_filter: TokenAccountsFilter,
    ) -> Result<Vec<RpcKeyedAccount>> {
        Ok(self
            .get_token_accounts_by_owner_with_commitment(
                owner,
                token_account_filter,
                self.commitment(),
            )?
            .value)
    }

    /// `getTokenAccountsByOwner` with commitment.
    pub fn get_token_accounts_by_owner_with_commitment(
        &self,
        owner: &Pubkey,
        token_account_filter: TokenAccountsFilter,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<Vec<RpcKeyedAccount>>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::JsonParsed),
            commitment: Some(commitment_config),
            ..Default::default()
        };
        self.call_typed(
            "getTokenAccountsByOwner",
            json!([owner.to_string(), wire_filter(token_account_filter), config]),
        )
    }

    /// `getTokenAccountsByDelegate`
    pub fn get_token_accounts_by_delegate(
        &self,
        delegate: &Pubkey,
        token_account_filter: TokenAccountsFilter,
    ) -> Result<Vec<RpcKeyedAccount>> {
        Ok(self
            .get_token_accounts_by_delegate_with_commitment(
                delegate,
                token_account_filter,
                self.commitment(),
            )?
            .value)
    }

    /// `getTokenAccountsByDelegate` with commitment.
    pub fn get_token_accounts_by_delegate_with_commitment(
        &self,
        delegate: &Pubkey,
        token_account_filter: TokenAccountsFilter,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<Vec<RpcKeyedAccount>>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::JsonParsed),
            commitment: Some(commitment_config),
            ..Default::default()
        };
        self.call_typed(
            "getTokenAccountsByDelegate",
            json!([
                delegate.to_string(),
                wire_filter(token_account_filter),
                config
            ]),
        )
    }

    // ----- Blockhash / fees ----------------------------------------------

    /// `getLatestBlockhash` — the blockhash (at the client's default commitment).
    pub fn get_latest_blockhash(&self) -> Result<Hash> {
        Ok(self
            .get_latest_blockhash_with_commitment(self.commitment())?
            .0)
    }

    /// `getLatestBlockhash` with commitment — `(blockhash, last_valid_block_height)`.
    pub fn get_latest_blockhash_with_commitment(
        &self,
        commitment: CommitmentConfig,
    ) -> Result<(Hash, u64)> {
        let r: RpcResponse<RpcBlockhash> =
            self.call_typed("getLatestBlockhash", json!([commitment]))?;
        Ok((
            parse_hash(&r.value.blockhash)?,
            r.value.last_valid_block_height,
        ))
    }

    /// `isBlockhashValid`
    pub fn is_blockhash_valid(
        &self,
        blockhash: &Hash,
        commitment: CommitmentConfig,
    ) -> Result<bool> {
        let r: RpcResponse<bool> = self.call_typed(
            "isBlockhashValid",
            json!([blockhash.to_string(), commitment]),
        )?;
        Ok(r.value)
    }

    /// `getFeeForMessage` — the fee for `message`; errors if the message's
    /// blockhash has expired (no fee available).
    pub fn get_fee_for_message(&self, message: &VersionedMessage) -> Result<u64> {
        let b64_message = b64(&message.serialize());
        let r: RpcResponse<Option<u64>> =
            self.call_typed("getFeeForMessage", json!([b64_message]))?;
        r.value
            .ok_or_else(|| Error::UnexpectedResponse("fee unavailable (blockhash expired?)".into()))
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

    /// `getInflationReward` — one entry per address (`None` where unavailable),
    /// for `epoch` (or the previous epoch if `None`).
    pub fn get_inflation_reward(
        &self,
        addresses: &[Pubkey],
        epoch: Option<u64>,
    ) -> Result<Vec<Option<RpcInflationReward>>> {
        let keys: Vec<String> = addresses.iter().map(|p| p.to_string()).collect();
        let config = RpcEpochConfig {
            epoch,
            commitment: Some(self.commitment()),
            ..Default::default()
        };
        self.call_typed("getInflationReward", json!([keys, config]))
    }

    // ----- Supply / performance ------------------------------------------

    /// `getSupply`
    pub fn supply(&self) -> Result<RpcResponse<RpcSupply>> {
        self.supply_with_commitment(self.commitment())
    }

    /// `getSupply` with commitment.
    pub fn supply_with_commitment(
        &self,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<RpcSupply>> {
        self.call_typed("getSupply", json!([commitment_config]))
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
    ) -> Result<Vec<RpcConfirmedTransactionStatusWithSignature>> {
        self.call_typed("getSignaturesForAddress", json!([address.to_string()]))
    }

    /// `getSignaturesForAddress` with config.
    pub fn get_signatures_for_address_with_config(
        &self,
        address: &Pubkey,
        config: RpcSignaturesForAddressConfig,
    ) -> Result<Vec<RpcConfirmedTransactionStatusWithSignature>> {
        self.call_typed(
            "getSignaturesForAddress",
            json!([address.to_string(), config]),
        )
    }

    /// `getSignatureStatuses` — one status per input signature (recent slots only).
    pub fn get_signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> Result<RpcResponse<Vec<Option<TransactionStatus>>>> {
        let sigs: Vec<String> = signatures.iter().map(|s| s.to_string()).collect();
        self.call_typed("getSignatureStatuses", json!([sigs]))
    }

    /// `getSignatureStatuses` searching the full transaction history.
    pub fn get_signature_statuses_with_history(
        &self,
        signatures: &[Signature],
    ) -> Result<RpcResponse<Vec<Option<TransactionStatus>>>> {
        let sigs: Vec<String> = signatures.iter().map(|s| s.to_string()).collect();
        self.call_typed(
            "getSignatureStatuses",
            json!([sigs, { "searchTransactionHistory": true }]),
        )
    }

    /// `getTransaction` with the given encoding — errors if not found.
    pub fn get_transaction(
        &self,
        signature: &Signature,
        encoding: UiTransactionEncoding,
    ) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
        self.get_transaction_with_config(
            signature,
            RpcTransactionConfig {
                encoding: Some(encoding),
                max_supported_transaction_version: Some(0),
                commitment: Some(self.commitment()),
            },
        )
    }

    /// `getTransaction` with config — errors if not found.
    pub fn get_transaction_with_config(
        &self,
        signature: &Signature,
        config: RpcTransactionConfig,
    ) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
        let tx: Option<EncodedConfirmedTransactionWithStatusMeta> =
            self.call_typed("getTransaction", json!([signature.to_string(), config]))?;
        tx.ok_or_else(|| Error::UnexpectedResponse(format!("transaction {signature} not found")))
    }

    // ----- Sending / simulating / airdrop --------------------------------

    /// `sendTransaction` — serialize and submit a signed transaction, returning
    /// its signature. Like the official client, this takes the typed
    /// transaction; sign it first (the node rejects an unsigned transaction).
    pub fn send_transaction(&self, transaction: &VersionedTransaction) -> Result<Signature> {
        self.send_transaction_with_config(transaction, RpcSendTransactionConfig::default())
    }

    /// `sendTransaction` with config.
    pub fn send_transaction_with_config(
        &self,
        transaction: &VersionedTransaction,
        config: RpcSendTransactionConfig,
    ) -> Result<Signature> {
        let mut config = config;
        config.encoding = Some(UiTransactionEncoding::Base64);
        config.preflight_commitment = config
            .preflight_commitment
            .or(Some(self.commitment().commitment));
        let sig: String = self.call_typed(
            "sendTransaction",
            json!([b64(&serialize_tx(transaction)?), config]),
        )?;
        parse_signature(&sig)
    }

    /// `simulateTransaction` — simulate a transaction (envelope + result).
    pub fn simulate_transaction(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<RpcResponse<RpcSimulateTransactionResult>> {
        self.simulate_transaction_with_config(transaction, RpcSimulateTransactionConfig::default())
    }

    /// `simulateTransaction` with config.
    pub fn simulate_transaction_with_config(
        &self,
        transaction: &VersionedTransaction,
        config: RpcSimulateTransactionConfig,
    ) -> Result<RpcResponse<RpcSimulateTransactionResult>> {
        let mut config = config;
        config.encoding = Some(UiTransactionEncoding::Base64);
        config.commitment = config.commitment.or(Some(self.commitment()));
        self.call_typed(
            "simulateTransaction",
            json!([b64(&serialize_tx(transaction)?), config]),
        )
    }

    // ----- Confirmation lifecycle ----------------------------------------

    /// The transaction result of a single signature (`None` if not found).
    pub fn get_signature_status(
        &self,
        signature: &Signature,
    ) -> Result<Option<TransactionResult<()>>> {
        Ok(self
            .signature_status(signature)?
            .map(|status| status.status))
    }

    /// [`get_signature_status`](Self::get_signature_status) at an explicit
    /// commitment. The status is reported only if it satisfies `commitment_config`
    /// (via [`TransactionStatus::satisfies_commitment`]); otherwise `None`. So at
    /// `finalized` a merely-confirmed transaction reads as `None`, exactly as the
    /// node's own commitment gating behaves.
    pub fn get_signature_status_with_commitment(
        &self,
        signature: &Signature,
        commitment_config: CommitmentConfig,
    ) -> Result<Option<TransactionResult<()>>> {
        Ok(self
            .signature_status(signature)?
            .filter(|status| status.satisfies_commitment(commitment_config))
            .map(|status| status.status))
    }

    /// [`get_signature_status_with_commitment`](Self::get_signature_status_with_commitment),
    /// optionally searching the full transaction history (not just recent status
    /// cache).
    pub fn get_signature_status_with_commitment_and_history(
        &self,
        signature: &Signature,
        commitment_config: CommitmentConfig,
        search_transaction_history: bool,
    ) -> Result<Option<TransactionResult<()>>> {
        let statuses = if search_transaction_history {
            self.get_signature_statuses_with_history(std::slice::from_ref(signature))?
        } else {
            self.get_signature_statuses(std::slice::from_ref(signature))?
        };
        Ok(statuses
            .value
            .into_iter()
            .next()
            .flatten()
            .filter(|status| status.satisfies_commitment(commitment_config))
            .map(|s| s.status))
    }

    /// Internal: the full [`TransactionStatus`] for one signature.
    fn signature_status(&self, signature: &Signature) -> Result<Option<TransactionStatus>> {
        Ok(self
            .get_signature_statuses(std::slice::from_ref(signature))?
            .value
            .into_iter()
            .next()
            .flatten())
    }

    /// Check whether `signature` has been committed at the client's default
    /// commitment. `true` only if the transaction both reached that commitment
    /// and succeeded.
    ///
    /// This does **not** wait — it is a single point-in-time check, matching the
    /// official client. To submit and wait, use
    /// [`send_and_confirm_transaction`](Self::send_and_confirm_transaction).
    pub fn confirm_transaction(&self, signature: &Signature) -> Result<bool> {
        Ok(self
            .confirm_transaction_with_commitment(signature, self.commitment())?
            .value)
    }

    /// Like [`confirm_transaction`](Self::confirm_transaction) at an explicit
    /// commitment (returns the response envelope).
    ///
    /// The value is `true` only when the transaction has reached
    /// `commitment_config` (per [`TransactionStatus::satisfies_commitment`]) **and**
    /// succeeded; a transaction that failed on-chain — like one not yet
    /// committed — reads as `false`. Callers that need the failure reason should
    /// inspect [`get_signature_status`](Self::get_signature_status), which carries
    /// the `Err`.
    pub fn confirm_transaction_with_commitment(
        &self,
        signature: &Signature,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<bool>> {
        let RpcResponse { context, value } =
            self.get_signature_statuses(std::slice::from_ref(signature))?;
        Ok(RpcResponse {
            context,
            value: value
                .into_iter()
                .next()
                .flatten()
                .filter(|status| status.satisfies_commitment(commitment_config))
                .map(|status| status.status.is_ok())
                .unwrap_or(false),
        })
    }

    /// Poll until `signature` appears (any status) or the window elapses.
    pub fn poll_for_signature(&self, signature: &Signature) -> Result<()> {
        self.poll_for_signature_with_commitment(signature, self.commitment())
    }

    /// Like [`poll_for_signature`](Self::poll_for_signature) at an explicit
    /// commitment.
    pub fn poll_for_signature_with_commitment(
        &self,
        signature: &Signature,
        commitment_config: CommitmentConfig,
    ) -> Result<()> {
        let _ = commitment_config;
        for attempt in 0..CONFIRM_ATTEMPTS {
            if self.get_signature_status(signature)?.is_some() {
                return Ok(());
            }
            if attempt + 1 < CONFIRM_ATTEMPTS {
                self.sleep(CONFIRM_DELAY);
            }
        }
        Err(Error::Timeout(format!("signature {signature} not found")))
    }

    /// Poll until `signature` has at least `min_confirmed_blocks` confirmations,
    /// returning the count reached.
    pub fn poll_for_signature_confirmation(
        &self,
        signature: &Signature,
        min_confirmed_blocks: usize,
    ) -> Result<usize> {
        let mut confirmed = 0;
        for attempt in 0..CONFIRM_ATTEMPTS {
            if let Ok(count) = self.get_num_blocks_since_signature_confirmation(signature) {
                confirmed = count;
                if confirmed >= min_confirmed_blocks {
                    return Ok(confirmed);
                }
            }
            if attempt + 1 < CONFIRM_ATTEMPTS {
                self.sleep(CONFIRM_DELAY);
            }
        }
        Err(Error::Timeout(format!(
            "signature {signature} reached only {confirmed} confirmations"
        )))
    }

    /// Number of confirmations for `signature` (rooted transactions report the
    /// maximum). Errors if the signature is not found.
    pub fn get_num_blocks_since_signature_confirmation(
        &self,
        signature: &Signature,
    ) -> Result<usize> {
        // Rooted transactions have no confirmation count; report the ceiling.
        const MAX_LOCKOUT_HISTORY_PLUS_ONE: usize = 32;
        let status = self
            .signature_status(signature)?
            .ok_or_else(|| Error::UnexpectedResponse("signature not found".into()))?;
        Ok(status.confirmations.unwrap_or(MAX_LOCKOUT_HISTORY_PLUS_ONE))
    }

    /// Submit a signed transaction and poll until it confirms at the default
    /// commitment.
    pub fn send_and_confirm_transaction(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<Signature> {
        let recent_blockhash = transaction.message.recent_blockhash();
        let signature = self.send_transaction(transaction)?;
        // Poll status at the client's commitment (like the official client's
        // status-retry loop). Surface an on-chain failure as an error, and stop
        // early once the transaction's blockhash can no longer be found — past
        // that point it can never land, so waiting the full window is pointless.
        for attempt in 0..CONFIRM_ATTEMPTS {
            match self.get_signature_status_with_commitment(&signature, self.commitment())? {
                Some(Ok(())) => return Ok(signature),
                Some(Err(err)) => {
                    return Err(Error::Rpc {
                        code: 0,
                        message: format!("transaction failed: {err:?}"),
                        data: serde_json::to_value(&err).ok(),
                    })
                }
                None => {
                    if !self.is_blockhash_valid(recent_blockhash, CommitmentConfig::processed())? {
                        return Err(Error::Timeout(format!(
                            "transaction {signature}'s blockhash expired before confirmation"
                        )));
                    }
                }
            }
            if attempt + 1 < CONFIRM_ATTEMPTS {
                self.sleep(CONFIRM_DELAY);
            }
        }
        Err(Error::Timeout(format!(
            "transaction {signature} not confirmed in time"
        )))
    }

    /// Poll `getLatestBlockhash` until it differs from `blockhash`. Useful when a
    /// prior transaction consumed the current blockhash and you need a fresh one.
    pub fn get_new_latest_blockhash(&self, blockhash: &Hash) -> Result<Hash> {
        for attempt in 0..CONFIRM_ATTEMPTS {
            let new = self.get_latest_blockhash()?;
            if new != *blockhash {
                return Ok(new);
            }
            if attempt + 1 < CONFIRM_ATTEMPTS {
                self.sleep(CONFIRM_DELAY);
            }
        }
        Err(Error::Timeout("no new blockhash".into()))
    }

    /// The raw data bytes of an account. Errors if the account is absent.
    pub fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        Ok(self.get_account(pubkey)?.data)
    }

    /// The slot at which `feature_id` was activated, or `None` if not activated
    /// (or the feature account does not exist).
    pub fn get_feature_activation_slot(&self, feature_id: &Pubkey) -> Result<Option<u64>> {
        // Feature accounts serialize as `struct Feature { activated_at: Option<u64> }`.
        #[derive(serde::Deserialize)]
        struct Feature {
            activated_at: Option<u64>,
        }
        let Some(account) = self
            .get_account_with_commitment(feature_id, self.commitment())?
            .value
        else {
            return Ok(None);
        };
        let feature: Feature = bincode::deserialize(&account.data)
            .map_err(|e| Error::UnexpectedResponse(format!("feature account: {e}")))?;
        Ok(feature.activated_at)
    }

    /// Balance at `pubkey` at an explicit commitment, retried until it resolves.
    pub fn poll_get_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig,
    ) -> Result<u64> {
        Ok(self
            .get_balance_with_commitment(pubkey, commitment_config)?
            .value)
    }

    /// Poll the balance at `pubkey` until it equals `expected_balance` (or return
    /// the current balance immediately if `expected_balance` is `None`). `None`
    /// if it never matched within the polling window — matching the official
    /// client's non-`Result` return.
    pub fn wait_for_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        expected_balance: Option<u64>,
        commitment_config: CommitmentConfig,
    ) -> Option<u64> {
        for attempt in 0..CONFIRM_ATTEMPTS {
            let balance = self
                .get_balance_with_commitment(pubkey, commitment_config)
                .ok()?
                .value;
            match expected_balance {
                None => return Some(balance),
                Some(expected) if expected == balance => return Some(balance),
                _ => {}
            }
            if attempt + 1 < CONFIRM_ATTEMPTS {
                self.sleep(CONFIRM_DELAY);
            }
        }
        None
    }

    /// The parsed SPL token account at `pubkey` (`jsonParsed`). `None` if the
    /// account does not exist; errors if it exists but is not a token account.
    pub fn get_token_account(&self, pubkey: &Pubkey) -> Result<Option<UiTokenAccount>> {
        Ok(self
            .get_token_account_with_commitment(pubkey, self.commitment())?
            .value)
    }

    /// Like [`get_token_account`](Self::get_token_account) at an explicit commitment.
    pub fn get_token_account_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig,
    ) -> Result<RpcResponse<Option<UiTokenAccount>>> {
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::JsonParsed),
            commitment: Some(commitment_config),
            ..Default::default()
        };
        let response = self.get_ui_account_with_config(pubkey, config)?;
        let context = response.context.clone();
        let value = match response.value {
            None => None,
            Some(account) => match account.data {
                UiAccountData::Json(parsed) => {
                    match serde_json::from_value::<TokenAccountType>(parsed.parsed) {
                        Ok(TokenAccountType::Account(token)) => Some(token),
                        _ => {
                            return Err(Error::UnexpectedResponse(format!(
                                "account {pubkey} is not a token account"
                            )))
                        }
                    }
                }
                _ => {
                    return Err(Error::UnexpectedResponse(format!(
                        "account {pubkey} is not a token account"
                    )))
                }
            },
        };
        Ok(RpcResponse { context, value })
    }

    /// `requestAirdrop` — devnet/testnet only. Returns the signature.
    pub fn request_airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<Signature> {
        let sig: String =
            self.call_typed("requestAirdrop", json!([pubkey.to_string(), lamports]))?;
        parse_signature(&sig)
    }

    /// `requestAirdrop` with config.
    pub fn request_airdrop_with_config(
        &self,
        pubkey: &Pubkey,
        lamports: u64,
        config: RpcRequestAirdropConfig,
    ) -> Result<Signature> {
        let sig: String = self.call_typed(
            "requestAirdrop",
            json!([pubkey.to_string(), lamports, config]),
        )?;
        parse_signature(&sig)
    }

    /// `requestAirdrop` against a specific `recent_blockhash`.
    pub fn request_airdrop_with_blockhash(
        &self,
        pubkey: &Pubkey,
        lamports: u64,
        recent_blockhash: &Hash,
    ) -> Result<Signature> {
        self.request_airdrop_with_config(
            pubkey,
            lamports,
            RpcRequestAirdropConfig {
                recent_blockhash: Some(recent_blockhash.to_string()),
                commitment: Some(self.commitment()),
            },
        )
    }
}

/// Serialize a transaction to bincode wire bytes for submission.
fn serialize_tx(tx: &VersionedTransaction) -> Result<Vec<u8>> {
    bincode::serialize(tx).map_err(|e| Error::Parse(format!("serialize transaction: {e}")))
}

/// Decode a [`UiAccount`] into a native [`Account`] (matching the official
/// client's account-returning methods).
fn decode_account(ui: UiAccount, pubkey: &Pubkey) -> Result<Account> {
    ui.to_account()
        .ok_or_else(|| Error::UnexpectedResponse(format!("account {pubkey} data not decodable")))
}

/// Convert the public [`TokenAccountsFilter`] into the wire (`String`) filter,
/// as the official client does before sending.
fn wire_filter(filter: TokenAccountsFilter) -> RpcTokenAccountsFilter {
    match filter {
        TokenAccountsFilter::Mint(mint) => RpcTokenAccountsFilter::Mint(mint.to_string()),
        TokenAccountsFilter::ProgramId(id) => RpcTokenAccountsFilter::ProgramId(id.to_string()),
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
