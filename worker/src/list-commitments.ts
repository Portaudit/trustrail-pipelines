import { getConnection } from "./rpc";
import { createHash } from "crypto";
import { PublicKey } from "@solana/web3.js";
import { PROGRAM_ID } from "./shared";

// Read-only. Lists Commitment accounts owned by the program.
// Default: only accounts of exactly 335 bytes (8 discriminator + 327 INIT_SPACE).
// --all: no size filter; accounts of any other size are listed by size only.
const COMMITMENT_SIZE = 335;
const STATUS_NAMES = ["Locked", "Passed", "FailedSlippage", "Released", "Refunded", "TimedOut", "Recovered"];
const DISC = createHash("sha256").update("account:Commitment").digest().subarray(0, 8);

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

async function main() {
  const all = process.argv.includes("--all");
  const connection = getConnection();
  const slot = BigInt(await connection.getSlot());

  const accounts = await connection.getProgramAccounts(
    PROGRAM_ID,
    all ? {} : { filters: [{ dataSize: COMMITMENT_SIZE }] }
  );
  console.log(`current slot: ${slot} | program-owned accounts returned: ${accounts.length}${all ? " (no size filter)" : " (dataSize 335)"}`);

  const rows: any[] = [];
  for (const { pubkey, account } of accounts) {
    const data = account.data;
    if (data.length !== COMMITMENT_SIZE) {
      console.log(`other size: ${pubkey.toBase58()} length=${data.length} (not decoded)`);
      continue;
    }
    const c = new Cursor(data);
    const disc = c.bytes(8);
    const discOk = Buffer.compare(disc, DISC) === 0;
    c.bytes(32); // task_id
    const payer = c.pubkey();
    c.pubkey(); // executor_agent
    c.pubkey(); // input_token
    c.pubkey(); // output_token
    const inputAmount = c.u64();
    const minOutputAmount = c.u64();
    const deadlineSlot = c.u64();
    c.option(() => c.pubkey()); // external_verdict
    const proofTx = c.option(() => c.bytes(64));
    c.option(() => c.u64()); // proof_slot
    c.u8(); // verified
    const statusByte = c.u8();
    c.u8(); // bump
    const escrowWithdrawn = c.u8() !== 0;
    rows.push({
      commitment: pubkey.toBase58(),
      status: STATUS_NAMES[statusByte] ?? `unknown(${statusByte})`,
      escrowWithdrawn,
      proofStamped: proofTx !== null,
      inputAmount: inputAmount.toString(),
      minOutputAmount: minOutputAmount.toString(),
      deadlineSlot: deadlineSlot.toString(),
      deadlinePassed: slot > deadlineSlot,
      payer: payer.toBase58(),
      discriminatorOk: discOk,
    });
  }

  rows.sort((a, b) => a.status.localeCompare(b.status));
  for (const r of rows) console.log(r);

  const counts: Record<string, number> = {};
  for (const r of rows) {
    const k = `${r.status}${r.escrowWithdrawn ? " + withdrawn" : ""}`;
    counts[k] = (counts[k] ?? 0) + 1;
  }
  console.log("\nsummary:", counts);

  const locked = rows.filter((r) => r.status === "Locked");
  console.log(`\nLocked commitments: ${locked.length}`);
  for (const r of locked) {
    console.log(`  ${r.commitment} withdrawn=${r.escrowWithdrawn} proofStamped=${r.proofStamped} deadlinePassed=${r.deadlinePassed} payer=${r.payer}`);
  }
  const bad = rows.filter((r) => !r.discriminatorOk);
  if (bad.length > 0) console.log(`WARNING: ${bad.length} account(s) with an unexpected discriminator`);
}

main().catch((e) => { console.error(String(e?.message ?? e).replace(/api-key=[^ &"]*/g, "api-key=REDACTED")); process.exit(1); });
