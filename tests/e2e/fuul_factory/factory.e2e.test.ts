import { Address,Asset,Contract,nativeToScVal,Operation,scValToNative,StrKey,xdr } from "@stellar/stellar-sdk";
import { expect,test } from "bun:test";
import assert from "node:assert/strict";
import { createHash,randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { deriveProjectContractId } from "../support/artifacts.js";
import { address,amount,fixture,read } from "../support/fixtures.js";
import { fundedAccount,PASSPHRASE,server } from "../support/localnet.js";
import { freshEnvelope,invoke,prepare,submit,submitFailure,transaction,type Receipt } from "../support/transactions.js";

type Fixture = Awaited<ReturnType<typeof fixture>>;
const symbol = (value: string) => nativeToScVal(value, { type: "symbol" });
const networkId = createHash("sha256").update(PASSPHRASE).digest("hex");
const instanceKey = (id: string) => xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({
  contract: new Address(id).toScAddress(), key: xdr.ScVal.scvLedgerKeyContractInstance(), durability: xdr.ContractDataDurability.persistent(),
}));
const feesKey = (factory: string, project: string) => xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({
  contract: new Address(factory).toScAddress(), key: xdr.ScVal.scvVec([symbol("ProjectFees"), address(project)]), durability: xdr.ContractDataDurability.persistent(),
}));
function events(receipt: Receipt, id: string) {
  return receipt.events.filter(event => {
    const hash = event.contractId();
    return hash instanceof Uint8Array && Buffer.from(hash).equals(StrKey.decodeContract(id));
  });
}
function event(receipt: Receipt, id: string, topic: string) {
  const matching = events(receipt, id).filter(value => scValToNative(value.body().v0().topics()[0]!) === topic);
  expect(matching).toHaveLength(1);
  const body = matching[0]!.body().v0();
  return { topics: body.topics().map(value => scValToNative(value)), data: scValToNative(body.data()) };
}
async function assertCode(id: string, hash: Buffer) {
  const entries = await server.getLedgerEntries(instanceKey(id));
  expect(entries.entries).toHaveLength(1);
  const actual = entries.entries[0]!.val.contractData().val().instance().executable().wasmHash();
  assert(actual instanceof Uint8Array);
  expect(Buffer.from(actual).toString("hex")).toBe(hash.toString("hex"));
}
const factoryCall = (f: Fixture, id: string, name: string, args: object) => f.call(1, id, name, { caller: f.admin.publicKey(), ...args }, [f.admin]);

test.serial("Factory bootstrap, permissionless repeated creation, snapshots and included revoked-admin rejection", async () => {
  const f = await fixture();
  const factory = await f.deploy(1, { admin: f.admin.publicKey(), manager: f.manager, fee_collector: f.collector.publicKey(), project_wasm_hash: f.projectWasmHash });
  expect(await read(factory, "contract_tracker", [], f.relayer)).toBe(0n);
  expect(await read(factory, "has_role", [symbol("default_admin"), address(f.admin.publicKey())], f.relayer)).toBe(true);
  expect(await read(factory, "has_role", [symbol("default_admin"), address(f.caller.publicKey())], f.relayer)).toBe(false);
  expect(await read(factory, "has_manager_role", [address(f.manager)], f.relayer)).toBe(true);
  expect(await read(factory, "project_wasm_hash", [], f.relayer)).toEqual(f.projectWasmHash);
  expect(await Promise.all(["default_native_claim_fee", "default_project_claim_fee", "default_remove_fee"].map(name => read(factory, name, [], f.relayer)))).toEqual([20_000n, 100, 0]);
  const args = { project_admin: f.admin.publicKey(), project_info_uri: "ipfs://factory-rpc", kyc_required: false };
  const firstReceipt = await invoke(f.caller, f.operation(1, factory, "create_fuul_project", args));
  const first = scValToNative(firstReceipt.value!) as string;
  expect(first).toBe(deriveProjectContractId(factory, 0n, networkId));
  expect(event(firstReceipt, factory, "project_created")).toEqual({ topics: ["project_created", first], data: { project_id: 1n, project_info_uri: args.project_info_uri } });
  await assertCode(first, f.projectWasmHash);
  expect(await read(first, "has_role", [symbol("default_admin"), address(f.admin.publicKey())], f.relayer)).toBe(true);
  expect(await read(first, "factory", [], f.relayer)).toBe(factory);
  await factoryCall(f, factory, "set_default_native_claim_fee", { value: 30_000n });
  await factoryCall(f, factory, "set_default_project_claim_fee", { value_bps: 250 });
  await factoryCall(f, factory, "set_default_remove_fee", { value_bps: 500 });
  const secondReceipt = await invoke(f.caller, f.operation(1, factory, "create_fuul_project", args));
  const second = scValToNative(secondReceipt.value!) as string;
  expect(second).toBe(deriveProjectContractId(factory, 1n, networkId));
  expect(second).not.toBe(first);
  await assertCode(second, f.projectWasmHash);
  expect(await read(factory, "project_fees", [address(first)], f.relayer)).toEqual({ native_user_claim_fee: 20_000n, project_claim_fee: 100, remove_fee: 0 });
  expect(await read(factory, "project_fees", [address(second)], f.relayer)).toEqual({ native_user_claim_fee: 30_000n, project_claim_fee: 250, remove_fee: 500 });
  const roleArgs = { role: "default_admin", account: f.caller.publicKey() };
  await factoryCall(f, factory, "grant_role", roleArgs);
  const setter = f.operation(1, factory, "set_default_remove_fee", { caller: f.caller.publicKey(), value_bps: 125 });
  const prepared = await prepare(f.relayer, setter, [f.caller]);
  await factoryCall(f, factory, "revoke_role", roleArgs);
  const snapshot = () => Promise.all([
    read(factory, "contract_tracker", [], f.relayer), read(factory, "default_remove_fee", [], f.relayer),
    read(factory, "project_fees", [address(first)], f.relayer), read(factory, "project_fees", [address(second)], f.relayer),
  ]);
  const before = await snapshot();
  await submitFailure(await freshEnvelope(prepared, f.relayer), f.relayer, { contractCode: 2000 });
  expect(await snapshot()).toEqual(before);
  await factoryCall(f, factory, "grant_role", roleArgs);
  const control = await invoke(f.relayer, setter, [f.caller]);
  expect(event(control, factory, "default_remove_fee_updated").data).toEqual({ default_remove_fee: 125 });
  expect(await read(factory, "default_remove_fee", [], f.relayer)).toBe(125);
}, 300_000);

function claimCheck(f: Fixture, project: string) {
  return { project_address: project, to: f.recipient.publicKey(), currency: f.currency, currency_type: { tag: "StellarAsset", values: undefined },
    amount: 10_000n, reason: { tag: "EndUserPayout", values: undefined }, token_id: 0n,
    deadline: BigInt(Math.floor(Date.now() / 1000) + 600), proof: randomBytes(32), signers: [f.signer.publicKey()] };
}

test.serial("Factory collector rotation routes claim, aggregate native fee and removals from existing Projects", async () => {
  const f = await fixture();
  const collector = await fundedAccount();
  await submit(await transaction(f.relayer, Operation.changeTrust({ asset: new Asset("MGRTEST", f.admin.publicKey()), source: collector.publicKey() })), f.relayer, [collector]);
  await factoryCall(f, f.factory, "set_native_user_claim_fee", { project: f.project, value: 40_000n });
  await factoryCall(f, f.factory, "set_project_claim_fee", { project: f.project, value_bps: 300 });
  await factoryCall(f, f.factory, "set_remove_fee", { project: f.project, value_bps: 1_000 });
  await factoryCall(f, f.factory, "set_default_native_claim_fee", { value: 30_000n });
  await factoryCall(f, f.factory, "set_default_project_claim_fee", { value_bps: 250 });
  await factoryCall(f, f.factory, "set_default_remove_fee", { value_bps: 500 });
  const created = await f.call(1, f.factory, "create_fuul_project", { project_admin: f.admin.publicKey(), project_info_uri: "ipfs://fees-rpc", kyc_required: false });
  const second = scValToNative(created.value!) as string;
  await invoke(f.relayer, new Contract(f.currency).call("mint", address(second), amount(50_000n)), [f.admin]);
  await factoryCall(f, f.factory, "set_fee_collector", { new_collector: collector.publicKey() });
  for (const project of [f.project, second]) {
    const information = await read(f.factory, "get_fees_information", [address(project)], f.relayer) as { fee_collector: string };
    expect(information.fee_collector).toBe(collector.publicKey());
  }
  const balance = (token: string, account: string) => read(token, "balance", [address(account)], f.relayer) as Promise<bigint>;
  const before = await Promise.all([balance(f.native, f.caller.publicKey()), balance(f.native, collector.publicKey()), balance(f.native, f.collector.publicKey())]);
  const checks = [claimCheck(f, f.project), claimCheck(f, second)];
  const receipt = await f.call(0, f.manager, "claim", { caller: f.caller.publicKey(), checks }, [f.caller, f.signer]);
  expect(events(receipt, f.native)).toHaveLength(1);
  expect(await balance(f.native, f.caller.publicKey())).toBe(before[0]! - 70_000n);
  expect(await balance(f.native, collector.publicKey())).toBe(before[1]! + 70_000n);
  expect(await balance(f.native, f.collector.publicKey())).toBe(before[2]);
  expect(await balance(f.currency, collector.publicKey())).toBe(550n);
  expect(await balance(f.currency, f.collector.publicKey())).toBe(0n);
  expect(await balance(f.currency, f.recipient.publicKey())).toBe(20_000n);
  for (const check of checks) expect(await read(check.project_address, "claimed_proofs", [nativeToScVal(check.proof)], f.relayer)).toBe(true);
  for (const project of [f.project, second]) await f.call(2, project, "remove_funds", {
    caller: f.admin.publicKey(),
    receiver: f.recipient.publicKey(), currency: f.currency, currency_type: { tag: "StellarAsset", values: undefined }, amount: 10_000n, token_ids: [], amounts: [],
  }, [f.admin]);
  expect(await balance(f.currency, collector.publicKey())).toBe(2_050n);
  expect(await balance(f.currency, f.collector.publicKey())).toBe(0n);
  expect(await balance(f.currency, f.recipient.publicKey())).toBe(38_500n);
  expect(await balance(f.currency, f.project)).toBe(80_700n);
  expect(await balance(f.currency, second)).toBe(29_750n);
}, 300_000);

async function uploadFixture(f: Fixture, name: string, expected: string) {
  const wasm = await readFile(new URL(`../../../target/wasm32v1-none/release/${name}.wasm`, import.meta.url));
  const hash = createHash("sha256").update(wasm).digest();
  assert.equal(hash.toString("hex"), expected);
  const receipt = await invoke(f.relayer, Operation.uploadContractWasm({ wasm }));
  expect(scValToNative(receipt.value!)).toEqual(hash);
  const uploaded = await server.getLedgerEntries(xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({ hash })));
  expect(uploaded.entries).toHaveLength(1);
  expect(createHash("sha256").update(uploaded.entries[0]!.val.contractCode().code()).digest()).toEqual(hash);
  return hash;
}

test.serial("included late constructor failure leaves no child or fees and repaired gate reuses the same salt", async () => {
  const f = await fixture();
  const gateHash = await uploadFixture(f, "fuul_constructor_gate_fixture", "a1ccadd099dcf948194aae85008cdcecf69b0d1cf425c4c989354df26c99f5b5");
  const probeHash = await uploadFixture(f, "fuul_project_constructor_fixture", "7ea57b4549b18afbafe112387570c516465f6dc7f8f6f89c63df63d311a24eff");
  const deployed = await invoke(f.relayer, Operation.createCustomContract({ address: new Address(f.relayer.publicKey()), wasmHash: gateHash, constructorArgs: [] }));
  const gate = scValToNative(deployed.value!) as string;
  const setReady = (ready: boolean) => invoke(f.relayer, new Contract(gate).call("set_ready", nativeToScVal(ready)));
  const factory = await f.deploy(1, { admin: f.admin.publicKey(), manager: f.manager, fee_collector: f.collector.publicKey(), project_wasm_hash: probeHash });
  const predicted = deriveProjectContractId(factory, 0n, networkId);
  await setReady(true);
  const operation = f.operation(1, factory, "create_fuul_project", { project_admin: gate, project_info_uri: "ipfs://constructor-retry", kyc_required: false });
  const prepared = await prepare(f.relayer, operation);
  await setReady(false);
  await submitFailure(await freshEnvelope(prepared, f.relayer), f.relayer, { contractCode: 8200 });
  expect((await server.getLedgerEntries(instanceKey(predicted), feesKey(factory, predicted))).entries).toHaveLength(0);
  expect(await read(factory, "contract_tracker", [], f.relayer)).toBe(0n);
  expect(await read(factory, "project_fees", [address(predicted)], f.relayer)).toEqual({ native_user_claim_fee: 0n, project_claim_fee: 0, remove_fee: 0 });
  await setReady(true);
  const control = await invoke(f.relayer, operation);
  expect(scValToNative(control.value!)).toBe(predicted);
  await assertCode(predicted, probeHash);
  expect(event(control, factory, "project_created").data).toEqual({ project_id: 1n, project_info_uri: "ipfs://constructor-retry" });
  expect(await read(factory, "contract_tracker", [], f.relayer)).toBe(1n);
  expect(await read(factory, "project_fees", [address(predicted)], f.relayer)).toEqual({ native_user_claim_fee: 20_000n, project_claim_fee: 100, remove_fee: 0 });
}, 300_000);
