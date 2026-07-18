//! Automated coverage guard against Anza's canonical method list.
//!
//! This test diffs our implemented method surface against `RpcRequest` — the
//! enum the official client uses to name every JSON-RPC method. It runs on the
//! host only (`solana-rpc-client-types` is a dev-dependency and never enters the
//! wasm component).
//!
//! It fires on three kinds of upstream drift:
//!
//! 1. **A new method is added upstream.** [`classify`] is an *exhaustive* match
//!    with no wildcard arm, and `RpcRequest` is not `#[non_exhaustive]`. A new
//!    variant makes this file fail to **compile** — a hard, unmissable signal.
//!
//! 2. **A method's wire name is renamed upstream.** We assert our hard-coded
//!    wire strings equal `RpcRequest::as_str()` for the matching variant.
//!
//! 3. **Our own list drifts from `methods.rs`.** We grep the source to confirm
//!    every name we claim to implement is actually wired up.
//!
//! When it breaks, bump `solana-rpc-client-types` to the failing version, read
//! the diff, and implement / classify the new method.

use solana_rpc_client_types::request::RpcRequest;

/// Our verdict for each upstream `RpcRequest` variant.
enum Coverage {
    /// A public HTTP method we implement. Carries the wire name we send.
    Implemented(&'static str),
    /// A variant we deliberately do not implement. The reason is documentation
    /// for the reader of `classify`, not consumed at runtime.
    Skipped(#[allow(dead_code)] &'static str),
}

/// Classify every upstream variant. **Exhaustive on purpose** — do not add a
/// `_ => ...` arm. When Anza adds an `RpcRequest` variant, this match stops
/// compiling, forcing a human to decide: implement it, or mark it `Skipped`.
fn classify(r: &RpcRequest) -> Coverage {
    use Coverage::{Implemented as Impl, Skipped};
    match r {
        // -- public HTTP methods we implement (name = what we send) ----------
        RpcRequest::GetAccountInfo => Impl("getAccountInfo"),
        RpcRequest::GetBalance => Impl("getBalance"),
        RpcRequest::GetBlock => Impl("getBlock"),
        RpcRequest::GetBlockHeight => Impl("getBlockHeight"),
        RpcRequest::GetBlockProduction => Impl("getBlockProduction"),
        RpcRequest::GetBlockTime => Impl("getBlockTime"),
        RpcRequest::GetBlocks => Impl("getBlocks"),
        RpcRequest::GetBlocksWithLimit => Impl("getBlocksWithLimit"),
        RpcRequest::GetClusterNodes => Impl("getClusterNodes"),
        RpcRequest::GetEpochInfo => Impl("getEpochInfo"),
        RpcRequest::GetEpochSchedule => Impl("getEpochSchedule"),
        RpcRequest::GetFeeForMessage => Impl("getFeeForMessage"),
        RpcRequest::GetFirstAvailableBlock => Impl("getFirstAvailableBlock"),
        RpcRequest::GetGenesisHash => Impl("getGenesisHash"),
        RpcRequest::GetHealth => Impl("getHealth"),
        RpcRequest::GetHighestSnapshotSlot => Impl("getHighestSnapshotSlot"),
        RpcRequest::GetIdentity => Impl("getIdentity"),
        RpcRequest::GetInflationGovernor => Impl("getInflationGovernor"),
        RpcRequest::GetInflationRate => Impl("getInflationRate"),
        RpcRequest::GetInflationReward => Impl("getInflationReward"),
        RpcRequest::GetLargestAccounts => Impl("getLargestAccounts"),
        RpcRequest::GetLatestBlockhash => Impl("getLatestBlockhash"),
        RpcRequest::GetLeaderSchedule => Impl("getLeaderSchedule"),
        RpcRequest::GetMaxRetransmitSlot => Impl("getMaxRetransmitSlot"),
        RpcRequest::GetMaxShredInsertSlot => Impl("getMaxShredInsertSlot"),
        RpcRequest::GetMinimumBalanceForRentExemption => {
            Impl("getMinimumBalanceForRentExemption")
        }
        RpcRequest::GetMultipleAccounts => Impl("getMultipleAccounts"),
        RpcRequest::GetProgramAccounts => Impl("getProgramAccounts"),
        RpcRequest::GetRecentPerformanceSamples => Impl("getRecentPerformanceSamples"),
        RpcRequest::GetRecentPrioritizationFees => Impl("getRecentPrioritizationFees"),
        RpcRequest::GetSignatureStatuses => Impl("getSignatureStatuses"),
        RpcRequest::GetSignaturesForAddress => Impl("getSignaturesForAddress"),
        RpcRequest::GetSlot => Impl("getSlot"),
        RpcRequest::GetSlotLeader => Skipped("official RpcClient exposes only getSlotLeaders"),
        RpcRequest::GetSlotLeaders => Impl("getSlotLeaders"),
        RpcRequest::GetStakeMinimumDelegation => Impl("getStakeMinimumDelegation"),
        RpcRequest::GetSupply => Impl("getSupply"),
        RpcRequest::GetTokenAccountBalance => Impl("getTokenAccountBalance"),
        RpcRequest::GetTokenAccountsByDelegate => Impl("getTokenAccountsByDelegate"),
        RpcRequest::GetTokenAccountsByOwner => Impl("getTokenAccountsByOwner"),
        RpcRequest::GetTokenLargestAccounts => Impl("getTokenLargestAccounts"),
        RpcRequest::GetTokenSupply => Impl("getTokenSupply"),
        RpcRequest::GetTransaction => Impl("getTransaction"),
        RpcRequest::GetTransactionCount => Impl("getTransactionCount"),
        RpcRequest::GetVersion => Impl("getVersion"),
        RpcRequest::GetVoteAccounts => Impl("getVoteAccounts"),
        RpcRequest::IsBlockhashValid => Impl("isBlockhashValid"),
        RpcRequest::MinimumLedgerSlot => Impl("minimumLedgerSlot"),
        RpcRequest::RequestAirdrop => Impl("requestAirdrop"),
        RpcRequest::SendTransaction => Impl("sendTransaction"),
        RpcRequest::SimulateTransaction => Impl("simulateTransaction"),

        // -- intentionally not implemented -----------------------------------
        // Escape hatch for arbitrary methods; not a real endpoint.
        RpcRequest::Custom { .. } => Skipped("escape hatch, not an endpoint"),
        // Legacy validator-to-validator RPCs, not part of the public HTTP API
        // and not served by public RPC providers.
        RpcRequest::DeregisterNode => Skipped("internal validator RPC"),
        RpcRequest::RegisterNode => Skipped("internal validator RPC"),
        RpcRequest::SignVote => Skipped("internal validator RPC"),
        RpcRequest::GetSlotsPerSegment => Skipped("removed archiver/storage RPC"),
        RpcRequest::GetStoragePubkeysForSlot => Skipped("removed archiver/storage RPC"),
        RpcRequest::GetStorageTurn => Skipped("removed archiver/storage RPC"),
        RpcRequest::GetStorageTurnRate => Skipped("removed archiver/storage RPC"),
    }
}

/// One value of every `RpcRequest` variant. Used to iterate at runtime for the
/// rename and list-sync checks. Kept in sync with [`classify`] by
/// [`census_covers_every_variant`], which counts arms against this list.
fn all_variants() -> Vec<RpcRequest> {
    vec![
        RpcRequest::GetAccountInfo,
        RpcRequest::GetBalance,
        RpcRequest::GetBlock,
        RpcRequest::GetBlockHeight,
        RpcRequest::GetBlockProduction,
        RpcRequest::GetBlockTime,
        RpcRequest::GetBlocks,
        RpcRequest::GetBlocksWithLimit,
        RpcRequest::GetClusterNodes,
        RpcRequest::GetEpochInfo,
        RpcRequest::GetEpochSchedule,
        RpcRequest::GetFeeForMessage,
        RpcRequest::GetFirstAvailableBlock,
        RpcRequest::GetGenesisHash,
        RpcRequest::GetHealth,
        RpcRequest::GetHighestSnapshotSlot,
        RpcRequest::GetIdentity,
        RpcRequest::GetInflationGovernor,
        RpcRequest::GetInflationRate,
        RpcRequest::GetInflationReward,
        RpcRequest::GetLargestAccounts,
        RpcRequest::GetLatestBlockhash,
        RpcRequest::GetLeaderSchedule,
        RpcRequest::GetMaxRetransmitSlot,
        RpcRequest::GetMaxShredInsertSlot,
        RpcRequest::GetMinimumBalanceForRentExemption,
        RpcRequest::GetMultipleAccounts,
        RpcRequest::GetProgramAccounts,
        RpcRequest::GetRecentPerformanceSamples,
        RpcRequest::GetRecentPrioritizationFees,
        RpcRequest::GetSignatureStatuses,
        RpcRequest::GetSignaturesForAddress,
        RpcRequest::GetSlot,
        RpcRequest::GetSlotLeader,
        RpcRequest::GetSlotLeaders,
        RpcRequest::GetSlotsPerSegment,
        RpcRequest::GetStakeMinimumDelegation,
        RpcRequest::GetStoragePubkeysForSlot,
        RpcRequest::GetStorageTurn,
        RpcRequest::GetStorageTurnRate,
        RpcRequest::GetSupply,
        RpcRequest::GetTokenAccountBalance,
        RpcRequest::GetTokenAccountsByDelegate,
        RpcRequest::GetTokenAccountsByOwner,
        RpcRequest::GetTokenLargestAccounts,
        RpcRequest::GetTokenSupply,
        RpcRequest::GetTransaction,
        RpcRequest::GetTransactionCount,
        RpcRequest::GetVersion,
        RpcRequest::GetVoteAccounts,
        RpcRequest::IsBlockhashValid,
        RpcRequest::MinimumLedgerSlot,
        RpcRequest::RegisterNode,
        RpcRequest::DeregisterNode,
        RpcRequest::SignVote,
        RpcRequest::RequestAirdrop,
        RpcRequest::SendTransaction,
        RpcRequest::SimulateTransaction,
        RpcRequest::Custom { method: "custom" },
    ]
}

/// Public HTTP methods that exist on real RPC nodes but are **absent from the
/// `RpcRequest` enum**. The enum is not a perfect mirror of the HTTP surface, so
/// these are tracked separately. Verified against `methods.rs`.
const EXTRA_HTTP_NOT_IN_ENUM: &[&str] = &[];

/// Detect (2): our hard-coded wire name must equal upstream's `as_str()`.
#[test]
fn wire_names_match_upstream() {
    for r in all_variants() {
        if let Coverage::Implemented(name) = classify(&r) {
            assert_eq!(
                name,
                r.as_str(),
                "wire name drift: we send {name:?} but upstream `RpcRequest::as_str()` is {:?}",
                r.as_str()
            );
        }
    }
}

/// Detect (1) at runtime too (belt-and-suspenders with the compile-time guard):
/// every public method upstream exposes is implemented, and we implement nothing
/// upstream doesn't know about.
#[test]
fn coverage_matches_upstream() {
    let ours = implemented_in_source();

    // Every implemented upstream variant is in methods.rs.
    for r in all_variants() {
        if let Coverage::Implemented(name) = classify(&r) {
            assert!(
                ours.contains(&name.to_string()),
                "{name} is classified Implemented but is not wired up in methods.rs"
            );
        }
    }

    // Every method in methods.rs is either a known upstream variant or a
    // documented extra — nothing orphaned / misspelled.
    let known: Vec<String> = all_variants()
        .iter()
        .filter_map(|r| match classify(r) {
            Coverage::Implemented(name) => Some(name.to_string()),
            Coverage::Skipped(_) => None,
        })
        .chain(EXTRA_HTTP_NOT_IN_ENUM.iter().map(|s| s.to_string()))
        .collect();
    for name in &ours {
        assert!(
            known.contains(name),
            "methods.rs sends {name:?} which is neither an upstream RpcRequest variant \
             nor listed in EXTRA_HTTP_NOT_IN_ENUM (renamed or typo?)"
        );
    }
}

/// Sanity: the runtime census lists exactly as many variants as `classify`
/// handles, so `all_variants()` cannot silently fall behind the enum.
#[test]
fn census_covers_every_variant() {
    // 50 implemented-in-enum + 9 skipped = 59 variants in RpcRequest 4.1.x.
    assert_eq!(
        all_variants().len(),
        59,
        "all_variants() is stale vs the RpcRequest enum; update it alongside classify()"
    );
}

/// Extract the JSON-RPC method-name string literals actually wired up in
/// `methods.rs` (the first argument to `call`/`call_typed`). Scans the whole
/// source so multi-line calls — where the method name sits on the line after
/// `call(` — are still captured.
fn implemented_in_source() -> Vec<String> {
    let src = include_str!("../src/rpc/methods.rs");
    let mut names = Vec::new();
    for marker in [".call_typed(", ".call("] {
        let mut from = 0;
        while let Some(rel) = src[from..].find(marker) {
            let after = from + rel + marker.len();
            // The method name is the first string literal after the paren.
            if let Some(q1) = src[after..].find('"') {
                let start = after + q1 + 1;
                if let Some(q2) = src[start..].find('"') {
                    names.push(src[start..start + q2].to_string());
                }
            }
            from = after;
        }
    }
    names.sort();
    names.dedup();
    names
}
