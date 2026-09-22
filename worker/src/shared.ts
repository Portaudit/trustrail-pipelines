import { createHash } from "crypto";
import { PublicKey } from "@solana/web3.js";

export const PROGRAM_ID = new PublicKey("CgXidrtsV5nLkjPCYhZUPUuZUpmvekMKvDN9uh3ucoC8");

export function sighash(name: string): Buffer {
  return createHash("sha256").update(`global:${name}`).digest().subarray(0, 8);
}

export function u64LE(value: bigint | number): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(value));
  return buf;
}

export function commitmentPda(taskId: Buffer): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("commitment"), taskId],
    PROGRAM_ID
  );
}

export function explorerTx(sig: string): string {
  return `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
}

export function explorerAddr(addr: string): string {
  return `https://explorer.solana.com/address/${addr}?cluster=devnet`;
}
