import { Address, Contract, nativeToScVal, Operation, scValToNative, StrKey, xdr } from "@stellar/stellar-sdk";
import { expect, test } from "bun:test";
import { createHash, randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { address, fixture, read } from "../support/fixtures.js";
import { server } from "../support/localnet.js";
import { freshEnvelope, invoke, prepare, rejectSimulation, submitFailure } from "../support/transactions.js";

async function instance(id: string) {
  const key = xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({
    contract: new Address(id).toScAddress(), key: xdr.ScVal.scvLedgerKeyContractInstance(),
    durability: xdr.ContractDataDurability.persistent(),
  }));
  const result = await server.getLedgerEntries(key);
  expect(result.entries).toHaveLength(1);
  const value = result.entries[0]!.val.contractData().val().instance();
  return { hash: value.executable().wasmHash(), storage: xdr.ScVal.scvMap(value.storage()).toXDR("base64") };
}

test.serial("real signatures protect all three upgrades and preserve funded claims through code migration", async () => {
  const f = await fixture();
  const check = { project_address: f.project, to: f.recipient.publicKey(), currency: f.currency,
    currency_type: { tag: "StellarAsset", values: undefined }, amount: 1_000n, token_id: 0n,
    reason: { tag: "EndUserPayout", values: undefined }, deadline: BigInt(Math.floor(Date.now() / 1000) + 600),
    proof: randomBytes(32), signers: [f.signer.publicKey()] };
  const claim = (checks: typeof check[]) => f.call(0, f.manager, "claim", { caller: f.caller.publicKey(), checks }, [f.caller, f.signer]);
  await claim([check]);
  await f.call(0, f.manager, "pause", { caller: f.pauser.publicKey() }, [f.pauser]);
  const state = () => Promise.all([
    read(f.currency, "balance", [address(f.project)], f.relayer),
    read(f.currency, "balance", [address(f.recipient.publicKey())], f.relayer),
    read(f.currency, "balance", [address(f.collector.publicKey())], f.relayer),
    read(f.project, "claimed_proofs", [nativeToScVal(check.proof)], f.relayer),
    read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(f.currency)], f.relayer),
    read(f.manager, "currency_limits", [address(f.currency)], f.relayer),
    read(f.factory, "get_fees_information", [address(f.project)], f.relayer),
    read(f.factory, "contract_tracker", [], f.relayer),
    read(f.project, "project_info_uri", [], f.relayer),
    read(f.project, "factory", [], f.relayer),
    read(f.factory, "project_wasm_hash", [], f.relayer),
    read(f.manager, "paused", [], f.relayer),
  ]);
  const before = await state();
  const records: object[] = [];
  for (const [index, id] of [f.manager, f.factory, f.project].entries()) {
    const name = ["manager", "factory", "project"][index]!;
    const wasm = await readFile(new URL(`../../../target/wasm32v1-none/release/fuul_upgrade_${name}_fixture.wasm`, import.meta.url));
    const hash = createHash("sha256").update(wasm).digest();
    const uploaded = await invoke(f.relayer, Operation.uploadContractWasm({ wasm }));
    expect(scValToNative(uploaded.value!)).toEqual(hash);
    const args = { new_wasm_hash: hash, operator: f.admin.publicKey() };
    const role = { role: "default_admin", account: f.caller.publicKey(), caller: f.admin.publicKey() };
    // Project upgrade authority is the Factory administrator role; the Project's own role does not grant it.
    const [authorityIndex, authorityId] = index === 2 ? [1, f.factory] : [index, id];
    await f.call(authorityIndex, authorityId, "grant_role", role, [f.admin]);
    const pending = await prepare(f.relayer, f.operation(index, id, "upgrade", { ...args, operator: f.caller.publicKey() }), [f.caller]);
    await f.call(authorityIndex, authorityId, "revoke_role", role, [f.admin]);
    const original = await instance(id);
    const revoked = await submitFailure(await freshEnvelope(pending, f.relayer), f.relayer, { contractCode: index === 2 ? 6101 : 2000 });
    if (index === 2) await rejectSimulation(f.relayer, f.operation(index, id, "upgrade", { ...args, operator: f.caller.publicKey() }), 6101);
    const valid = await prepare(f.relayer, f.operation(index, id, "upgrade", args), [f.admin]);
    const corrupt = await freshEnvelope(valid, f.relayer, async entries => {
      const entry = entries.find(entry => entry.credentials().switch().name === "sorobanCredentialsAddress"
        && Address.fromScAddress(entry.credentials().address().address()).toString() === f.admin.publicKey())!;
      expect(entry).toBeDefined();
      const signature = entry.credentials().address().signature().vec()![0]!.map()!
        .find(field => field.key().sym().toString() === "signature")!;
      const bytes = Buffer.from(signature.val().bytes());
      bytes[0] = bytes[0]! ^ 1;
      signature.val(xdr.ScVal.scvBytes(bytes));
    });
    const invalidSignature = await submitFailure(corrupt, f.relayer, { authMessage: /failed ED25519 verification/ });
    expect(await instance(id)).toEqual(original);
    expect(await state()).toEqual(before);
    const codeKey = xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({ hash }));
    const previousCode = await server.getLedgerEntries(codeKey);
    expect(previousCode.entries).toHaveLength(1);
    const previousExpiry = previousCode.entries[0]!.liveUntilLedgerSeq!;
    const upgraded = await f.call(index, id, "upgrade", args, [f.admin]);
    const actual = await instance(id);
    expect(actual.hash).toEqual(hash);
    expect(actual.storage).toBe(original.storage);
    const code = await server.getLedgerEntries(codeKey);
    expect(code.entries).toHaveLength(1);
    // Shared code may already have a long TTL from another upgraded instance.
    // Only code below the 30-day threshold receives a fresh 90-day extension.
    const minimumExpiry = previousExpiry - upgraded.ledger <= 518_400
      ? upgraded.ledger + 1_555_200 - 1 : previousExpiry;
    expect(code.entries[0]!.liveUntilLedgerSeq).toBeGreaterThanOrEqual(minimumExpiry);
    expect(createHash("sha256").update(code.entries[0]!.val.contractCode().code()).digest()).toEqual(hash);
    const events = upgraded.events.filter(event => { const owner = event.contractId(); return owner instanceof Uint8Array && Buffer.from(owner).equals(StrKey.decodeContract(id)); })
      .map(event => ({ topics: event.body().v0().topics().map(scValToNative), data: scValToNative(event.body().v0().data()) }));
    expect(events).toEqual([
      { topics: ["executable_update", ["Wasm", original.hash], ["Wasm", hash]], data: [] },
      { topics: ["contract_upgraded"], data: { operator: f.admin.publicKey(), new_wasm_hash: hash } },
    ]);
    expect(await read(id, "fixture_schema_version", [], f.relayer)).toBe(0);
    await rejectSimulation(f.relayer, new Contract(id).call("migrate_fixture", address(f.caller.publicKey())), 2000);
    const migrated = await invoke(f.relayer, new Contract(id).call("migrate_fixture", address(f.admin.publicKey())), [f.admin]);
    expect(await read(id, "fixture_schema_version", [], f.relayer)).toBe(2);
    await rejectSimulation(f.relayer, new Contract(id).call("migrate_fixture", address(f.admin.publicKey())), 6900);
    expect(await state()).toEqual(before);
    records.push({ name, contractId: id, previousWasm: original.hash.toString("hex"), newWasm: hash.toString("hex"),
      upload: uploaded.hash, revoked: revoked.hash, invalidSignature: invalidSignature.hash,
      upgrade: upgraded.hash, upgradeLedger: upgraded.ledger, migration: migrated.hash });
  }
  await f.call(0, f.manager, "unpause", { caller: f.unpauser.publicKey() }, [f.unpauser]);
  await rejectSimulation(f.relayer, f.operation(0, f.manager, "claim", { caller: f.caller.publicKey(), checks: [check] }), 6102);
  await claim([{ ...check, proof: randomBytes(32) }]);
  expect(await read(f.currency, "balance", [address(f.recipient.publicKey())], f.relayer)).toBe(2_000n);
  expect(await read(f.currency, "balance", [address(f.project)], f.relayer)).toBe(98_980n);
  expect(await read(f.currency, "balance", [address(f.collector.publicKey())], f.relayer)).toBe(20n);
  expect(await read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(f.currency)], f.relayer)).toBe(2_000n);
  const created = await f.call(1, f.factory, "create_fuul_project", { project_admin: f.admin.publicKey(), project_info_uri: "ipfs://after-upgrade", kyc_required: false }, [f.admin]);
  const project = scValToNative(created.value!) as string;
  expect(await read(project, "factory", [], f.relayer)).toBe(f.factory);
  expect(await read(f.factory, "contract_tracker", [], f.relayer)).toBe(2n);
  expect((await instance(project)).hash).toEqual(f.projectWasmHash);
  console.info(JSON.stringify({ stage: "verified-upgrades", contracts: records, postUpgradeProject: { id: project, hash: created.hash, ledger: created.ledger } }));
}, 300_000);
