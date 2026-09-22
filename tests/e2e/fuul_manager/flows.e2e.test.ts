import { Address,authorizeEntry,Contract,contract,Keypair,nativeToScVal,Operation,scValToNative,StrKey,xdr } from "@stellar/stellar-sdk";
import { expect,test } from "bun:test";
import assert from "node:assert/strict";
import { createHash,randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { address,amount,fixture,read } from "../support/fixtures.js";
import { fundedAccount,PASSPHRASE,server } from "../support/localnet.js";
import { freshEnvelope,invoke,prepare,rejectSimulation,submitFailure,type Receipt } from "../support/transactions.js";

type Fixture = Awaited<ReturnType<typeof fixture>>;
function check(f: Fixture, project = f.project, signers = [f.signer.publicKey()]) {
  return { project_address: project, to: f.recipient.publicKey(), currency: f.currency,
    currency_type: { tag: "StellarAsset", values: undefined }, amount: 1_000n,
    reason: { tag: "EndUserPayout", values: undefined }, token_id: 0n,
    deadline: BigInt(Math.floor(Date.now() / 1000) + 600), proof: randomBytes(32), signers };
}
type Check = ReturnType<typeof check>;
const claimOperation = (f: Fixture, checks: Check[]) => f.operation(0, f.manager, "claim", { caller: f.caller.publicKey(), checks });
const claim = (f: Fixture, checks: Check[], signers = [f.caller, f.signer]) => invoke(f.relayer, claimOperation(f, checks), signers);
const prepareClaim = (f: Fixture, checks: Check[], signers = [f.caller, f.signer]) => prepare(f.relayer, claimOperation(f, checks), signers);
const managerCall = (f: Fixture, method: string, args: object, signers = [f.admin]) => f.call(0, f.manager, method, { caller: signers[0]!.publicKey(), ...args }, signers);

async function state(f: Fixture, checks: Check[]) {
  const projects = [...new Set(checks.map(value => value.project_address))];
  return {
    balances: await Promise.all([...projects, f.recipient.publicKey(), f.collector.publicKey(), f.caller.publicKey()]
      .map(id => read(f.currency, "balance", [address(id)], f.relayer))),
    native: await Promise.all([f.caller, f.collector].map(key => read(f.native, "balance", [address(key.publicKey())], f.relayer))),
    window: await read(f.manager, "currency_limits", [address(f.currency)], f.relayer),
    user: await read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(f.currency)], f.relayer),
    proofs: await Promise.all(checks.map(value => read(value.project_address, "claimed_proofs", [nativeToScVal(value.proof)], f.relayer))),
  };
}

function assertClaimEvents(f: Fixture, receipt: Receipt, nativeFee: boolean) {
  const at = (id: string) => receipt.events.filter(event => {
    const hash = event.contractId();
    if (!hash) return false;
    assert(hash instanceof Uint8Array, "Expected raw contract ID bytes from SDK event");
    return Buffer.from(hash).equals(StrKey.decodeContract(id));
  });
  expect(at(f.manager)).toHaveLength(1);
  expect(at(f.currency)).toHaveLength(2);
  expect(at(f.native)).toHaveLength(nativeFee ? 1 : 0);
}

test.serial("pause authorization, paused rejection, same-proof recovery and fresh-auth proof replay", async () => {
  const f = await fixture();
  const c = check(f);
  const before = await state(f, [c]);
  await rejectSimulation(f.relayer, f.operation(0, f.manager, "pause", { caller: f.caller.publicKey() }), 2000);
  expect(await read(f.manager, "paused", [], f.relayer)).toBe(false);
  expect(await state(f, [c])).toEqual(before);
  const paused = await managerCall(f, "pause", { caller: f.pauser.publicKey() }, [f.pauser]);
  expect(eventData(paused, f.manager, "paused")).toEqual({ topics: ["paused"], data: { account: f.pauser.publicKey() } });
  expect(paused.events).toHaveLength(1);
  expect(await read(f.manager, "paused", [], f.relayer)).toBe(true);
  await rejectSimulation(f.relayer, claimOperation(f, [c]), 1000);
  expect(await state(f, [c])).toEqual(before);
  const unpaused = await managerCall(f, "unpause", { caller: f.unpauser.publicKey() }, [f.unpauser]);
  expect(eventData(unpaused, f.manager, "unpaused")).toEqual({ topics: ["unpaused"], data: { account: f.unpauser.publicKey() } });
  expect(unpaused.events).toHaveLength(1);
  expect(await read(f.manager, "paused", [], f.relayer)).toBe(false);
  // These valid native authorizations are never consumed by the first claim.
  // After it succeeds, only the business proof is stale; the envelope gets a fresh sequence.
  const replay = await prepareClaim(f, [c]);
  const success = await claim(f, [c]);
  assertClaimEvents(f, success, true);
  const after = await state(f, [c]);
  expect(after.proofs).toEqual([true]);
  expect(after.user).toBe(1_000n);
  const replayEnvelope = await freshEnvelope(replay, f.relayer, async entries => {
    // Sign again after the successful claim, retaining only the unused nonces and
    // their already-prepared footprint keys. This cannot be a used-auth-nonce replay.
    const expiration = (await server.getLatestLedger()).sequence + 100;
    for (const signer of [f.caller, f.signer]) {
      const entry = signerEntry(entries, signer);
      entries[entries.indexOf(entry)] = await authorizeEntry(entry, signer, expiration, PASSPHRASE);
    }
  });
  await submitFailure(replayEnvelope, f.relayer, { contractCode: 6102 });
  expect(await state(f, [c])).toEqual(after);
  await claim(f, [check(f)]);
}, 300_000);

test.serial("fee exemption belongs to the caller, with exact SAC balances and committed events", async () => {
  const f = await fixture();
  expect(new Set([f.relayer.publicKey(), f.caller.publicKey(), f.recipient.publicKey(), f.signer.publicKey()]).size).toBe(4);
  await managerCall(f, "add_no_claim_fee_address", { account: f.recipient.publicKey() });
  for (const [mode, expectedNativeFee] of [["recipient-only", 20_000n], ["caller-exempt", 0n], ["caller-removed", 20_000n]] as const) {
    if (mode === "caller-exempt") await managerCall(f, "add_no_claim_fee_address", { account: f.caller.publicKey() });
    if (mode === "caller-removed") await managerCall(f, "remove_no_claim_fee_address", { account: f.caller.publicKey() });
    const c = check(f);
    const before = await state(f, [c]);
    const receipt = await claim(f, [c]);
    const after = await state(f, [c]);
    expect(after.balances).toEqual([(before.balances[0] as bigint) - 1_010n, (before.balances[1] as bigint) + 1_000n, (before.balances[2] as bigint) + 10n, before.balances[3]]);
    expect(after.native).toEqual([(before.native[0] as bigint) - expectedNativeFee, (before.native[1] as bigint) + expectedNativeFee]);
    expect(after.user).toBe((before.user as bigint) + 1_000n);
    expect(after.proofs).toEqual([true]);
    assertClaimEvents(f, receipt, expectedNativeFee !== 0n);
  }
}, 300_000);

test.serial("signer grant, threshold two, revocation and restored quorum use distinct G-account signatures", async () => {
  const f = await fixture();
  const second = await fundedAccount();
  await managerCall(f, "grant_role", { account: second.publicKey(), role: "claim_signer", caller: f.admin.publicKey() });
  await managerCall(f, "set_required_signers", { value: 2n });
  const single = check(f);
  const before = await state(f, [single]);
  await rejectSimulation(f.relayer, claimOperation(f, [single]), 6306);
  expect(await state(f, [single])).toEqual(before);
  const both = { ...single, signers: [f.signer.publicKey(), second.publicKey()] };
  await claim(f, [both], [f.caller, f.signer, second]);
  await managerCall(f, "revoke_role", { account: f.signer.publicKey(), role: "claim_signer", caller: f.admin.publicKey() });
  const next = check(f, f.project, both.signers);
  const revoked = await state(f, [next]);
  await rejectSimulation(f.relayer, claimOperation(f, [next]), 6307);
  expect(await state(f, [next])).toEqual(revoked);
  await managerCall(f, "grant_role", { account: f.signer.publicKey(), role: "claim_signer", caller: f.admin.publicKey() });
  await claim(f, [next], [f.caller, f.signer, second]);
  expect(await read(f.manager, "required_signers", [], f.relayer)).toBe(2n);
}, 300_000);

function signerEntry(entries: xdr.SorobanAuthorizationEntry[], signer: Keypair) {
  const entry = entries.find(value => value.credentials().switch().name !== "sorobanCredentialsSourceAccount" && Address.fromScAddress(value.credentials().address().address()).toString() === signer.publicKey());
  assert(entry, "Expected non-source signer authorization");
  return entry;
}

test.serial("corrupt and expired native authorizations fail on chain, then fresh authorization succeeds", async () => {
  const f = await fixture();
  for (const mode of ["corrupt", "expired"] as const) {
    const c = check(f);
    const valid = await prepareClaim(f, [c]);
    const before = await state(f, [c]);
    const invalid = await freshEnvelope(valid, f.relayer, async entries => {
      const entry = signerEntry(entries, f.signer);
      if (mode === "corrupt") {
        const signatures = entry.credentials().address().signature().vec()!;
        const signature = signatures[0]!.map()!.find(field => field.key().sym().toString() === "signature")!;
        const bytes = Buffer.from(signature.val().bytes());
        bytes[0] = bytes[0]! ^ 1;
        signature.val(xdr.ScVal.scvBytes(bytes));
      } else {
        const expiredLedger = (await server.getLatestLedger()).sequence - 1;
        const expired = await authorizeEntry(entry, f.signer, expiredLedger, PASSPHRASE);
        entries[entries.indexOf(entry)] = expired;
      }
    });
    await submitFailure(invalid, f.relayer, { authMessage: mode === "corrupt" ? /failed ED25519 verification/ : /signature has expired/ });
    expect(await state(f, [c])).toEqual(before);
    await claim(f, [c]);
    expect((await state(f, [c])).proofs).toEqual([true]);
  }
}, 300_000);

test.serial("a late second-Project proof failure rolls back first transfers, proofs and Manager accounting", async () => {
  const f = await fixture();
  const created = await f.call(1, f.factory, "create_fuul_project", { project_admin: f.admin.publicKey(), project_info_uri: "ipfs://late-batch", kyc_required: false }, [f.admin]);
  const second = scValToNative(created.value!) as string;
  await invoke(f.relayer, new Contract(f.currency).call("mint", address(second), amount(101_000n)), [f.admin]);
  const first = check(f);
  const last = check(f, second);
  // Simulate the complete successful two-Project transfer path and sign fresh batch auth.
  const batch = await prepareClaim(f, [first, last]);
  const operation = batch.operations[0]!;
  assert.equal(operation.type, "invokeHostFunction");
  if (operation.type !== "invokeHostFunction") throw new Error("Expected claim invocation");
  const proofs = operation.auth!.filter(entry => entry.credentials().switch().name !== "sorobanCredentialsSourceAccount" &&
    Address.fromScAddress(entry.credentials().address().address()).toString() === f.signer.publicKey());
  expect(proofs).toHaveLength(2);
  expect(new Set(proofs.map(entry => entry.credentials().address().nonce().toString())).size).toBe(2);
  for (const [index, entry] of proofs.entries()) {
    const args = entry.rootInvocation().function().contractFn().args();
    expect(args).toHaveLength(1);
    const intent = scValToNative(args[0]!);
    expect(Object.keys(intent).sort()).toEqual(["amount", "currency", "deadline", "project_address", "proof", "reason", "to", "token_id"]);
    expect(intent.proof).toEqual([first, last][index]!.proof);
  }
  // Consume only the second proof with independent native auth. The original footprint
  // already contains this exact proof key; the prepared batch still has sufficient funds.
  await claim(f, [last]);
  const before = await state(f, [first, last]);
  expect(before.proofs).toEqual([false, true]);
  const sourceBefore = await server.getAccount(f.relayer.publicKey());
  const sourceBalanceBefore = await read(f.native, "balance", [address(f.relayer.publicKey())], f.relayer) as bigint;
  await submitFailure(await freshEnvelope(batch, f.relayer), f.relayer, { contractCode: 6102, attemptedTransfer: { currency: f.currency, from: f.project } });
  expect(await state(f, [first, last])).toEqual(before);
  expect(BigInt((await server.getAccount(f.relayer.publicKey())).sequenceNumber())).toBe(BigInt(sourceBefore.sequenceNumber()) + 1n);
  expect(await read(f.native, "balance", [address(f.relayer.publicKey())], f.relayer)).toBeLessThan(sourceBalanceBefore);
  // The unchanged first proof succeeds once the second check receives a fresh proof.
  const repaired = { ...last, proof: randomBytes(32) };
  await claim(f, [first, repaired]);
  const after = await state(f, [first, repaired]);
  expect(after.proofs).toEqual([true, true]);
  expect(after.user).toBe((before.user as bigint) + 2_000n);
}, 300_000);

function eventData(receipt: Receipt, id: string, name: string) {
  const events = receipt.events.filter(event => {
    const hash = event.contractId();
    return hash instanceof Uint8Array && Buffer.from(hash).equals(StrKey.decodeContract(id)) &&
      scValToNative(event.body().v0().topics()[0]!) === name;
  });
  expect(events).toHaveLength(1);
  const body = events[0]!.body().v0();
  return { topics: body.topics().map(value => scValToNative(value)), data: scValToNative(body.data()) };
}

test.serial("test-only KYC provider None, deny and unavailable fail on chain before same-proof allow controls", async () => {
  const f = await fixture();
  const wasm = await readFile(new URL("../../../target/wasm32v1-none/release/fuul_e2e_kyc_fixture.wasm", import.meta.url));
  const source = await readFile(new URL("../../fixtures/kyc-provider/src/lib.rs", import.meta.url));
  const hash = createHash("sha256").update(wasm).digest();
  const spec = contract.Spec.fromWasm(wasm);
  console.info(JSON.stringify({ stage: "test-only-kyc-fixture", wasmSha256: hash.toString("hex"), sourceSha256: createHash("sha256").update(source).digest("hex"), sdk: "26.1.1" }));
  expect(scValToNative((await invoke(f.relayer, Operation.uploadContractWasm({ wasm }))).value!)).toEqual(hash);
  const uploaded = await server.getLedgerEntries(xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({ hash })));
  expect(createHash("sha256").update(uploaded.entries[0]!.val.contractCode().code()).digest()).toEqual(hash);
  const deployed = await invoke(f.relayer, Operation.createCustomContract({ address: new Address(f.relayer.publicKey()), wasmHash: hash,
    constructorArgs: spec.funcArgsToScVals("__constructor", { admin: f.admin.publicKey(), expected_recipient: f.recipient.publicKey(), mode: 1 }) }));
  const provider = scValToNative(deployed.value!) as string;
  const setMode = (mode: number) => invoke(f.relayer, new Contract(provider).call("set_mode", nativeToScVal(mode, { type: "u32" })), [f.admin]);
  const setProvider = async (validator: string | undefined) => {
    const receipt = await managerCall(f, "set_kyc_validator", { validator });
    // SDK 16.3.0 decodes Soroban Void/None as null, while the ABI encoder accepts undefined.
    expect(eventData(receipt, f.manager, "kyc_validator_updated").topics).toEqual(["kyc_validator_updated", validator ?? null]);
    expect(await read(f.manager, "kyc_validator", [], f.relayer)).toBe(validator ?? null);
  };
  const created = await f.call(1, f.factory, "create_fuul_project", { project_admin: f.admin.publicKey(), project_info_uri: "ipfs://test-only-kyc", kyc_required: true }, [f.admin]);
  const project = scValToNative(created.value!) as string;
  expect(await read(project, "kyc_required", [], f.relayer)).toBe(true);
  await invoke(f.relayer, new Contract(f.currency).call("mint", address(project), amount(101_000n)), [f.admin]);
  for (const mode of ["none", "deny", "fail"] as const) {
    await setProvider(provider);
    await setMode(1);
    expect(await read(provider, "is_user_kyc_registered", [address(f.recipient.publicKey())], f.relayer)).toBe(true);
    const c = check(f, project);
    const prepared = await prepareClaim(f, [c]);
    if (mode === "none") await setProvider(undefined);
    else await setMode(mode === "deny" ? 0 : 2);
    if (mode === "deny") expect(await read(provider, "is_user_kyc_registered", [address(f.recipient.publicKey())], f.relayer)).toBe(false);
    const before = await state(f, [c]);
    const sourceSequence = BigInt((await server.getAccount(f.relayer.publicKey())).sequenceNumber());
    const sourceBalance = await read(f.native, "balance", [address(f.relayer.publicKey())], f.relayer) as bigint;
    // Same provider code/instance/recipient keys remain in the successful footprint.
    // None uses a strict subset; deny/fail change only provider instance state.
    await submitFailure(await freshEnvelope(prepared, f.relayer), f.relayer, { contractCode: mode === "fail" ? 9100 : 6103 });
    expect(await state(f, [c])).toEqual(before);
    expect(BigInt((await server.getAccount(f.relayer.publicKey())).sequenceNumber())).toBe(sourceSequence + 1n);
    expect(await read(f.native, "balance", [address(f.relayer.publicKey())], f.relayer)).toBeLessThan(sourceBalance);
    await setProvider(provider);
    await setMode(1);
    const allowed = await claim(f, [c]);
    assertClaimEvents(f, allowed, true);
    const after = await state(f, [c]);
    expect(after.proofs).toEqual([true]);
    expect(after.user).toBe((before.user as bigint) + c.amount);
  }
}, 300_000);

async function wrongAccountSignature(prepared: Awaited<ReturnType<typeof prepare>>, f: Fixture, expected: Keypair, wrong: Keypair) {
  return freshEnvelope(prepared, f.relayer, async entries => {
    const entry = signerEntry(entries, expected);
    // Keep destination credentials and nonce, but supply a valid signature from
    // a key that has no weight in that destination account. The outer signer stays valid.
    entries[entries.indexOf(entry)] = await authorizeEntry(entry, wrong, entry.credentials().address().signatureExpirationLedger(), PASSPHRASE);
  });
}

test.serial("C2 zero and below-cumulative setters preserve state and positive readdition resets only the window", async () => {
  const f = await fixture();
  await managerCall(f, "set_currency_token_limit", { token: f.currency, limit: 2_000n });
  const first = check(f);
  await claim(f, [first]);
  const next = { ...check(f), amount: 100n };
  const prepared = await prepareClaim(f, [next]);
  const initial = await state(f, [first, next]);
  const initialWindow = initial.window as Record<string, bigint>;
  await managerCall(f, "set_currency_token_limit", { token: f.currency, limit: 500n });
  const reduced = await state(f, [first, next]);
  expect(reduced.window).toEqual({ ...initialWindow, claim_limit_per_cooldown: 500n });
  await submitFailure(await freshEnvelope(prepared, f.relayer), f.relayer, { contractCode: 6304 });
  expect(await state(f, [first, next])).toEqual(reduced);
  const zero = await managerCall(f, "set_currency_token_limit", { token: f.currency, limit: 0n });
  expect(eventData(zero, f.manager, "token_limit_updated")).toEqual({ topics: ["token_limit_updated"], data: { token: f.currency, limit: 0n } });
  expect(await read(f.manager, "currency_limits", [address(f.currency)], f.relayer)).toEqual({ ...initialWindow, claim_limit_per_cooldown: 0n });
  await rejectSimulation(f.relayer, f.operation(0, f.manager, "set_currency_token_limit", { caller: f.admin.publicKey(), token: f.currency, limit: 1_500n }), 6300);
  const added = await managerCall(f, "add_currency_limit", { token: f.currency, limit: 1_500n });
  expect(eventData(added, f.manager, "token_limit_added")).toEqual({ topics: ["token_limit_added"], data: { token: f.currency, limit: 1_500n } });
  const reset = await state(f, [first, next]);
  expect(reset.proofs).toEqual([true, false]);
  expect(reset.user).toBe(1_000n);
  expect(reset.balances).toEqual(initial.balances);
  expect(reset.native).toEqual(initial.native);
  expect((reset.window as Record<string, bigint>).cumulative_claim_per_cooldown).toBe(0n);
  expect((reset.window as Record<string, bigint>).claim_cooldown_period_started).toBeGreaterThanOrEqual(initialWindow.claim_cooldown_period_started!);
  await claim(f, [next]);
  expect((await state(f, [first, next])).proofs).toEqual([true, true]);
  expect(await read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(f.currency)], f.relayer)).toBe(1_100n);
}, 300_000);

test.serial("C3 final zero-fee collector receives one aggregate payment and altered payer auth rolls back the batch", async () => {
  const f = await fixture();
  const collector = await fundedAccount();
  const factory = await f.deploy(1, { admin: f.admin.publicKey(), manager: f.manager, fee_collector: collector.publicKey(), project_wasm_hash: f.projectWasmHash });
  await f.call(1, factory, "set_default_native_claim_fee", { caller: f.admin.publicKey(), value: 0n }, [f.admin]);
  await f.call(1, factory, "set_default_project_claim_fee", { caller: f.admin.publicKey(), value_bps: 0 }, [f.admin]);
  const created = await f.call(1, factory, "create_fuul_project", { project_admin: f.admin.publicKey(), project_info_uri: "ipfs://last-collector", kyc_required: false }, [f.admin]);
  const second = scValToNative(created.value!) as string;
  await invoke(f.relayer, new Contract(f.currency).call("mint", address(second), amount(10_000n)), [f.admin]);
  await f.call(1, f.factory, "set_native_user_claim_fee", { caller: f.admin.publicKey(), project: f.project, value: 10n }, [f.admin]);
  const checks = [check(f), check(f, second)];
  const prepared = await prepareClaim(f, checks);
  const before = await state(f, checks);
  const collectorBefore = await read(f.native, "balance", [address(collector.publicKey())], f.relayer) as bigint;
  for (const mutation of ["missing", "amount", "collector"] as const) {
    const invalid = await freshEnvelope(prepared, f.relayer, async entries => {
      const entry = signerEntry(entries, f.caller);
      const root = entry.rootInvocation();
      expect(root.subInvocations()).toHaveLength(1);
      const fee = root.subInvocations()[0]!.function().contractFn();
      expect(Address.fromScAddress(fee.contractAddress()).toString()).toBe(f.native);
      expect(fee.functionName().toString()).toBe("transfer");
      expect(fee.args().map(value => scValToNative(value))).toEqual([f.caller.publicKey(), collector.publicKey(), 10n]);
      if (mutation === "missing") root.subInvocations([]);
      else fee.args()[mutation === "amount" ? 2 : 1] = mutation === "amount" ? amount(9n) : address(f.collector.publicKey());
      entries[entries.indexOf(entry)] = await authorizeEntry(entry, f.caller, entry.credentials().address().signatureExpirationLedger(), PASSPHRASE);
    });
    await submitFailure(invalid, f.relayer, { authMessage: /Unauthorized function call/, attemptedTransfer: { currency: f.currency, from: f.project } });
    expect(await state(f, checks)).toEqual(before);
    expect(await read(f.native, "balance", [address(collector.publicKey())], f.relayer)).toBe(collectorBefore);
  }
  const receipt = await claim(f, checks);
  const nativeEvents = receipt.events.filter(event => {
    const hash = event.contractId();
    return hash instanceof Uint8Array && Buffer.from(hash).equals(StrKey.decodeContract(f.native));
  });
  expect(nativeEvents).toHaveLength(1);
  expect(nativeEvents[0]!.body().v0().topics().slice(0, 3).map(value => scValToNative(value))).toEqual(["transfer", f.caller.publicKey(), collector.publicKey()]);
  expect(scValToNative(nativeEvents[0]!.body().v0().data())).toBe(10n);
  const after = await state(f, checks);
  expect(after.proofs).toEqual([true, true]);
  expect(after.user).toBe(2_000n);
  expect(after.native).toEqual([(before.native[0] as bigint) - 10n, before.native[1]]);
  expect(await read(f.native, "balance", [address(collector.publicKey())], f.relayer)).toBe(collectorBefore + 10n);
}, 300_000);

test.serial("admin grants and revocations enforce caller signatures and preserve operational roles", async () => {
  const f = await fixture();
  const successor = await fundedAccount();
  const roles = async () => Promise.all([[f.pauser, "pauser"], [f.unpauser, "unpauser"], [f.signer, "claim_signer"]].map(([key, role]) =>
    read(f.manager, "has_role", [nativeToScVal(role as string, { type: "symbol" }), address((key as Keypair).publicKey())], f.relayer)));
  const originalRoles = await roles();
  const cyclePause = async () => {
    await managerCall(f, "pause", { caller: f.pauser.publicKey() }, [f.pauser]);
    expect(await read(f.manager, "paused", [], f.relayer)).toBe(true);
    await managerCall(f, "unpause", { caller: f.unpauser.publicKey() }, [f.unpauser]);
    expect(await read(f.manager, "paused", [], f.relayer)).toBe(false);
  };
  await cyclePause();
  const granted = await managerCall(f, "grant_role", { role: "default_admin", account: successor.publicKey() });
  expect(eventData(granted, f.manager, "role_granted")).toEqual({ topics: ["role_granted", "default_admin", successor.publicKey()], data: { caller: f.admin.publicKey() } });
  const members = () => read(f.manager, "get_role_members", [nativeToScVal("default_admin", { type: "symbol" })], f.relayer);
  expect(await members()).toEqual([f.admin.publicKey(), successor.publicKey()]);
  await managerCall(f, "set_claim_cooldown", { period: 86_401n });
  const revocation = await prepare(f.relayer, f.operation(0, f.manager, "revoke_role", { role: "default_admin", account: f.admin.publicKey(), caller: successor.publicKey() }), [successor]);
  await submitFailure(await wrongAccountSignature(revocation, f, successor, f.admin), f.relayer, { authMessage: /signer does not belong to account/ });
  expect(await members()).toEqual([f.admin.publicKey(), successor.publicKey()]);
  const revoked = await managerCall(f, "revoke_role", { role: "default_admin", account: f.admin.publicKey() }, [successor]);
  expect(eventData(revoked, f.manager, "role_revoked")).toEqual({ topics: ["role_revoked", "default_admin", f.admin.publicKey()], data: { caller: successor.publicKey() } });
  expect(await members()).toEqual([successor.publicKey()]);
  await rejectSimulation(f.relayer, f.operation(0, f.manager, "set_claim_cooldown", { caller: f.admin.publicKey(), period: 86_402n }), 2000);
  const setter = await prepare(f.relayer, f.operation(0, f.manager, "set_claim_cooldown", { caller: successor.publicKey(), period: 86_402n }), [successor]);
  await submitFailure(await wrongAccountSignature(setter, f, successor, f.admin), f.relayer, { authMessage: /signer does not belong to account/ });
  expect(await read(f.manager, "claim_cooldown", [], f.relayer)).toBe(86_401n);
  await managerCall(f, "set_claim_cooldown", { period: 86_402n }, [successor]);
  expect(await read(f.manager, "claim_cooldown", [], f.relayer)).toBe(86_402n);
  expect(await roles()).toEqual(originalRoles);
  await cyclePause();
}, 300_000);
