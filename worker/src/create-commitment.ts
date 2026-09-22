import { readFileSync, writeFileSync } from "fs";
import { randomBytes } from "crypto";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { PROGRAM_ID, sighash, u64LE, commitmentPda, explorerTx, explorerAddr } from "./shared";

async function main() {
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const payer = Keypair.fromSecretKey(
    Buffer.from(JSON.parse(readFileSync(process.env.HOME + "/.config/solana/id.json", "utf-8")))
  );

  const executor = Keypair.generate();
  writeFileSync("executor-keypair.json", JSON.stringify(Array.from(executor.secretKey)));
  console.log("executor pubkey:", executor.publicKey.toBase58(), "(saved to worker/executor-keypair.json)");

  console.log("creating input mint...");
  const inputMint = await createMint(connection, payer, payer.publicKey, null, 6);
  console.log("creating output mint...");
  const outputMint = await createMint(connection, payer, payer.publicKey, null, 6);
  console.log("input_mint:", inputMint.toBase58(), "output_mint:", outputMint.toBase58());

  const payerInputAta = await getOrCreateAssociatedTokenAccount(connection, payer, inputMint, payer.publicKey);
  console.log("minting 1_000_000 input tokens to payer_input_ata...");
  await mintTo(connection, payer, inputMint, payerInputAta.address, payer, 1_000_000);

  const taskId = randomBytes(32);
  const [commitment] = commitmentPda(taskId);
  const escrowAta = getAssociatedTokenAddressSync(inputMint, commitment, true);
  const outputAta = getAssociatedTokenAddressSync(outputMint, commitment, true);

  const currentSlot = await connection.getSlot();
  const deadlineSlot = currentSlot + 5000; // ~a few hours of buffer at ~2 slots/sec

  const inputAmount = 1_000_000n;
  const minOutputAmount = 500_000n;

  const data = Buffer.concat([
    sighash("create_commitment"),
    taskId,
    executor.publicKey.toBuffer(),
    u64LE(inputAmount),
    u64LE(minOutputAmount),
    u64LE(deadlineSlot),
  ]);

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: executor.publicKey, isSigner: false, isWritable: false },
      { pubkey: inputMint, isSigner: false, isWritable: false },
      { pubkey: outputMint, isSigner: false, isWritable: false },
      { pubkey: commitment, isSigner: false, isWritable: true },
      { pubkey: escrowAta, isSigner: false, isWritable: true },
      { pubkey: outputAta, isSigner: false, isWritable: true },
      { pubkey: payerInputAta.address, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });

  const tx = new Transaction().add(ix);
  const sig = await sendAndConfirmTransaction(connection, tx, [payer]);

  console.log("\n=== create_commitment sent ===");
  console.log("tx:", explorerTx(sig));
  console.log("commitment PDA:", commitment.toBase58(), explorerAddr(commitment.toBase58()));
  console.log("output_ata:", outputAta.toBase58());
  console.log("task_id (hex):", taskId.toString("hex"));

  writeFileSync(
    "last-commitment.json",
    JSON.stringify({
      taskId: taskId.toString("hex"),
      commitment: commitment.toBase58(),
      outputAta: outputAta.toBase58(),
      outputMint: outputMint.toBase58(),
      minOutputAmount: minOutputAmount.toString(),
    }, null, 2)
  );
  console.log("\nsaved to worker/last-commitment.json");
  console.log("next: mint devnet tokens into output_ata to stand in for the swap, then run submit-proof");
}

main().catch((e) => { console.error(e); process.exit(1); });
