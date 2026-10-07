// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

// Refund a FailedSlippage commitment whose escrow was never withdrawn: returns
// the escrow to the payer's input ATA, pays out any output balance to the
// payer's output ATA, and closes escrow and output.
//
// usage (from worker/):
//   SETTLER_KEYPAIR=<path> npx tsx src/refund.ts <COMMITMENT_PUBKEY>
//
// exit codes: 0 ok, 1 failure (nothing sent, or the send/confirm failed), 2 verification mismatch.

import { getConnection } from "./rpc";
import { readFileSync } from "fs";
import {
  Commitment as RpcCommitment,
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  ACCOUNT_SIZE,
  TOKEN_PROGRAM_ID,
  TokenAccountNotFoundError,
  createAssociatedTokenAccountIdempotentInstruction,
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { PROGRAM_ID, sighash, commitmentPda, explorerTx } from "./shared";

const STATUS_NAMES = ["Locked", "Passed", "FailedSlippage", "Released", "Refunded", "TimedOut", "Recovered"];
const FAILED_SLIPPAGE = 2;
const REFUNDED = 4;

// Deliberate safety buffer on top of the computed fee and rent. Not derived.
const SAFETY_MARGIN_LAMPORTS = 1_000_000;

class Cursor {
  offset = 0;
  constructor(private buf: Buffer) {}
  u8() { return this.buf.readUInt8(this.offset++); }
  bytes(n: number) { const b = this.buf.subarray(this.offset, this.offset + n); this.offset += n; return b; }
  pubkey() { return new PublicKey(this.bytes(32)); }
  u64() { const v = this.buf.readBigUInt64LE(this.offset); this.offset += 8; return v; }
  option<T>(reader: () => T): T | null {
    const tag = this.u8();
    return tag === 0 ? null : reader();
  }
}

async function readCommitment(connection: Connection, address: PublicKey, commitment: RpcCommitment) {
  const info = await connection.getAccountInfo(address, commitment);
  if (!info) throw new Error("commitment account not found");
  if (!info.owner.equals(PROGRAM_ID)) throw new Error("account is not owned by the TrustRail program");
  const c = new Cursor(info.data);
  c.bytes(8); // anchor discriminator
  const taskId = Buffer.from(c.bytes(32));
  const payer = c.pubkey();
  c.pubkey(); // executor_agent
  const inputToken = c.pubkey();
  const outputToken = c.pubkey();
  c.u64(); c.u64(); c.u64(); // input_amount, min_output_amount, deadline_slot
  c.option(() => c.pubkey()); // external_verdict
  c.option(() => c.bytes(64)); // proof_tx
  c.option(() => c.u64()); // proof_slot
  c.u8(); // verified
  const status = c.u8();
  c.u8(); // bump
  const escrowWithdrawn = c.u8() !== 0;
  const expectedStagingAta = c.pubkey();
  return { taskId, payer, inputToken, outputToken, status, escrowWithdrawn, expectedStagingAta };
}

async function rawBalance(connection: Connection, addr: PublicKey, commitment: RpcCommitment): Promise<bigint | null> {
  try {
    return (await getAccount(connection, addr, commitment)).amount;
  } catch (e) {
    if (e instanceof TokenAccountNotFoundError) return null;
    throw e;
  }
}

const show = (v: bigint | null) => (v === null ? "(absent)" : v.toString());
const nz = (v: bigint | null) => v ?? 0n;

// Never print the RPC endpoint, even if an error message quotes it.
function scrub(s: string): string {
  const url = process.env.RPC_URL;
  return url ? s.split(url).join("<RPC>") : s;
}

async function main() {
  const arg = process.argv[2];
  if (!arg) {
    console.error("usage: SETTLER_KEYPAIR=<path> npx tsx src/refund.ts <COMMITMENT_PUBKEY>");
    process.exit(1);
  }
  const commitment = new PublicKey(arg);

  const settlerPath = process.env.SETTLER_KEYPAIR;
  if (!settlerPath) {
    console.error("SETTLER_KEYPAIR is not set. There is no default settler.");
    process.exit(1);
  }
  let settler: Keypair;
  try {
    settler = Keypair.fromSecretKey(Buffer.from(JSON.parse(readFileSync(settlerPath, "utf-8"))));
  } catch {
    // Deliberately generic: a parse error message can quote part of the file contents.
    console.error("could not load a keypair from SETTLER_KEYPAIR (missing file, or not a valid keypair JSON array)");
    process.exit(1);
  }

  const connection = getConnection();
  const c = await readCommitment(connection, commitment, "confirmed");
  const [expectedPda] = commitmentPda(c.taskId);
  if (!expectedPda.equals(commitment)) throw new Error("commitment address does not match its task_id PDA");

  // NOTE: no refuse-if-settler-is-payer rule here (unlike recover.ts/cancel.ts).
  // refund takes the settler as a bare Signer, so settler == payer is allowed.
  // When they are equal the same key appears twice in the account list; web3.js
  // merges the flags (signer + writable).

  console.log("settler:", settler.publicKey.toBase58());
  console.log("commitment:", commitment.toBase58());
  console.log("status before:", STATUS_NAMES[c.status] ?? `unknown(${c.status})`, "| escrowWithdrawn:", c.escrowWithdrawn);

  const solLamports = await connection.getBalance(settler.publicKey, "confirmed");
  console.log("settler SOL balance (lamports):", solLamports.toString());

  // Preconditions (refund.rs): FailedSlippage and escrow not withdrawn.
  if (c.status !== FAILED_SLIPPAGE) {
    console.error(
      `refusing to run: status is ${STATUS_NAMES[c.status] ?? `unknown(${c.status})`}, expected FailedSlippage. Nothing was sent.`
    );
    process.exit(1);
  }
  if (c.escrowWithdrawn) {
    console.error("refusing to run: escrow_withdrawn is true (refund requires it to be false). Nothing was sent.");
    process.exit(1);
  }

  const escrowAta = getAssociatedTokenAddressSync(c.inputToken, commitment, true);
  const outputAta = getAssociatedTokenAddressSync(c.outputToken, commitment, true);
  const payerInputAta = getAssociatedTokenAddressSync(c.inputToken, c.payer);
  const payerOutputAta = getAssociatedTokenAddressSync(c.outputToken, c.payer);

  const snapshot = async (cm: RpcCommitment) => ({
    escrow: await rawBalance(connection, escrowAta, cm),
    output: await rawBalance(connection, outputAta, cm),
    payerIn: await rawBalance(connection, payerInputAta, cm),
    payerOut: await rawBalance(connection, payerOutputAta, cm),
  });
  const fmt = (b: Awaited<ReturnType<typeof snapshot>>) => ({
    escrow: show(b.escrow), output: show(b.output),
    payerInputAta: show(b.payerIn), payerOutputAta: show(b.payerOut),
  });

  const preSim = await snapshot("confirmed");
  console.log("raw balances (pre-simulation, not the BEFORE snapshot):", fmt(preSim));

  // Account order follows refund.rs.
  const refundIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: settler.publicKey, isSigner: true, isWritable: false },
      { pubkey: commitment, isSigner: false, isWritable: true },
      { pubkey: c.payer, isSigner: false, isWritable: true },
      { pubkey: escrowAta, isSigner: false, isWritable: true },
      { pubkey: outputAta, isSigner: false, isWritable: true },
      { pubkey: payerInputAta, isSigner: false, isWritable: true },
      { pubkey: payerOutputAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: sighash("refund"),
  });

  // The program requires both payer ATAs to exist; the settler pays rent if they do not.
  const buildTx = (blockhash: string, lastValidBlockHeight: number) => {
    const tx = new Transaction();
    tx.add(createAssociatedTokenAccountIdempotentInstruction(settler.publicKey, payerInputAta, c.payer, c.inputToken));
    tx.add(createAssociatedTokenAccountIdempotentInstruction(settler.publicKey, payerOutputAta, c.payer, c.outputToken));
    tx.add(refundIx);
    tx.feePayer = settler.publicKey;
    tx.recentBlockhash = blockhash;
    tx.lastValidBlockHeight = lastValidBlockHeight;
    return tx;
  };

  // SOL threshold = fee + rent for each payer ATA that is currently absent + margin.
  const rent = await connection.getMinimumBalanceForRentExemption(ACCOUNT_SIZE);
  const missing = (preSim.payerIn === null ? 1 : 0) + (preSim.payerOut === null ? 1 : 0);
  const bh0 = await connection.getLatestBlockhash();
  const feeResp = await connection.getFeeForMessage(buildTx(bh0.blockhash, bh0.lastValidBlockHeight).compileMessage());
  const fee = feeResp.value ?? 0;
  const required = fee + rent * missing + SAFETY_MARGIN_LAMPORTS;
  console.log(
    `SOL needed (lamports): ${required} = fee ${fee}${feeResp.value === null ? " (estimate unavailable, margin only)" : ""}` +
      ` + rent ${rent} x ${missing} missing payer ATA(s) + margin ${SAFETY_MARGIN_LAMPORTS}`
  );
  if (solLamports < required) {
    console.error("refusing to run: settler SOL balance is below the amount needed. Nothing was sent.");
    process.exit(1);
  }

  // SIMULATE FIRST. A failed simulation prints the error and exits 1 without sending.
  const bh1 = await connection.getLatestBlockhash();
  const simTx = buildTx(bh1.blockhash, bh1.lastValidBlockHeight);
  const sim = await connection.simulateTransaction(simTx, [settler]);
  if (sim.value.err) {
    console.error("SIMULATION FAILED - nothing was sent");
    console.error("error:", JSON.stringify(sim.value.err));
    if (sim.value.logs) console.error(scrub(sim.value.logs.join("\n")));
    process.exit(1);
  }
  console.log("simulation OK", sim.value.unitsConsumed !== undefined ? `(units consumed: ${sim.value.unitsConsumed})` : "");

  // BEFORE: read immediately before the send.
  const before = await snapshot("confirmed");
  console.log("raw balances BEFORE:", fmt(before));

  const bh2 = await connection.getLatestBlockhash();
  const tx = buildTx(bh2.blockhash, bh2.lastValidBlockHeight);
  tx.sign(settler);

  let sig: string;
  try {
    sig = await connection.sendRawTransaction(tx.serialize(), { preflightCommitment: "confirmed" });
  } catch (e: any) {
    console.error("refund send FAILED:", scrub(String(e?.message ?? e)));
    if (e?.logs) console.error(scrub(e.logs.join("\n")));
    process.exit(1);
  }
  console.log("sent, waiting for finalized...");
  const conf = await connection.confirmTransaction(
    { signature: sig, blockhash: bh2.blockhash, lastValidBlockHeight: bh2.lastValidBlockHeight },
    "finalized"
  );
  if (conf.value.err) {
    console.error("refund FAILED on chain:", JSON.stringify(conf.value.err));
    console.error("signature:", sig);
    process.exit(1);
  }
  const statuses = await connection.getSignatureStatuses([sig], { searchTransactionHistory: true });
  const slot = statuses.value[0]?.slot;
  console.log("\n=== refund finalized ===");
  console.log("signature:", sig);
  console.log("slot:", slot === undefined ? "(unavailable)" : slot);
  console.log("tx:", explorerTx(sig));

  // AFTER: finalized reads.
  const after = await snapshot("finalized");
  console.log("raw balances AFTER:", fmt(after));
  const afterC = await readCommitment(connection, commitment, "finalized");
  console.log("status after:", STATUS_NAMES[afterC.status] ?? `unknown(${afterC.status})`);

  const problems: string[] = [];
  if (afterC.status !== REFUNDED) problems.push("status is not Refunded");
  if (after.escrow !== null) problems.push("escrow account still exists");
  if (after.output !== null) problems.push("output account still exists");
  if (after.payerIn === null) problems.push("payer input ATA is absent after refund");
  if (after.payerOut === null) problems.push("payer output ATA is absent after refund");
  if (nz(after.payerIn) !== nz(before.payerIn) + nz(before.escrow)) problems.push("payer input delta mismatch");
  if (nz(after.payerOut) !== nz(before.payerOut) + nz(before.output)) problems.push("payer output delta mismatch");

  if (problems.length > 0) {
    console.error("VERIFICATION FAILED:", problems.join("; "));
    process.exit(2);
  }
  console.log(
    "verification OK: payer input delta =", (nz(after.payerIn) - nz(before.payerIn)).toString(),
    "| payer output delta =", (nz(after.payerOut) - nz(before.payerOut)).toString()
  );
}

main().catch((e) => { console.error(scrub(String(e?.message ?? e))); process.exit(1); });
