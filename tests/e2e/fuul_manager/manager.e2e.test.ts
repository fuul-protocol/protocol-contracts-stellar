import { nativeToScVal } from "@stellar/stellar-sdk";
import { expect,test } from "bun:test";
import { randomBytes } from "node:crypto";
import { address,fixture,read } from "../support/fixtures.js";
import { guardNetwork } from "../support/localnet.js";

test.serial("isolated RPC deployment and signed C1 maximum-cooldown claim", async () => {
  await guardNetwork();
  const f = await fixture();
  const balances = async () => Promise.all([f.project, f.recipient.publicKey(), f.collector.publicKey()]
    .map(id => read(f.currency, "balance", [address(id)], f.relayer)));
  expect(await balances()).toEqual([101_000n, 0n, 0n]);
  const nativeBefore = await Promise.all([f.caller, f.collector].map(key => read(f.native, "balance", [address(key.publicKey())], f.relayer))) as bigint[];
  const maxCooldown = (1n << 128n) - 1n;
  const maxUnsigned = (1n << 256n) - 1n;
  await f.call(0, f.manager, "set_claim_cooldown", { caller: f.admin.publicKey(), period: maxCooldown }, [f.admin]);
  const proof = randomBytes(32);
  const check = { project_address: f.project, to: f.recipient.publicKey(), currency: f.currency,
    currency_type: { tag: "StellarAsset", values: undefined }, amount: 100_000n,
    reason: { tag: "EndUserPayout", values: undefined }, token_id: maxUnsigned,
    deadline: maxUnsigned, proof, signers: [f.signer.publicKey()] };
  await f.call(0, f.manager, "claim", { caller: f.caller.publicKey(), checks: [check] }, [f.caller, f.signer]);
  expect(await balances()).toEqual([0n, 100_000n, 1_000n]);
  expect(await read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(f.currency)], f.relayer)).toBe(100_000n);
  expect(await read(f.project, "claimed_proofs", [nativeToScVal(proof)], f.relayer)).toBe(true);
  expect(await read(f.manager, "claim_cooldown", [], f.relayer)).toBe(maxCooldown);
  const nativeAfter = await Promise.all([f.caller, f.collector].map(key => read(f.native, "balance", [address(key.publicKey())], f.relayer))) as bigint[];
  expect(nativeAfter).toEqual([nativeBefore[0]! - 20_000n, nativeBefore[1]! + 20_000n]);
}, 300_000);
