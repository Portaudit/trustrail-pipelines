// One-time setup: creates the mock pool's vaults and seeds output liquidity.
// Run ONCE, AFTER create-commitment.ts has run at least once (that's what
// creates worker/mints.json).

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { createAccount, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import * as fs from "fs";
import { sighash, u64LE } from "./shared";

const MOCK_SWAP_PROGRAM_ID = new PublicKey("3qqN3ZXc1SPEtDLdn9Ek68r9YtDc1mBVgdhS7r48d9vJ");
const MINTS_PATH = "mints.json";

async function main() {
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");

  const payer = Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(process.env.HOME + "/.config/solana/id.json", "utf-8")))
  );

  if (!fs.existsSync(MINTS_PATH)) {
    throw new Error(
      "worker/mints.json not found. Run create-commitment.ts at least once first — " +
      "it creates and persists input_mint/output_mint."
    );
  }
  const mints = JSON.parse(fs.readFileSync(MINTS_PATH, "utf-8"));
  const outputMint = new PublicKey(mints.outputMint);
  const inputMint = new PublicKey(mints.inputMint);
  console.log("output_mint (from mints.json):", outputMint.toBase58());
  console.log("input_mint (from mints.json):", inputMint.toBase58());

  const [poolAuthority, poolAuthorityBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool_authority")],
    MOCK_SWAP_PROGRAM_ID
  );

  const mockPoolOutputVault = await createAccount(
    connection, payer, outputMint, poolAuthority, Keypair.generate()
  );

  // pool_authority owning mockPoolInputVault is correct but inert — the
  // input side only ever receives via staging_authority-signed transfers;
  // pool_authority never signs a transfer OUT of it. No drain needed.
  const mockPoolInputVault = await createAccount(
    connection, payer, inputMint, poolAuthority, Keypair.generate()
  );

  console.log("pool_authority PDA:", poolAuthority.toBase58());
  console.log("mock_pool_output_vault:", mockPoolOutputVault.toBase58());
  console.log("mock_pool_input_vault:", mockPoolInputVault.toBase58());

  const seedAmount = BigInt(1_000_000_000);

  const data = Buffer.concat([
    sighash("initialize_mock_pool"),
    u64LE(seedAmount),
  ]);

  const ix = new TransactionInstruction({
    programId: MOCK_SWAP_PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: outputMint, isSigner: false, isWritable: true },
      { pubkey: mockPoolOutputVault, isSigner: false, isWritable: true },
      { pubkey: poolAuthority, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });

  const tx = new Transaction().add(ix);
  const sig = await sendAndConfirmTransaction(connection, tx, [payer]);
  console.log("initialize_mock_pool tx:", sig);

  fs.writeFileSync(
    "mock-pool-state.json",
    JSON.stringify({
      mockSwapProgramId: MOCK_SWAP_PROGRAM_ID.toBase58(),
      poolAuthority: poolAuthority.toBase58(),
      poolAuthorityBump,
      outputMint: outputMint.toBase58(),
      inputMint: inputMint.toBase58(),
      mockPoolOutputVault: mockPoolOutputVault.toBase58(),
      mockPoolInputVault: mockPoolInputVault.toBase58(),
    }, null, 2)
  );
  console.log("Wrote worker/mock-pool-state.json");
}

main().catch((e) => { console.error(e); process.exit(1); });
