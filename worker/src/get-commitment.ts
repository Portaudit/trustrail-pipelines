import { Connection, PublicKey } from "@solana/web3.js";

const STATUS_NAMES = ["Locked", "Passed", "FailedSlippage", "Released", "Refunded", "TimedOut"];

class Cursor {
  offset = 0;
  constructor(private buf: Buffer) {}
  u8() { return this.buf.readUInt8(this.offset++); }
  bytes(n: number) { const b = this.buf.subarray(this.offset, this.offset + n); this.offset += n; return b; }
  pubkey() { return new PublicKey(this.bytes(32)); }
  u64() { const v = this.buf.readBigUInt64LE(this.offset); this.offset += 8; return v; }
  bool() { return this.u8() !== 0; }
  option<T>(reader: () => T): T | null {
    const tag = this.u8();
    return tag === 0 ? null : reader();
  }
}

async function main() {
  const address = process.argv[2];
  if (!address) {
    console.error("usage: tsx src/get-commitment.ts <COMMITMENT_PDA>");
    process.exit(1);
  }

  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const info = await connection.getAccountInfo(new PublicKey(address));
  if (!info) {
    console.error("account not found");
    process.exit(1);
  }

  const c = new Cursor(info.data);
  c.bytes(8); // anchor account discriminator

  const taskId = c.bytes(32);
  const payer = c.pubkey();
  const executorAgent = c.pubkey();
  const inputToken = c.pubkey();
  const outputToken = c.pubkey();
  const inputAmount = c.u64();
  const minOutputAmount = c.u64();
  const deadlineSlot = c.u64();
  const externalVerdict = c.option(() => c.pubkey());
  const proofTx = c.option(() => c.bytes(64));
  const proofSlot = c.option(() => c.u64());
  const verified = c.bool();
  const statusByte = c.u8();
  const bump = c.u8();

  console.log({
    taskId: Buffer.from(taskId).toString("hex"),
    payer: payer.toBase58(),
    executorAgent: executorAgent.toBase58(),
    inputToken: inputToken.toBase58(),
    outputToken: outputToken.toBase58(),
    inputAmount: inputAmount.toString(),
    minOutputAmount: minOutputAmount.toString(),
    deadlineSlot: deadlineSlot.toString(),
    externalVerdict: externalVerdict ? externalVerdict.toBase58() : null,
    proofTx: proofTx ? Buffer.from(proofTx).toString("hex") : null,
    proofSlot: proofSlot !== null ? proofSlot.toString() : null,
    verified,
    status: STATUS_NAMES[statusByte] ?? `unknown byte: ${statusByte}`,
    bump,
  });
}

main().catch((e) => { console.error(e); process.exit(1); });
