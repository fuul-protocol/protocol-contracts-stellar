import { Address,Contract,contract,nativeToScVal,Operation,scValToNative,StrKey } from "@stellar/stellar-sdk";
import { expect,test } from "bun:test";
import { createHash,randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { address,fixture,read } from "../support/fixtures.js";
import { freshEnvelope,invoke,prepare,rejectSimulation,submitFailure,type Receipt } from "../support/transactions.js";

type Fixture = Awaited<ReturnType<typeof fixture>>;
const variant = (tag: string) => ({ tag, values: undefined });
const symbol = (value: string) => nativeToScVal(value, { type: "symbol" });
const amount = (value: bigint) => nativeToScVal(value, { type: "i128" });
const idValue = (value: number) => nativeToScVal(value, { type: "u32" });

function projectEvents(receipt: Receipt, project: string) {
  return receipt.events.filter(event => {
    const hash = event.contractId();
    return hash instanceof Uint8Array && Buffer.from(hash).equals(StrKey.decodeContract(project));
  })
    .map(event => ({ topics: event.body().v0().topics().map(scValToNative), data: scValToNative(event.body().v0().data()) }));
}

function check(f: Fixture, currency = f.currency, kind = "StellarAsset", quantity = 0n, tokenId = 0n) {
  return { project_address: f.project, to: f.recipient.publicKey(), currency, currency_type: variant(kind), amount: quantity,
    token_id: tokenId, reason: variant("EndUserPayout"), deadline: BigInt(Math.floor(Date.now() / 1000) + 600),
    proof: randomBytes(32), signers: [f.signer.publicKey()] };
}

const claim = (f: Fixture, checks: ReturnType<typeof check>[]) => f.call(0, f.manager, "claim", { caller: f.caller.publicKey(), checks }, [f.caller, f.signer]);
const remove = (f: Fixture, currency: string, kind: string, quantity: bigint, tokenIds: bigint[] = [], amounts: bigint[] = [], receiver = f.recipient.publicKey()) =>
  f.call(2, f.project, "remove_funds", { caller: f.admin.publicKey(), receiver, currency, currency_type: variant(kind), amount: quantity, token_ids: tokenIds, amounts }, [f.admin]);

test.serial("Project accepts EVM floor-zero fees and zero withdrawals in included transactions", async () => {
  const f = await fixture();
  const nativeBefore = await read(f.native, "balance", [address(f.caller.publicKey())], f.relayer) as bigint;
  let currentBps = 100;
  for (const [quantity, bps] of [[1n, 1], [99n, 100], [0n, 100]] as const) {
    if (bps !== currentBps) {
      await f.call(1, f.factory, "set_project_claim_fee", { caller: f.admin.publicKey(), project: f.project, value_bps: bps }, [f.admin]);
      currentBps = bps;
    }
    const c = check(f, f.currency, "StellarAsset", quantity);
    const receipt = await claim(f, [c]);
    expect(receipt.status).toBe("SUCCESS");
    expect(await read(f.project, "claimed_proofs", [nativeToScVal(c.proof)], f.relayer)).toBe(true);
  }
  expect(await read(f.currency, "balance", [address(f.recipient.publicKey())], f.relayer)).toBe(100n);
  expect(await read(f.currency, "balance", [address(f.collector.publicKey())], f.relayer)).toBe(0n);
  expect(await read(f.native, "balance", [address(f.caller.publicKey())], f.relayer)).toBe(nativeBefore - 60_000n);
  for (const bps of [0, 100]) {
    if (bps) await f.call(1, f.factory, "set_remove_fee", { caller: f.admin.publicKey(), project: f.project, value_bps: bps }, [f.admin]);
    const receipt = await remove(f, f.currency, "StellarAsset", 0n);
    expect(projectEvents(receipt, f.project)).toEqual([{ topics: ["funds_removed"], data: {
      receiver: f.recipient.publicKey(), currency: f.currency, currency_type: ["StellarAsset"], amount: 0n, token_ids: [], amounts: [],
    }}]);
  }
  expect(await read(f.currency, "balance", [address(f.project)], f.relayer)).toBe(100_900n);
}, 300_000);

test.serial("Project revocation and live KYC reject included prepared calls without consuming business state", async () => {
  const f = await fixture();
  const role = { role: "default_admin", account: f.caller.publicKey(), caller: f.admin.publicKey() };
  await f.call(2, f.project, "grant_role", role, [f.admin]);
  await f.call(2, f.project, "set_project_uri", { caller: f.caller.publicKey(), project_uri: "ipfs://second-admin" }, [f.caller]);
  const pending = await prepare(f.relayer, f.operation(2, f.project, "set_project_uri", { caller: f.caller.publicKey(), project_uri: "ipfs://revoked" }), [f.caller]);
  await f.call(2, f.project, "revoke_role", role, [f.admin]);
  await submitFailure(await freshEnvelope(pending, f.relayer), f.relayer, { contractCode: 2000 });
  expect(await read(f.project, "project_info_uri", [], f.relayer)).toBe("ipfs://second-admin");
  expect(await read(f.project, "has_role", [symbol("default_admin"), address(f.caller.publicKey())], f.relayer)).toBe(false);
  const c = check(f, f.currency, "StellarAsset", 100n);
  const operation = f.operation(0, f.manager, "claim", { caller: f.caller.publicKey(), checks: [c] });
  const prepared = await prepare(f.relayer, operation, [f.caller, f.signer]);
  await f.call(2, f.project, "set_kyc_required", { caller: f.admin.publicKey(), required: true }, [f.admin]);
  const before = await read(f.currency, "balance", [address(f.project)], f.relayer);
  await submitFailure(await freshEnvelope(prepared, f.relayer), f.relayer, { contractCode: 6103 });
  expect(await read(f.currency, "balance", [address(f.project)], f.relayer)).toBe(before);
  expect(await read(f.project, "claimed_proofs", [nativeToScVal(c.proof)], f.relayer)).toBe(false);
  await f.call(2, f.project, "set_kyc_required", { caller: f.admin.publicKey(), required: false }, [f.admin]);
  await claim(f, [c]);
  expect(await read(f.project, "claimed_proofs", [nativeToScVal(c.proof)], f.relayer)).toBe(true);
}, 300_000);

async function asset(f: Fixture, name: string, expectedHash: string) {
  const wasm = await readFile(new URL(`../../../target/wasm32v1-none/release/${name}.wasm`, import.meta.url));
  const hash = createHash("sha256").update(wasm).digest();
  expect(hash.toString("hex")).toBe(expectedHash);
  const folder = name === "fuul_e2e_nft_fixture" ? "non-fungible-token" : "multi-token";
  const inputs = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", `tests/fixtures/${folder}/Cargo.toml`, `tests/fixtures/${folder}/src/lib.rs`];
  const sources = await Promise.all(inputs.map(async path => [path, createHash("sha256").update(await readFile(new URL(`../../../${path}`, import.meta.url))).digest("hex")]));
  console.info(JSON.stringify({ stage: "project-asset-build-inputs", name, wasmSha256: expectedHash, sourceSha256: createHash("sha256").update(JSON.stringify(sources)).digest("hex") }));
  const spec = contract.Spec.fromWasm(wasm);
  const uploaded = await invoke(f.relayer, Operation.uploadContractWasm({ wasm }));
  expect(scValToNative(uploaded.value!)).toEqual(hash);
  const receipt = await invoke(f.relayer, Operation.createCustomContract({ address: new Address(f.relayer.publicKey()), wasmHash: hash,
    constructorArgs: spec.funcArgsToScVals("__constructor", { issuer: f.admin.publicKey() }) }));
  return scValToNative(receipt.value!) as string;
}

test.serial("Project uses official NFT and faithful multitoken transfers with included late-batch rollback", async () => {
  const f = await fixture();
  const nft = await asset(f, "fuul_e2e_nft_fixture", "377a0d2b0523df40cb8c7a8be2e6685234e9b8a048c8e1bc8c9984d6a6403b9f");
  const multi = await asset(f, "fuul_e2e_multi_token_fixture", "cca1bdfba9cc9f32f74928a53d7f3b27382fa63247cc740373d81c963181e858");
  await invoke(f.relayer, new Contract(nft).call("mint", address(f.project), idValue(0xffff_ffff)), [f.admin]);
  const nftClaim = check(f, nft, "NonFungible", 0n, 0xffff_ffffn);
  await claim(f, [nftClaim]);
  expect(await read(nft, "owner_of", [idValue(0xffff_ffff)], f.relayer)).toBe(f.recipient.publicKey());
  expect(await read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(nft)], f.relayer)).toBe(0n);
  for (const id of [7, 8]) await invoke(f.relayer, new Contract(multi).call("mint", address(f.project), idValue(id), amount(id === 7 ? 4n : 1n)), [f.admin]);
  const c = check(f, multi, "MultiToken", 0n, 7n);
  await claim(f, [c]);
  const balance = (owner: string, id: number) => read(multi, "balance", [address(owner), idValue(id)], f.relayer);
  expect(await balance(f.project, 7)).toBe(3n);
  await remove(f, multi, "MultiToken", 0n, [7n], [2n], f.project);
  expect(await balance(f.project, 7)).toBe(3n);
  const batch = f.operation(2, f.project, "remove_funds", { caller: f.admin.publicKey(), receiver: f.recipient.publicKey(),
    currency: multi, currency_type: variant("MultiToken"), amount: 0n, token_ids: [7n, 8n], amounts: [1n, 1n] });
  const prepared = await prepare(f.relayer, batch, [f.admin]);
  await remove(f, multi, "MultiToken", 0n, [8n], [1n]);
  const snapshot = () => Promise.all([balance(f.project, 7), balance(f.project, 8), balance(f.recipient.publicKey(), 7), balance(f.recipient.publicKey(), 8)]);
  const before = await snapshot();
  await submitFailure(await freshEnvelope(prepared, f.relayer), f.relayer, { contractCode: 2 });
  expect(await snapshot()).toEqual(before);
  await invoke(f.relayer, new Contract(multi).call("mint", address(f.project), idValue(8), amount(1n)), [f.admin]);
  await invoke(f.relayer, batch, [f.admin]);
  expect(await balance(f.project, 7)).toBe(2n);
  await remove(f, multi, "MultiToken", 0n, [7n, 7n], [1n, 1n]);
  expect(await balance(f.project, 7)).toBe(0n);
  await remove(f, multi, "MultiToken", 0n);
  const invalid = check(f, multi, "MultiToken", 0n, 1n << 32n);
  await rejectSimulation(f.relayer, f.operation(0, f.manager, "claim", { caller: f.caller.publicKey(), checks: [invalid] }), 6105);
  expect(await read(f.project, "claimed_proofs", [nativeToScVal(invalid.proof)], f.relayer)).toBe(false);
}, 300_000);
