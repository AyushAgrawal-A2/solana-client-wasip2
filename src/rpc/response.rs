//! Response payloads.
//!
//! Re-exported from Anza's official crates (pinned in `Cargo.toml`) so the
//! deserialized shapes are, by construction, identical to what the official
//! client parses — no hand-maintained copies to drift. Any upstream change
//! surfaces as a compile error on a deliberate version bump.

// The `{ context, value }` envelope. Anza calls it `Response<T>`; we keep the
// name `RpcResponse<T>` used throughout our method layer.
pub use solana_rpc_client_types::response::{
    Response as RpcResponse, RpcAccountBalance, RpcApiVersion, RpcBlockCommitment,
    RpcBlockProduction, RpcBlockProductionRange, RpcBlockhash,
    RpcConfirmedTransactionStatusWithSignature, RpcContactInfo, RpcIdentity, RpcInflationGovernor,
    RpcInflationRate, RpcInflationReward, RpcKeyedAccount, RpcLeaderSchedule, RpcPerfSample,
    RpcPrioritizationFee, RpcResponseContext, RpcSimulateTransactionResult, RpcSnapshotSlotInfo,
    RpcSupply, RpcTokenAccountBalance, RpcVersionInfo, RpcVoteAccountInfo, RpcVoteAccountStatus,
};

pub use solana_account_decoder_client_types::token::UiTokenAmount;
pub use solana_account_decoder_client_types::{UiAccount, UiAccountData};

pub use solana_transaction_status_client_types::{
    EncodedConfirmedBlock, EncodedConfirmedTransactionWithStatusMeta,
    TransactionConfirmationStatus, TransactionStatus, UiConfirmedBlock,
};

pub use solana_epoch_info::EpochInfo;
pub use solana_epoch_schedule::EpochSchedule;
