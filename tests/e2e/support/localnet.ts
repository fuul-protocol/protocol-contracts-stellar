import { Keypair } from "@stellar/stellar-sdk";
import { LocalRpcServer } from "./rpc.js";

const PORT = process.env.FUUL_E2E_PORT ?? "18010";
if (!/^[0-9]+$/.test(PORT) || Number(PORT) < 1024 || Number(PORT) > 65535) throw new Error("Invalid local E2E port");
export const BASE = `http://127.0.0.1:${PORT}`;
export const PASSPHRASE = "Standalone Network ; February 2017";
const protocol = process.env.FUUL_E2E_PROTOCOL ?? "27";
if (protocol !== "27" && protocol !== "28") throw new Error("FUUL_E2E_PROTOCOL must be 27 or 28");
export const PROTOCOL = Number(protocol);

export function assertLocalUrl(value: string): void {
  const url = new URL(value);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || url.port !== PORT || url.username || url.password || url.hash) {
    throw new Error("Core E2E requires its selected isolated loopback endpoint");
  }
}

assertLocalUrl(BASE);
export const server = new LocalRpcServer(`${BASE}/soroban/rpc`);
let reportedIdentity = false;

export async function guardNetwork(): Promise<void> {
  assertLocalUrl(BASE);
  const [health, network] = await Promise.all([server.getHealth(), server.getNetwork()]);
  if (health.status !== "healthy" || network.passphrase !== PASSPHRASE || Number(network.protocolVersion) !== PROTOCOL) {
    throw new Error(`Core E2E requires healthy Protocol ${PROTOCOL} Standalone; public networks are forbidden`);
  }
  if (!reportedIdentity) {
    console.info(JSON.stringify({ stage: "network-identity", endpoint: BASE, health, network }));
    reportedIdentity = true;
  }
}

export async function fundedAccount(): Promise<Keypair> {
  await guardNetwork();
  const key = Keypair.random();
  const url = `${BASE}/friendbot?addr=${key.publicKey()}`;
  assertLocalUrl(url);
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    // Friendbot can start after RPC becomes healthy. A lost response can also
    // hide successful funding. Read the same disposable account before retrying.
    const response = await fetch(url, { redirect: "error", signal: AbortSignal.timeout(15_000) });
    if (!response.ok && ![502, 503, 504].includes(response.status)) {
      throw new Error(`Local Friendbot rejected funding: HTTP ${response.status}`);
    }
    try { await server.getAccount(key.publicKey()); return key; } catch { /* Wait for account visibility. */ }
    await new Promise(resolve => setTimeout(resolve, 1_000));
    try { await server.getAccount(key.publicKey()); return key; } catch { /* Retry only this account. */ }
  }
  throw new Error("Local Friendbot did not fund the disposable account within 60 seconds");
}
