import { getConnection } from "./rpc";
import { readFileSync } from "fs";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  TokenAccountNotFoundError,
  createAssociatedTokenAccountIdempotentInstruction,
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { PROGRAM_ID, sighash, commitmentPda, explorerTx } from "./shared";

const STATUS_NAMES = ["Locked", "Passed", "FailedSlippage", "Released", "Refunded", "TimedOut", "Recovered"];
const RECOVERED = 6;

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

async function readCommitment(connection: Connection, address: PublicKey) {
  const info = await connection.getAccountInfo(address);
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

async function rawBalance(connection: Connection, addr: PublicKey): Promise<bigint | null> {
  try {
    return (await getAccount(connection, addr)).amount;
  } catch (e) {
    if (e instanceof TokenAccountNotFoundError) return null;
    throw e;
  }
}

const show = (v: bigint | null) => (v === null ? "(absent)" : v.toString());
const nz = (v: bigint | null) => v ?? 0n;

async function main() {
  const arg = process.argv[2];
  if (!arg) {
    console.error("usage: tsx src/recover.ts <COMMITMENT_PUBKEY>");
    process.exit(1);
  }
  const commitment = new PublicKey(arg);
  const connection = getConnection();

  const settlerPath = process.env.SETTLER_KEYPAIR;
  if (!settlerPath) {
    console.error("SETTLER_KEYPAIR is not set. There is no default settler. Point it at a throwaway keypair file outside the repo.");
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

  const c = await readCommitment(connection, commitment);
  const [expectedPda] = commitmentPda(c.taskId);
  if (!expectedPda.equals(commitment)) throw new Error("commitment address does not match its task_id PDA");
  if (settler.publicKey.equals(c.payer)) {
    console.error("refusing to run: the settler is the commitment's payer. Use a separate settler keypair.");
    process.exit(1);
  }

  console.log("settler:", settler.publicKey.toBase58());
  console.log("commitment:", commitment.toBase58());
  console.log("status before:", STATUS_NAMES[c.status] ?? `unknown(${c.status})`, "| escrowWithdrawn:", c.escrowWithdrawn);

  const escrowAta = getAssociatedTokenAddressSync(c.inputToken, commitment, true);
  const outputAta = getAssociatedTokenAddressSync(c.outputToken, commitment, true);
  const staging = c.expectedStagingAta;
  const payerInputAta = getAssociatedTokenAddressSync(c.inputToken, c.payer);
  const payerOutputAta = getAssociatedTokenAddressSync(c.outputToken, c.payer);

  const snapshot = async () => ({
    escrow: await rawBalance(connection, escrowAta),
    output: await rawBalance(connection, outputAta),
    staging: await rawBalance(connection, staging),
    payerIn: await rawBalance(connection, payerInputAta),
    payerOut: await rawBalance(connection, payerOutputAta),
  });
  const pre = await snapshot();
  console.log("raw balances BEFORE:", {
    escrow: show(pre.escrow), output: show(pre.output), staging: show(pre.staging),
    payerInputAta: show(pre.payerIn), payerOutputAta: show(pre.payerOut),
  });

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: settler.publicKey, isSigner: true, isWritable: false },
      { pubkey: commitment, isSigner: false, isWritable: true },
      { pubkey: c.payer, isSigner: false, isWritable: true },
      { pubkey: escrowAta, isSigner: false, isWritable: true },
      { pubkey: outputAta, isSigner: false, isWritable: true },
      { pubkey: staging, isSigner: false, isWritable: true },
      { pubkey: payerInputAta, isSigner: false, isWritable: true },
      { pubkey: payerOutputAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: sighash("recover"),
  });

  // The program requires both payer ATAs to exist; the settler pays rent if they do not.
  const tx = new Transaction()
    .add(createAssociatedTokenAccountIdempotentInstruction(settler.publicKey, payerInputAta, c.payer, c.inputToken))
    .add(createAssociatedTokenAccountIdempotentInstruction(settler.publicKey, payerOutputAta, c.payer, c.outputToken))
    .add(ix);
  tx.feePayer = settler.publicKey;

  let sig: string;
  try {
    sig = await sendAndConfirmTransaction(connection, tx, [settler]);
  } catch (e: any) {
    console.error("recover FAILED:", e?.message ?? e);
    if (e?.logs) console.error(e.logs.join("\n"));
    process.exit(1);
  }
  console.log("\n=== recover sent ===");
  console.log("tx:", explorerTx(sig));

  const post = await snapshot();
  console.log("raw balances AFTER:", {
    escrow: show(post.escrow), output: show(post.output), staging: show(post.staging),
    payerInputAta: show(post.payerIn), payerOutputAta: show(post.payerOut),
  });
  const after = await readCommitment(connection, commitment);
  console.log("status after:", STATUS_NAMES[after.status] ?? `unknown(${after.status})`);

  const problems: string[] = [];
  if (after.status !== RECOVERED) problems.push("status is not Recovered");
  if (post.escrow !== null) problems.push("escrow account still exists");
  if (post.output !== null) problems.push("output account still exists");
  if (post.staging !== 0n) problems.push("staging is not 0");
  if (nz(post.payerIn) !== nz(pre.payerIn) + nz(pre.escrow) + nz(pre.staging)) problems.push("payer input delta mismatch");
  if (nz(post.payerOut) !== nz(pre.payerOut) + nz(pre.output)) problems.push("payer output delta mismatch");

  if (problems.length > 0) {
    console.error("VERIFICATION FAILED:", problems.join("; "));
    process.exit(2);
  }
  console.log("verification OK: payer output delta =", (nz(post.payerOut) - nz(pre.payerOut)).toString());
}

main().catch((e) => { console.error(e); process.exit(1); });
