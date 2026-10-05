use anchor_lang::prelude::Pubkey;
use sha2::{Digest, Sha256};
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::rent::Rent;
use spl_token::solana_program::program_pack::Pack;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::system_program;
use anchor_lang::{AccountDeserialize, AccountSerialize, AnchorSerialize};
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
    create_commitment_with_staging(s, task_id, input_amount, min_output_amount, Pubkey::new_unique())
}

fn create_commitment_with_staging(
    s: &mut Setup,
    task_id: [u8; 32],
    input_amount: u64,
    min_output_amount: u64,
    expected_staging_ata: Pubkey,
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

// ---------------------------------------------------------------------------
// recover (Slice 1, branch 2) helpers and tests
// ---------------------------------------------------------------------------

struct Fixture {
    pda: Pubkey,
    escrow_ata: Pubkey,
    output_ata: Pubkey,
    staging_kp: Keypair,
}

fn is_closed(s: &Setup, addr: &Pubkey) -> bool {
    match s.svm.get_account(addr) {
        None => true,
        Some(a) => a.lamports == 0,
    }
}

fn token_amount(s: &Setup, addr: &Pubkey) -> u64 {
    let acct = s.svm.get_account(addr).expect("token account missing");
    spl_token::state::Account::unpack(&acct.data).expect("unpack failed").amount
}

fn set_token_amount(s: &mut Setup, addr: &Pubkey, amount: u64) {
    let mut acct = s.svm.get_account(addr).expect("token account missing");
    let mut ta = spl_token::state::Account::unpack(&acct.data).expect("unpack failed");
    ta.amount = amount;
    spl_token::state::Account::pack(ta, &mut acct.data).expect("pack failed");
    s.svm.set_account(*addr, acct).expect("set_account failed");
}

// Rewrites status and escrow_withdrawn directly in the commitment account.
fn force_state(s: &mut Setup, pda: &Pubkey, status: CommitmentStatus, escrow_withdrawn: bool) {
    let mut acct = s.svm.get_account(pda).expect("commitment account missing");
    let mut c = Commitment::try_deserialize(&mut acct.data.as_slice()).expect("deserialize failed");
    c.status = status;
    c.escrow_withdrawn = escrow_withdrawn;
    let mut buf: Vec<u8> = Vec::new();
    c.try_serialize(&mut buf).expect("serialize failed");
    assert!(buf.len() <= acct.data.len());
    acct.data.iter_mut().for_each(|b| *b = 0);
    acct.data[..buf.len()].copy_from_slice(&buf);
    s.svm.set_account(*pda, acct).expect("set_account failed");
}

fn new_settler(s: &mut Setup) -> Keypair {
    let k = Keypair::new();
    s.svm.airdrop(&k.pubkey(), 1_000_000_000).unwrap();
    k
}

// Call once per test: a second identical call in the same blockhash would be
// rejected by litesvm's replay guard.
fn ensure_payer_output_ata(s: &mut Setup) {
    let ix = spl_associated_token_account::instruction::create_associated_token_account_idempotent(
        &s.payer.pubkey(),
        &s.payer.pubkey(),
        &s.output_mint,
        &spl_token::ID,
    );
    send(&mut s.svm, &s.payer, ix, &[]).expect("payer_output_ata setup failed");
}

fn payer_output_ata(s: &Setup) -> Pubkey {
    get_associated_token_address(&s.payer.pubkey(), &s.output_mint)
}

// Plain SPL token account of the input mint, like the worker's staging account.
fn make_staging(s: &mut Setup, staging: &Keypair, owner: &Pubkey, amount: u64) {
    let space = spl_token::state::Account::LEN;
    let rent = Rent::default().minimum_balance(space);
    let create_ix = system_instruction::create_account(
        &s.payer.pubkey(), &staging.pubkey(), rent, space as u64, &spl_token::ID,
    );
    let init_ix = spl_token::instruction::initialize_account3(
        &spl_token::ID, &staging.pubkey(), &s.input_mint, owner,
    )
    .unwrap();
    let mut ixs = vec![create_ix, init_ix];
    if amount > 0 {
        ixs.push(
            spl_token::instruction::mint_to(
                &spl_token::ID, &s.input_mint, &staging.pubkey(), &s.payer.pubkey(), &[], amount,
            )
            .unwrap(),
        );
    }
    let msg = Message::new(&ixs, Some(&s.payer.pubkey()));
    let mut tx = Transaction::new_unsigned(msg);
    tx.sign(&[&s.payer, staging], s.svm.latest_blockhash());
    s.svm.send_transaction(tx).expect("make_staging failed");
}

fn approve_delegate(s: &mut Setup, staging: &Pubkey, owner: &Keypair, delegate: &Pubkey, amount: u64) {
    let ix = spl_token::instruction::approve(
        &spl_token::ID, staging, delegate, &owner.pubkey(), &[], amount,
    )
    .unwrap();
    send(&mut s.svm, &s.payer, ix, &[owner]).expect("approve failed");
}

// Real create_commitment + real submit_proof (output below the floor), so the
// commitment is genuinely FailedSlippage with real proof fields. Input is 100_000
// so several fixtures fit in the payer's 1_000_000.
fn stuck_fixture(s: &mut Setup, task_id: [u8; 32], output_amount: u64) -> Fixture {
    let staging_kp = Keypair::new();
    let (pda, output_ata) =
        create_commitment_with_staging(s, task_id, 100_000, 500_000, staging_kp.pubkey()).unwrap();
    let escrow_ata = get_associated_token_address(&pda, &s.input_mint);
    if output_amount > 0 {
        fund_output_ata(s, &output_ata, output_amount);
    }
    submit_proof(s, pda, output_ata, output_amount).unwrap();
    Fixture { pda, escrow_ata, output_ata, staging_kp }
}

// Run B shape: FailedSlippage, escrow withdrawn (escrow 0), staging empty,
// output holds the proceeds.
fn run_b_shaped(s: &mut Setup, task_id: [u8; 32]) -> Fixture {
    let f = stuck_fixture(s, task_id, 300_000);
    make_staging(s, &f.staging_kp, &Pubkey::new_unique(), 0);
    set_token_amount(s, &f.escrow_ata, 0);
    force_state(s, &f.pda, CommitmentStatus::FailedSlippage, true);
    f
}

fn recover(s: &mut Setup, settler: &Keypair, f: &Fixture) -> Result<(), String> {
    let accounts = vec![
        AccountMeta::new_readonly(settler.pubkey(), true),
        AccountMeta::new(f.pda, false),
        AccountMeta::new(s.payer.pubkey(), false),
        AccountMeta::new(f.escrow_ata, false),
        AccountMeta::new(f.output_ata, false),
        AccountMeta::new(f.staging_kp.pubkey(), false),
        AccountMeta::new(s.payer_input_ata, false),
        AccountMeta::new(payer_output_ata(s), false),
        AccountMeta::new_readonly(spl_token::ID, false),
    ];
    let ix = Instruction { program_id: PROGRAM_ID, accounts, data: sighash("recover").to_vec() };
    send(&mut s.svm, settler, ix, &[])
}

// M1
#[test]
fn recover_run_b_shaped_happy_path() {
    let mut s = setup();
    ensure_payer_output_ata(&mut s);
    let f = run_b_shaped(&mut s, [11u8; 32]);
    let settler = new_settler(&mut s);

    let out_ata = payer_output_ata(&s);
    let out_before = token_amount(&s, &out_ata);
    let in_before = token_amount(&s, &s.payer_input_ata);

    recover(&mut s, &settler, &f).unwrap();

    assert_eq!(token_amount(&s, &out_ata), out_before + 300_000);
    assert_eq!(token_amount(&s, &s.payer_input_ata), in_before);
    assert!(is_closed(&s, &f.escrow_ata));
    assert!(is_closed(&s, &f.output_ata));
    assert_eq!(token_amount(&s, &f.staging_kp.pubkey()), 0);
    assert_eq!(load_commitment(&s, &f.pda).status, CommitmentStatus::Recovered);
}

// M2 (see note: the closed ATAs make Anchor reject before the handler runs)
#[test]
fn recover_twice_fails() {
    let mut s = setup();
    ensure_payer_output_ata(&mut s);
    let f = run_b_shaped(&mut s, [12u8; 32]);
    let settler = new_settler(&mut s);
    recover(&mut s, &settler, &f).unwrap();

    s.svm.expire_blockhash();
    let err = recover(&mut s, &settler, &f).unwrap_err();
    let e = err.to_lowercase();
    assert!(e.contains("accountnotinitialized") || e.contains("wrongstate"), "got: {err}");
    assert_eq!(load_commitment(&s, &f.pda).status, CommitmentStatus::Recovered);
}

// M3
#[test]
fn recover_rejects_other_states() {
    let mut s = setup();
    ensure_payer_output_ata(&mut s);
    let settler = new_settler(&mut s);
    let cases = [
        (CommitmentStatus::Passed, false),
        (CommitmentStatus::Passed, true),
        (CommitmentStatus::Released, true),
        (CommitmentStatus::Refunded, false),
        (CommitmentStatus::TimedOut, false),
        (CommitmentStatus::Recovered, true),
        (CommitmentStatus::Locked, true), // S1 gap: stays WrongState until Slice 2
    ];
    for (i, (status, withdrawn)) in cases.iter().enumerate() {
        let f = stuck_fixture(&mut s, [30 + i as u8; 32], 300_000);
        make_staging(&mut s, &f.staging_kp, &Pubkey::new_unique(), 0);
        force_state(&mut s, &f.pda, *status, *withdrawn);
        let err = recover(&mut s, &settler, &f).unwrap_err();
        assert!(err.to_lowercase().contains("wrongstate"), "case {i} {status:?}/{withdrawn}: {err}");
        let c = load_commitment(&s, &f.pda);
        assert_eq!(c.status, *status);
        assert_eq!(token_amount(&s, &f.output_ata), 300_000);
    }
}

// M4
#[test]
fn recover_rejects_failed_slippage_not_withdrawn() {
    let mut s = setup();
    ensure_payer_output_ata(&mut s);
    let f = stuck_fixture(&mut s, [40u8; 32], 300_000);
    make_staging(&mut s, &f.staging_kp, &Pubkey::new_unique(), 0);
    force_state(&mut s, &f.pda, CommitmentStatus::FailedSlippage, false);
    let settler = new_settler(&mut s);

    let err = recover(&mut s, &settler, &f).unwrap_err();
    assert!(err.to_lowercase().contains("wrongstate"), "got: {err}");
    assert_eq!(token_amount(&s, &f.escrow_ata), 100_000);
    assert_eq!(load_commitment(&s, &f.pda).status, CommitmentStatus::FailedSlippage);
}

// M5
#[test]
fn recover_rejects_unpullable_staging_atomically() {
    let mut s = setup();
    ensure_payer_output_ata(&mut s);
    let f = stuck_fixture(&mut s, [50u8; 32], 300_000);
    // unrelated owner, no delegate
    make_staging(&mut s, &f.staging_kp, &Pubkey::new_unique(), 50_000);
    set_token_amount(&mut s, &f.escrow_ata, 0);
    force_state(&mut s, &f.pda, CommitmentStatus::FailedSlippage, true);
    let settler = new_settler(&mut s);

    let err = recover(&mut s, &settler, &f).unwrap_err();
    assert!(err.to_lowercase().contains("stagingnotpullable"), "got: {err}");
    assert_eq!(load_commitment(&s, &f.pda).status, CommitmentStatus::FailedSlippage);
    assert_eq!(token_amount(&s, &f.staging_kp.pubkey()), 50_000);
    assert_eq!(token_amount(&s, &f.output_ata), 300_000);
    assert!(!is_closed(&s, &f.escrow_ata));
    assert!(!is_closed(&s, &f.output_ata));
}

// M6: enum index serialization, and that appending Recovered did not change the size
#[test]
fn status_serialization_indices() {
    let mut v = Vec::new();
    CommitmentStatus::Recovered.serialize(&mut v).unwrap();
    assert_eq!(v, vec![6u8]);
    let mut v = Vec::new();
    CommitmentStatus::TimedOut.serialize(&mut v).unwrap();
    assert_eq!(v, vec![5u8]);
    assert_eq!(<Commitment as anchor_lang::Space>::INIT_SPACE, 327);
}

// S4: staging non-empty, commitment is delegate -> pulled to payer_input_ata
#[test]
fn recover_pulls_staging_via_delegate() {
    let mut s = setup();
    ensure_payer_output_ata(&mut s);
    let f = stuck_fixture(&mut s, [60u8; 32], 300_000);
    let owner = Keypair::new();
    make_staging(&mut s, &f.staging_kp, &owner.pubkey(), 50_000);
    approve_delegate(&mut s, &f.staging_kp.pubkey(), &owner, &f.pda, u64::MAX);
    set_token_amount(&mut s, &f.escrow_ata, 0);
    force_state(&mut s, &f.pda, CommitmentStatus::FailedSlippage, true);
    let settler = new_settler(&mut s);
    let in_before = token_amount(&s, &s.payer_input_ata);

    recover(&mut s, &settler, &f).unwrap();

    assert_eq!(token_amount(&s, &s.payer_input_ata), in_before + 50_000);
    assert_eq!(token_amount(&s, &f.staging_kp.pubkey()), 0);
    assert_eq!(load_commitment(&s, &f.pda).status, CommitmentStatus::Recovered);
}

// S5: staging non-empty, commitment PDA is the owner -> pulled
#[test]
fn recover_pulls_staging_when_commitment_owns_it() {
    let mut s = setup();
    ensure_payer_output_ata(&mut s);
    let f = stuck_fixture(&mut s, [70u8; 32], 300_000);
    make_staging(&mut s, &f.staging_kp, &f.pda, 50_000);
    set_token_amount(&mut s, &f.escrow_ata, 0);
    force_state(&mut s, &f.pda, CommitmentStatus::FailedSlippage, true);
    let settler = new_settler(&mut s);
    let in_before = token_amount(&s, &s.payer_input_ata);

    recover(&mut s, &settler, &f).unwrap();

    assert_eq!(token_amount(&s, &s.payer_input_ata), in_before + 50_000);
    assert_eq!(token_amount(&s, &f.staging_kp.pubkey()), 0);
}
