import { readFileSync } from "fs";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import * as os from "os";
import * as path from "path";
import { executeMockSwap } from "./mock-swap";

async function main() {
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const last = JSON.parse(readFileSync("last-commitment.json", "utf-8"));
  const taskId = last.taskId;
  const stagingAta = new PublicKey(last.swapStagingAta);
  const outputAta = new PublicKey(last.outputAta);
  const stagingKeypair = Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(`./swap-staging-keypair-${taskId}.json`, "utf-8")))
  );

  // funded local wallet, pays the tx fee — adjust path if your CLI config differs
  const payerPath = path.join(os.homedir(), ".config/solana/id.json");
  const payer = Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(payerPath, "utf-8")))
  );

  const amountIn = 1_000_000n;
  console.log("running mock_swap_execute for task", taskId, "amountIn:", amountIn.toString());
  console.log("fee payer:", payer.publicKey.toBase58());
  console.log("staging authority:", stagingKeypair.publicKey.toBase58());

  await executeMockSwap(connection, stagingKeypair, stagingAta, outputAta, amountIn, payer);
}

main().catch((e) => { console.error(e); process.exit(1); });
