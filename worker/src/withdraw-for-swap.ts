import { readFileSync } from "fs";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { getAccount, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { PROGRAM_ID, sighash, explorerTx } from "./shared";

async function main() {
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const payer = Keypair.fromSecretKey(
    Buffer.from(JSON.parse(readFileSync(process.env.HOME + "/.config/solana/id.json", "utf-8")))
  );

  const last = JSON.parse(readFileSync("last-commitment.json", "utf-8"));
  const commitment = new PublicKey(last.commitment);
  const escrowAta = new PublicKey(last.escrowAta);
  const swapStagingAta = new PublicKey(last.swapStagingAta);

  const before = await getAccount(connection, escrowAta);
  console.log("escrow_ata balance before withdraw_for_swap:", before.amount.toString());

  const data = sighash("withdraw_for_swap");

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: false },
      { pubkey: commitment, isSigner: false, isWritable: true },
      { pubkey: escrowAta, isSigner: false, isWritable: true },
      { pubkey: swapStagingAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });

  const tx = new Transaction().add(ix);
  const sig = await sendAndConfirmTransaction(connection, tx, [payer]);

  console.log("\n=== withdraw_for_swap sent ===");
  console.log("tx:", explorerTx(sig));

  const afterEscrow = await getAccount(connection, escrowAta);
  const afterStaging = await getAccount(connection, swapStagingAta);
  console.log("escrow_ata balance after:", afterEscrow.amount.toString(), "(expect 0)");
  console.log("swap_staging_ata balance after:", afterStaging.amount.toString());
  console.log("\nnext: run executeMockSwap (from mock-swap.ts) with this staging balance as amountIn");
}

main().catch((e) => { console.error(e); process.exit(1); });
