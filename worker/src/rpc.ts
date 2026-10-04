import { Connection } from "@solana/web3.js";

const DEFAULT_RPC = "https://api.devnet.solana.com";

export function getConnection(): Connection {
  return new Connection(process.env.RPC_URL || DEFAULT_RPC, "confirmed");
}
