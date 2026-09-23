// Per-run: creates the staging account BEFORE create_commitment is called,
// then later — after withdraw_for_swap has moved funds into staging —
// executes mock_swap_execute: staging -> pool -> output_ata.
//
// ORDERING: createStagingAccount (Step A) must run and its returned pubkey
// be passed into create-commitment.ts before create_commitment executes.
// executeMockSwap (Step B) can only run after withdraw_for_swap has succeeded.

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { createAccount, getAccount, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import * as fs from "fs";
import { sighash, u64LE } from "./shared";

// Must match programs/mock-swap/src/lib.rs's declare_id!, Anchor.toml's
// [programs.localnet] mock_swap entry, and initialize-mock-pool.ts's
// MOCK_SWAP_PROGRAM_ID. Four independent copies, nothing cross-checks them —
// a typo in any one fails as a generic "program not found" / wrong-owner
// error, not a named one.
const MOCK_SWAP_PROGRAM_ID = new PublicKey("3qqN3ZXc1SPEtDLdn9Ek68r9YtDc1mBVgdhS7r48d9vJ");

interface MockPoolState {
  mockSwapProgramId: string;
  poolAuthority: string;
  outputMint: string;
  mockPoolOutputVault: string;
  mockPoolInputVault: string;
}

/**
 * STEP A — call BEFORE create-commitment.ts.
 * Creates a dedicated staging keypair + its token account (input mint),
 * unique per task_id since expected_staging_ata is per-commitment.
 */
export async function createStagingAccount(
  connection: Connection,
  payer: Keypair,
  inputMint: PublicKey,
  taskId: string
): Promise<{ stagingAta: PublicKey; stagingKeypair: Keypair }> {
  const stagingKeypair = Keypair.generate();
  fs.writeFileSync(
    `./swap-staging-keypair-${taskId}.json`,
    JSON.stringify(Array.from(stagingKeypair.secretKey))
  );

  const stagingAta = await createAccount(
    connection, payer, inputMint, stagingKeypair.publicKey, Keypair.generate()
  );

  console.log(`swap_staging_ata for task ${taskId}:`, stagingAta.toBase58());
  console.log(`staging authority keypair saved: swap-staging-keypair-${taskId}.json`);

  return { stagingAta, stagingKeypair };
}

/**
 * STEP B — call AFTER withdraw_for_swap has succeeded (staging_ata now funded).
 * Executes mock_swap_execute: debits staging_ata, credits output_ata.
 *
 * `payer` is the funded local wallet that pays the tx fee and is included
 * as a signer; `stagingKeypair` signs as the authority over `stagingAta`
 * but is never itself funded with SOL.
 */
export async function executeMockSwap(
  connection: Connection,
  stagingKeypair: Keypair,
  stagingAta: PublicKey,
  outputAta: PublicKey,
  amountIn: bigint,
  payer: Keypair
) {
  const poolState: MockPoolState = JSON.parse(fs.readFileSync("./mock-pool-state.json", "utf-8"));

  const poolAuthority = new PublicKey(poolState.poolAuthority);
  const mockPoolOutputVault = new PublicKey(poolState.mockPoolOutputVault);
  const mockPoolInputVault = new PublicKey(poolState.mockPoolInputVault);

  const stagingAccountInfo = await getAccount(connection, stagingAta);
  if (stagingAccountInfo.amount < amountIn) {
    throw new Error(
      `swap_staging_ata balance (${stagingAccountInfo.amount}) < amountIn (${amountIn}) — did withdraw_for_swap actually run?`
    );
  }

  const data = Buffer.concat([
    sighash("mock_swap_execute"),
    u64LE(amountIn),
  ]);

  const ix = new TransactionInstruction({
    programId: MOCK_SWAP_PROGRAM_ID,
    keys: [
      { pubkey: stagingAta, isSigner: false, isWritable: true },
      { pubkey: stagingKeypair.publicKey, isSigner: true, isWritable: false },
      { pubkey: mockPoolInputVault, isSigner: false, isWritable: true },
      { pubkey: mockPoolOutputVault, isSigner: false, isWritable: true },
      { pubkey: poolAuthority, isSigner: false, isWritable: false },
      { pubkey: outputAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });

  const tx = new Transaction().add(ix);
  tx.feePayer = payer.publicKey;
  const sig = await sendAndConfirmTransaction(connection, tx, [payer, stagingKeypair]);
  console.log("mock_swap_execute tx:", sig);

  const outputAccountInfo = await getAccount(connection, outputAta);
  console.log("output_ata balance after swap:", outputAccountInfo.amount.toString());

  return sig;
}
