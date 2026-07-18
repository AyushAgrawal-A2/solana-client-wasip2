//! Proves the upstream SDK crates (added directly, as any consumer would) build,
//! sign, and serialize a versioned transaction on the host and interoperate with
//! the `message`/`transaction`/`hash` types this crate re-exports — with no
//! wrappers of our own. Doubles as a worked example of the "bring your own SDK
//! crates" workflow the crate docs describe.

use solana_client_wasip2::{
    hash::Hash, message::v0, message::VersionedMessage, pubkey::Pubkey, signature::Signature,
    transaction::versioned::VersionedTransaction,
};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use spl_associated_token_account_interface as spl_associated_token_account;
use spl_memo_interface as spl_memo;
use spl_token_interface as spl_token;

/// Build an unsigned v0 transaction with placeholder signatures — no key needed.
#[test]
fn unsigned_v0_from_reexports() {
    let payer = Keypair::new();
    let to = Pubkey::new_unique();
    let ix = system_instruction::transfer(&payer.pubkey(), &to, 1_000_000);

    let msg = VersionedMessage::V0(
        v0::Message::try_compile(&payer.pubkey(), &[ix], &[], Hash::default()).unwrap(),
    );
    let num_sigs = msg.header().num_required_signatures as usize;
    let tx = VersionedTransaction {
        signatures: vec![Signature::default(); num_sigs],
        message: msg,
    };

    let wire = bincode::serialize(&tx).unwrap();
    assert!(!wire.is_empty());
    assert_eq!(tx.signatures.len(), 1);
    assert_eq!(tx.signatures[0], Signature::default()); // unsigned
}

/// Sign with a caller-supplied key via the upstream `try_new`.
#[test]
fn signed_v0_from_reexports() {
    let payer = Keypair::new();
    let ix = system_instruction::transfer(&payer.pubkey(), &Pubkey::new_unique(), 5_000);
    let msg = VersionedMessage::V0(
        v0::Message::try_compile(&payer.pubkey(), &[ix], &[], Hash::default()).unwrap(),
    );

    let tx = VersionedTransaction::try_new(msg, &[&payer]).unwrap();
    assert_ne!(tx.signatures[0], Signature::default()); // actually signed
}

/// The full SPL surface — token transfer + ATA + memo + priority fee — from the
/// conventionally-named re-exports (`spl_token`, `spl_associated_token_account`, …).
#[test]
fn spl_transfer_from_reexports() {
    let payer = Keypair::new();
    let owner = payer.pubkey();
    let mint = Pubkey::new_unique();
    let dest_wallet = Pubkey::new_unique();
    let token_id = spl_token::id();

    let src_ata = spl_associated_token_account::address::get_associated_token_address(&owner, &mint);
    let dest_ata = spl_associated_token_account::address::get_associated_token_address(&dest_wallet, &mint);

    let ixs = vec![
        ComputeBudgetInstruction::set_compute_unit_price(1_000),
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &owner,
            &dest_wallet,
            &mint,
            &token_id,
        ),
        spl_token::instruction::transfer_checked(
            &token_id, &src_ata, &mint, &dest_ata, &owner, &[], 1_000_000, 6,
        )
        .unwrap(),
        spl_memo::instruction::build_memo(&spl_memo::v3::id(), b"invoice-412", &[&owner]),
    ];

    let msg =
        VersionedMessage::V0(v0::Message::try_compile(&owner, &ixs, &[], Hash::default()).unwrap());
    let tx = VersionedTransaction::try_new(msg, &[&payer]).unwrap();
    assert_ne!(tx.signatures[0], Signature::default());
    if let VersionedMessage::V0(m) = &tx.message {
        assert_eq!(m.instructions.len(), 4);
    } else {
        panic!("expected v0 message");
    }
}
