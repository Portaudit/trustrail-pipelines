import { readFileSync } from "fs";
import { randomBytes } from "crypto";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { getAccount } from "@solana/spl-token";
import { PROGRAM_ID, sighash, u64LE, explorerTx } from "./shared";

async function main() {
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const payer = Keypair.fromSecretKey(
    Buffer.from(JSON.parse(readFileSync(process.env.HOME + "/.config/solana/id.json", "utf-8")))
  );
  const executor = Keypair.fromSecretKey(
    Buffer.from(JSON.parse(readFileSync("executor-keypair.json", "utf-8")))
  );

  const last = JSON.parse(readFileSync("last-commitment.json", "utf-8"));
  const commitment = new PublicKey(last.commitment);
  const outputAta = new PublicKey(last.outputAta);

  const account = await getAccount(connection, outputAta);
  const actualOutput = account.amount; // bigint, real on-chain balance
  console.log("output_ata real balance:", actualOutput.toString());
  console.log("min_output_amount was:", last.minOutputAmount);

  const proofTx = randomBytes(64); // stand-in off-chain reference until a real swap tx exists
  const proofSlot = BigInt(await connection.getSlot());

  const data = Buffer.concat([
    sighash("submit_proof"),
    proofTx,
    u64LE(proofSlot),
    u64LE(actualOutput),
  ]);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: executor.publicKey, isSigner: true, isWritable: false },
      { pubkey: commitment, isSigner: false, isWritable: true },
      { pubkey: outputAta, isSigner: false, isWritable: false },
    ],
    data,
  });

  const tx = new Transaction().add(ix);
  tx.feePayer = payer.publicKey;
  const sig = await sendAndConfirmTransaction(connection, tx, [payer, executor]);

  console.log("\n=== submit_proof sent ===");
  console.log("tx:", explorerTx(sig));
  console.log(
    actualOutput >= BigInt(last.minOutputAmount)
      ? "expected outcome: Passed"
      : "expected outcome: FailedSlippage"
  );
}

main().catch((e) => { console.error(e); process.exit(1); });
