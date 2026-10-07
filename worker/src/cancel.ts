// Copyright 2026 Ishvir and Company (Pty) Ltd
// SPDX-License-Identifier: Apache-2.0

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
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { PROGRAM_ID, sighash, explorerTx } from "./shared";

async function bal(connection: Connection, addr: PublicKey): Promise<bigint | null> {
  try {
    return (await getAccount(connection, addr)).amount;
  } catch (e) {
    if (e instanceof TokenAccountNotFoundError) return null;
    throw e;
  }
}
const show = (v: bigint | null) => (v === null ? "(absent)" : v.toString());

async function main() {
  const arg = process.argv[2];
  if (!arg) { console.error("usage: tsx src/cancel.ts <COMMITMENT_PUBKEY>"); process.exit(1); }
  const settlerPath = process.env.SETTLER_KEYPAIR;
  if (!settlerPath) { console.error("SETTLER_KEYPAIR is not set. There is no default settler."); process.exit(1); }
  let settler: Keypair;
  try {
    settler = Keypair.fromSecretKey(Buffer.from(JSON.parse(readFileSync(settlerPath, "utf-8"))));
  } catch {
    console.error("could not load a keypair from SETTLER_KEYPAIR");
    process.exit(1);
  }

  const connection = getConnection();
  const commitment = new PublicKey(arg);
  const info = await connection.getAccountInfo(commitment);
  if (!info) throw new Error("commitment account not found");
  if (!info.owner.equals(PROGRAM_ID)) throw new Error("account is not owned by the TrustRail program");
  // offsets: 8 discriminator, task_id 32, payer 32, executor 32, input_token 32
  const payer = new PublicKey(info.data.subarray(40, 72));
  const inputToken = new PublicKey(info.data.subarray(104, 136));
  if (settler.publicKey.equals(payer)) {
    console.error("refusing to run: the settler is the commitment's payer.");
    process.exit(1);
  }

  const escrowAta = getAssociatedTokenAddressSync(inputToken, commitment, true);
  const payerInputAta = getAssociatedTokenAddressSync(inputToken, payer);
  const preEscrow = await bal(connection, escrowAta);
  const prePayer = await bal(connection, payerInputAta);
  console.log("BEFORE:", { escrow: show(preEscrow), payerInputAta: show(prePayer) });

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: settler.publicKey, isSigner: true, isWritable: false },
      { pubkey: commitment, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: false, isWritable: true },
      { pubkey: escrowAta, isSigner: false, isWritable: true },
      { pubkey: payerInputAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: sighash("cancel"),
  });
  const tx = new Transaction().add(ix);
  tx.feePayer = settler.publicKey;
  let sig: string;
  try {
    sig = await sendAndConfirmTransaction(connection, tx, [settler]);
  } catch (e: any) {
    console.error("cancel FAILED:", e?.message ?? e);
    if (e?.logs) console.error(e.logs.join("\n"));
    process.exit(1);
  }
  console.log("tx:", explorerTx(sig));
  const postEscrow = await bal(connection, escrowAta);
  const postPayer = await bal(connection, payerInputAta);
  console.log("AFTER:", { escrow: show(postEscrow), payerInputAta: show(postPayer) });
  const expected = (prePayer ?? 0n) + (preEscrow ?? 0n);
  if (postEscrow !== null || (postPayer ?? 0n) !== expected) {
    console.error("VERIFICATION FAILED");
    process.exit(2);
  }
  console.log("verification OK");
}
main().catch((e) => { console.error(e); process.exit(1); });
