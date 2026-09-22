import { Address,Asset,Contract,contract,Keypair,nativeToScVal,Operation,rpc,scValToNative,xdr } from "@stellar/stellar-sdk";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { readCoreBuildInputSha256 } from "./artifacts.js";
import { fundedAccount,guardNetwork,PASSPHRASE,server } from "./localnet.js";
import { invoke,submit,transaction } from "./transactions.js";

export const address = (value: string) => new Address(value).toScVal();
export const amount = (value: bigint) => nativeToScVal(value, { type: "i128" });

export async function read(contractId: string, method: string, args: xdr.ScVal[], source: Keypair): Promise<unknown> {
  const tx = await transaction(source, new Contract(contractId).call(method, ...args));
  const result = await server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(result) || !result.result) throw new Error(`read simulation failed: ${method}`);
  return scValToNative(result.result.retval);
}

export async function fixture() {
  await guardNetwork();
  const artifacts = await Promise.all(["manager", "factory", "project"].map(async name => {
    const wasm = await readFile(new URL(`../../../target/wasm32v1-none/release/fuul_${name}.wasm`, import.meta.url));
    return { name, wasm, hash: createHash("sha256").update(wasm).digest(), spec: contract.Spec.fromWasm(wasm) };
  }));
  const expectedHashes = Object.fromEntries(artifacts.map(artifact => {
    const hash = artifact.hash.toString("hex");
    return [artifact.name, hash];
  }));
  console.info(JSON.stringify({ stage: "core-build-inputs", sourceSha256: await readCoreBuildInputSha256(fileURLToPath(new URL("../../../", import.meta.url))), wasm: expectedHashes }));
  const relayer = await fundedAccount();
  const admin = await fundedAccount();
  const caller = await fundedAccount();
  const signer = await fundedAccount();
  const recipient = await fundedAccount();
  const collector = await fundedAccount();
  const pauser = await fundedAccount();
  const unpauser = await fundedAccount();
  const asset = new Asset("MGRTEST", admin.publicKey());
  const currency = asset.contractId(PASSPHRASE);
  const native = Asset.native().contractId(PASSPHRASE);
  for (const key of [recipient, collector, caller]) {
    await submit(await transaction(relayer, Operation.changeTrust({ asset, source: key.publicKey() })), relayer, [key]);
  }
  await invoke(relayer, Operation.createStellarAssetContract({ asset }));
  const nativeInstance = xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({
    contract: new Address(native).toScAddress(), key: xdr.ScVal.scvLedgerKeyContractInstance(),
    durability: xdr.ContractDataDurability.persistent(),
  }));
  if ((await server.getLedgerEntries(nativeInstance)).entries.length === 0) {
    await invoke(relayer, Operation.createStellarAssetContract({ asset: Asset.native() }));
  }
  for (const item of artifacts) {
    const receipt = await invoke(relayer, Operation.uploadContractWasm({ wasm: item.wasm }));
    assert.deepEqual(scValToNative(receipt.value!), item.hash);
    const entries = await server.getLedgerEntries(xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({ hash: item.hash })));
    assert.equal(entries.entries.length, 1);
    assert.deepEqual(createHash("sha256").update(entries.entries[0]!.val.contractCode().code()).digest(), item.hash);
  }
  async function deploy(index: number, args: object): Promise<string> {
    const item = artifacts[index]!;
    const result = await invoke(relayer, Operation.createCustomContract({ address: new Address(relayer.publicKey()), wasmHash: item.hash, constructorArgs: item.spec.funcArgsToScVals("__constructor", args) }));
    const id = scValToNative(result.value!) as string;
    assert.equal(typeof id, "string");
    return id;
  }
  const manager = await deploy(0, { admin: admin.publicKey(), pauser: pauser.publicKey(), unpauser: unpauser.publicKey(), initial_required_signers: 1n, claim_signers: [signer.publicKey()], accepted_currency: currency, native_asset: native, initial_kyc_validator: undefined, initial_currency_limit: 1_000_000_000_000n });
  const factory = await deploy(1, { admin: admin.publicKey(), manager, fee_collector: collector.publicKey(), project_wasm_hash: artifacts[2]!.hash });
  assert.deepEqual(await read(factory, "project_wasm_hash", [], relayer), artifacts[2]!.hash);
  async function call(index: number, id: string, method: string, args: object, signers: Keypair[] = []) {
    return invoke(relayer, new Contract(id).call(method, ...artifacts[index]!.spec.funcArgsToScVals(method, args)), signers);
  }
  const created = await call(1, factory, "create_fuul_project", { project_admin: admin.publicKey(), project_info_uri: "ipfs://manager-local-e2e", kyc_required: false }, [admin]);
  const project = scValToNative(created.value!) as string;
  assert.equal(await read(manager, "default_admin_role", [], relayer), "default_admin");
  assert.equal(await read(manager, "has_role", [nativeToScVal("default_admin", { type: "symbol" }), address(admin.publicKey())], relayer), true);
  assert.equal(await read(manager, "native_asset", [], relayer), native);
  assert.equal(await read(manager, "required_signers", [], relayer), 1n);
  assert.equal(await read(manager, "paused", [], relayer), false);
  assert.equal(await read(factory, "fee_collector", [], relayer), collector.publicKey());
  assert.equal(await read(project, "factory", [], relayer), factory);
  assert.equal(await read(project, "has_role", [nativeToScVal("default_admin", { type: "symbol" }), address(admin.publicKey())], relayer), true);
  for (const [key, role] of [[signer, "claim_signer"], [pauser, "pauser"], [unpauser, "unpauser"]] as const) {
    assert.equal(await read(manager, "has_role", [nativeToScVal(role, { type: "symbol" }), address(key.publicKey())], relayer), true);
  }
  await invoke(relayer, new Contract(currency).call("mint", address(project), amount(101_000n)), [admin]);
  const operation = (index: number, id: string, method: string, args: object) => new Contract(id).call(method, ...artifacts[index]!.spec.funcArgsToScVals(method, args));
  return { relayer, admin, caller, signer, recipient, collector, pauser, unpauser, manager, factory, project, native, currency, call, operation, deploy, projectWasmHash: artifacts[2]!.hash };
}
