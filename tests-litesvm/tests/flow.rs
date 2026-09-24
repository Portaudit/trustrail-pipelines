use anchor_lang::prelude::Pubkey;
use sha2::{Digest, Sha256};
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::rent::Rent;
use spl_token::solana_program::program_pack::Pack;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::system_program;
use anchor_lang::{AccountDeserialize, AnchorSerialize};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;

use trustrail_pipelines::state::{Commitment, CommitmentStatus};
use trustrail_pipelines::ID as PROGRAM_ID;

fn sighash(name: &str) -> [u8; 8] {
    let preimage = format!("global:{name}");
    let full = Sha256::digest(preimage.as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&full[..8]);
    out
}

fn ix_data<T: AnchorSerialize>(name: &str, args: T) -> Vec<u8> {
    let mut data = sighash(name).to_vec();
    args.serialize(&mut data).unwrap();
    data
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction, extra_signers: &[&Keypair]) -> Result<(), String> {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend(extra_signers);
    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let mut tx = Transaction::new_unsigned(msg);
    tx.sign(&signers, svm.latest_blockhash());
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

struct Setup {
    svm: LiteSVM,
    payer: Keypair,
    executor: Keypair,
    input_mint: Pubkey,
    output_mint: Pubkey,
    payer_input_ata: Pubkey,
}

fn setup() -> Setup {
    let mut svm = LiteSVM::new();
    // Path is 2 levels up from tests-litesvm/tests/ to the repo root, then
    // into target/deploy/ — same .so anchor build produces in the anchor
    // workspace; this crate's own (separate) build graph never touches it,
    // it's just read as bytes.
    svm.add_program(
        PROGRAM_ID,
        include_bytes!("../../target/deploy/trustrail_pipelines.so"),
    )
    .expect("add_program failed");

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    let executor = Keypair::new();

    let input_mint = Keypair::new();
    let output_mint = Keypair::new();
    let mint_space = spl_token::state::Mint::LEN;
    let mint_rent = Rent::default().minimum_balance(mint_space);

    for mint in [&input_mint, &output_mint] {
        let create_ix = system_instruction::create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            mint_rent,
            mint_space as u64,
            &spl_token::ID,
        );
        let init_ix = spl_token::instruction::initialize_mint2(
            &spl_token::ID,
            &mint.pubkey(),
            &payer.pubkey(),
            None,
            6,
        )
        .unwrap();
        let msg = Message::new(&[create_ix, init_ix], Some(&payer.pubkey()));
        let mut tx = Transaction::new_unsigned(msg);
        tx.sign(&[&payer, mint], svm.latest_blockhash());
        svm.send_transaction(tx).expect("mint init failed");
    }

    let payer_input_ata = get_associated_token_address(&payer.pubkey(), &input_mint.pubkey());
    let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        &payer.pubkey(),
        &input_mint.pubkey(),
        &spl_token::ID,
    );
    let mint_to_ix = spl_token::instruction::mint_to(
        &spl_token::ID,
        &input_mint.pubkey(),
        &payer_input_ata,
        &payer.pubkey(),
        &[],
        1_000_000,
    )
    .unwrap();
    let msg = Message::new(&[create_ata_ix, mint_to_ix], Some(&payer.pubkey()));
    let mut tx = Transaction::new_unsigned(msg);
    tx.sign(&[&payer], svm.latest_blockhash());
    svm.send_transaction(tx).expect("payer_input_ata setup failed");

    Setup { svm, payer, executor, input_mint: input_mint.pubkey(), output_mint: output_mint.pubkey(), payer_input_ata }
}

fn create_commitment(
    s: &mut Setup,
    task_id: [u8; 32],
    input_amount: u64,
    min_output_amount: u64,
) -> Result<(Pubkey, Pubkey), String> {
    let (commitment_pda, _bump) =
        Pubkey::find_program_address(&[b"commitment", &task_id], &PROGRAM_ID);
    let escrow_ata = get_associated_token_address(&commitment_pda, &s.input_mint);
    let output_ata = get_associated_token_address(&commitment_pda, &s.output_mint);

    // create_commitment has taken 6 args since the escrow-withdrawal safety
    // layer (commit 436e494) added expected_staging_ata as the last param.
    // This test file wasn't updated then — a 5-tuple here silently short-fed
    // Anchor's deserializer. These tests never exercise withdraw_for_swap, so
    // any valid Pubkey works as the placeholder; using a fresh unique one
    // rather than Pubkey::default() so it can't collide with a real account.
    let expected_staging_ata = Pubkey::new_unique();
    let data = ix_data(
        "create_commitment",
        (task_id, s.executor.pubkey(), input_amount, min_output_amount, 1_000_000u64, expected_staging_ata),
    );

    let accounts = vec![
        AccountMeta::new(s.payer.pubkey(), true),
        AccountMeta::new_readonly(s.executor.pubkey(), false),
        AccountMeta::new_readonly(s.input_mint, false),
        AccountMeta::new_readonly(s.output_mint, false),
        AccountMeta::new(commitment_pda, false),
        AccountMeta::new(escrow_ata, false),
        AccountMeta::new(output_ata, false),
        AccountMeta::new(s.payer_input_ata, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(spl_token::ID, false),
        AccountMeta::new_readonly(spl_associated_token_account::ID, false),
    ];

    let ix = Instruction { program_id: PROGRAM_ID, accounts, data };
    send(&mut s.svm, &s.payer, ix, &[])?;
    Ok((commitment_pda, output_ata))
}

fn fund_output_ata(s: &mut Setup, output_ata: &Pubkey, amount: u64) {
    let mint_to_ix = spl_token::instruction::mint_to(
        &spl_token::ID,
        &s.output_mint,
        output_ata,
        &s.payer.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    let msg = Message::new(&[mint_to_ix], Some(&s.payer.pubkey()));
    let mut tx = Transaction::new_unsigned(msg);
    tx.sign(&[&s.payer], s.svm.latest_blockhash());
    s.svm.send_transaction(tx).expect("fund output_ata failed");
}

fn submit_proof(
    s: &mut Setup,
    commitment_pda: Pubkey,
    output_ata: Pubkey,
    claimed_actual_output: u64,
) -> Result<(), String> {
    let data = ix_data("submit_proof", ([9u8; 64], 42u64, claimed_actual_output));
    let accounts = vec![
        AccountMeta::new_readonly(s.executor.pubkey(), true),
        AccountMeta::new(commitment_pda, false),
        AccountMeta::new_readonly(output_ata, false),
    ];
    let ix = Instruction { program_id: PROGRAM_ID, accounts, data };
    send(&mut s.svm, &s.payer, ix, &[&s.executor])
}

fn load_commitment(s: &Setup, commitment_pda: &Pubkey) -> Commitment {
    let acct = s.svm.get_account(commitment_pda).expect("commitment account missing");
    Commitment::try_deserialize(&mut acct.data.as_slice()).expect("deserialize failed")
}

#[test]
fn create_commitment_happy_path() {
    let mut s = setup();
    let (commitment_pda, _output_ata) = create_commitment(&mut s, [1u8; 32], 1000, 500).unwrap();
    let c = load_commitment(&s, &commitment_pda);
    assert_eq!(c.status, CommitmentStatus::Locked);
    assert_eq!(c.input_amount, 1000);
    assert_eq!(c.min_output_amount, 500);
}

#[test]
fn create_commitment_rejects_zero_amount() {
    let mut s = setup();
    let err = create_commitment(&mut s, [2u8; 32], 0, 500).unwrap_err();
    assert!(err.contains("WrongState") || err.to_lowercase().contains("wrongstate"), "got: {err}");
}

#[test]
fn submit_proof_happy_path_passed() {
    let mut s = setup();
    let (commitment_pda, output_ata) = create_commitment(&mut s, [3u8; 32], 1000, 500).unwrap();
    fund_output_ata(&mut s, &output_ata, 600);
    submit_proof(&mut s, commitment_pda, output_ata, 600).unwrap();
    let c = load_commitment(&s, &commitment_pda);
    assert_eq!(c.status, CommitmentStatus::Passed);
    assert!(c.verified);
    assert_eq!(c.proof_slot, Some(42));
}

#[test]
fn submit_proof_failed_slippage() {
    let mut s = setup();
    let (commitment_pda, output_ata) = create_commitment(&mut s, [4u8; 32], 1000, 500).unwrap();
    fund_output_ata(&mut s, &output_ata, 100);
    submit_proof(&mut s, commitment_pda, output_ata, 100).unwrap();
    let c = load_commitment(&s, &commitment_pda);
    assert_eq!(c.status, CommitmentStatus::FailedSlippage);
}

#[test]
fn submit_proof_rejects_claim_mismatch() {
    let mut s = setup();
    let (commitment_pda, output_ata) = create_commitment(&mut s, [5u8; 32], 1000, 500).unwrap();
    fund_output_ata(&mut s, &output_ata, 600);
    let err = submit_proof(&mut s, commitment_pda, output_ata, 999).unwrap_err();
    assert!(err.to_lowercase().contains("claimmismatch"), "got: {err}");
}

#[test]
fn submit_proof_second_call_rejected() {
    let mut s = setup();
    let (commitment_pda, output_ata) = create_commitment(&mut s, [6u8; 32], 1000, 500).unwrap();
    fund_output_ata(&mut s, &output_ata, 600);
    submit_proof(&mut s, commitment_pda, output_ata, 600).unwrap();

    // Ed25519 signing is deterministic: identical instruction + identical
    // blockhash produces byte-identical message and therefore an identical
    // signature. litesvm 0.16's transaction-history dedup then rejects the
    // second send as AlreadyProcessed before it ever reaches the program —
    // that's a litesvm-level replay guard, not the on-chain WrongState guard
    // this test is actually meant to exercise. Expiring the blockhash first
    // makes the second transaction genuinely distinct, so it reaches
    // submit_proof's own status check instead of being caught earlier.
    s.svm.expire_blockhash();
    let err = submit_proof(&mut s, commitment_pda, output_ata, 600).unwrap_err();
    assert!(err.to_lowercase().contains("wrongstate"), "got: {err}");
}
