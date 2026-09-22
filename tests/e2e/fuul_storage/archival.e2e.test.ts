import { Address, nativeToScVal, Operation, rpc, SorobanDataBuilder, TransactionBuilder, xdr } from "@stellar/stellar-sdk";
import { expect, test } from "bun:test";
import { createHash, randomBytes } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { address, fixture, read } from "../support/fixtures.js";
import { guardNetwork, PASSPHRASE, server } from "../support/localnet.js";
import { rejectSimulation, submit } from "../support/transactions.js";

test.serial("physical archival and explicit restoration preserve a paid core proof and reject a second payout", async () => {
  await guardNetwork();
  expect(process.env.FUUL_ARCHIVAL_IMAGE).toMatch(/^sha256:[0-9a-f]{64}$/);
  const settingsKey = xdr.LedgerKey.configSetting(new xdr.LedgerKeyConfigSetting({ configSettingId: xdr.ConfigSettingId.configSettingStateArchival() }));
  const settings = await server.getLedgerEntries(settingsKey);
  const archival = settings.entries[0]!.val.configSetting().stateArchivalSettings();
  expect(archival.minPersistentTtl()).toBe(16);
  expect(archival.maxEntryTtl()).toBe(128);
  const f = await fixture();
  const check = { project_address: f.project, to: f.recipient.publicKey(), currency: f.currency,
    currency_type: { tag: "StellarAsset", values: undefined }, amount: 1_000n, token_id: 0n,
    reason: { tag: "EndUserPayout", values: undefined }, deadline: BigInt(Math.floor(Date.now() / 1000) + 600),
    proof: randomBytes(32), signers: [f.signer.publicKey()] };
  const claim = () => f.operation(0, f.manager, "claim", { caller: f.caller.publicKey(), checks: [check] });
  const paid = await f.call(0, f.manager, "claim", { caller: f.caller.publicKey(), checks: [check] }, [f.caller, f.signer]);
  const included = await server.getTransaction(paid.hash);
  expect(included.status).toBe(rpc.Api.GetTransactionStatus.SUCCESS);
  if (included.status !== "SUCCESS") throw new Error("Claim was not included successfully");
  const dataKey = xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({
    contract: new Address(f.project).toScAddress(),
    key: xdr.ScVal.scvVec([xdr.ScVal.scvSymbol("ClaimedProof"), nativeToScVal(check.proof)]),
    durability: xdr.ContractDataDurability.persistent(),
  }));
  const keyXdr = dataKey.toXDR("base64");
  const ttlKey = xdr.LedgerKey.ttl(new xdr.LedgerKeyTtl({ keyHash: createHash("sha256").update(dataKey.toXDR()).digest() }));
  const live = await server.getLedgerEntries(dataKey);
  expect(live.entries).toHaveLength(1);
  expect(live.entries[0]!.val.contractData().val().b()).toBe(true);
  const valueBefore = live.entries[0]!.val.contractData().val().toXDR("base64");
  const expiresAt = live.entries[0]!.liveUntilLedgerSeq!;
  expect(expiresAt).toBe(paid.ledger + 127);

  // Keep every other persistent entry from the actual claim footprint alive.
  // Excluding only the paid proof isolates its archival from roles, code, and balances.
  const footprint = included.envelopeXdr.v1().tx().ext().sorobanData().resources().footprint();
  const candidates = [...footprint.readOnly(), ...footprint.readWrite()].filter(key => key.toXDR("base64") !== keyXdr
    && (key.switch().name === "contractCode" || (key.switch().name === "contractData" && key.contractData().durability().name === "persistent")));
  // A read footprint can include absent optional values, such as FeeExemption.
  // Renew only real entries; retain the absent keys as part of the observation.
  const baseline = await server.getLedgerEntries(...candidates);
  const candidateIds = new Set(candidates.map(key => key.toXDR("base64")));
  const retained = baseline.entries.map(entry => {
    expect(candidateIds.has(entry.key.toXDR("base64"))).toBe(true);
    expect(entry.liveUntilLedgerSeq!).toBeGreaterThanOrEqual(baseline.latestLedger);
    return entry.key;
  });
  const presentIds = new Set(retained.map(key => key.toXDR("base64")));
  const absentKeys = candidates.filter(key => !presentIds.has(key.toXDR("base64")));
  expect(retained.length).toBeGreaterThan(10);
  expect(new Set(retained.map(key => key.toXDR("base64"))).size).toBe(retained.length);
  const maintenance: { hash: string; ledger: number }[] = [];
  async function lifecycle(keys: xdr.LedgerKey[], action: { kind: "restore" } | { kind: "extend"; extendTo: number }) {
    await guardNetwork();
    const footprint = new SorobanDataBuilder();
    if (action.kind === "restore") footprint.setReadWrite(keys);
    else footprint.setReadOnly(keys);
    const raw = new TransactionBuilder(await server.getAccount(f.relayer.publicKey()), { networkPassphrase: PASSPHRASE, fee: "100" })
      .addOperation(action.kind === "restore" ? Operation.restoreFootprint({}) : Operation.extendFootprintTtl({ extendTo: action.extendTo }))
      .setSorobanData(footprint.build()).setTimeout(90).build();
    const transaction = await server.prepareTransaction(raw);
    // Simulation can reorder keys. Require identical membership and access modes.
    const actual = transaction.toEnvelope().v1().tx().ext().sorobanData().resources().footprint();
    const expected = raw.toEnvelope().v1().tx().ext().sorobanData().resources().footprint();
    const ids = (entries: xdr.LedgerKey[]) => entries.map(key => key.toXDR("base64")).sort();
    expect(ids(actual.readOnly())).toEqual(ids(expected.readOnly()));
    expect(ids(actual.readWrite())).toEqual(ids(expected.readWrite()));
    expect(BigInt(transaction.fee)).toBeLessThanOrEqual(250_000_000n);
    const receipt = await submit(transaction, f.relayer);
    const hash = receipt.hash;
    const result = await server.getTransaction(hash);
    expect(result.status).toBe(rpc.Api.GetTransactionStatus.SUCCESS);
    if (result.status !== "SUCCESS") throw new Error("Lifecycle transaction was not included successfully");
    maintenance.push({ hash, ledger: result.ledger });
    return { hash, result };
  }
  const snapshot = () => Promise.all([
    read(f.currency, "balance", [address(f.project)], f.relayer),
    read(f.currency, "balance", [address(f.recipient.publicKey())], f.relayer),
    read(f.currency, "balance", [address(f.collector.publicKey())], f.relayer),
    read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(f.currency)], f.relayer),
    read(f.manager, "currency_limits", [address(f.currency)], f.relayer),
  ]);
  const before = await snapshot();
  expect(before.slice(0, 4)).toEqual([99_990n, 1_000n, 10n, 1_000n]);
  async function renewRetained(): Promise<number> {
    const current = await server.getLedgerEntries(...retained);
    expect(current.entries).toHaveLength(retained.length);
    for (const entry of current.entries) expect(entry.liveUntilLedgerSeq!).toBeGreaterThanOrEqual(current.latestLedger);
    // Simulation omits entries that already meet the target. Select only due
    // entries so the test can assert the exact prepared footprint.
    const due = current.entries.filter(entry => entry.liveUntilLedgerSeq! < current.latestLedger + 120).map(entry => entry.key);
    if (due.length === 0) return current.latestLedger;
    return (await lifecycle(due, { kind: "extend", extendTo: 120 })).result.ledger;
  }
  let lastMaintenance = await renewRetained();
  let scanFrom = live.latestLedger;
  let eviction: rpc.Api.LedgerResponse | undefined;
  const deadline = Date.now() + 250_000;
  while (!eviction && Date.now() < deadline) {
    const latest = (await server.getLatestLedger()).sequence;
    if (latest - lastMaintenance >= 30) {
      lastMaintenance = await renewRetained();
    }
    if (scanFrom <= latest) {
      const page = await server.getLedgers({ startLedger: scanFrom, pagination: { limit: 100 } });
      for (const ledger of page.ledgers) {
        expect(ledger.sequence).toBe(scanFrom++);
        expect(ledger.metadataXdr.switch()).toBe(2);
        const keys = ledger.metadataXdr.v2().evictedKeys().map(key => key.toXDR("base64"));
        if (keys.includes(keyXdr)) {
          expect(keys).toContain(ttlKey.toXDR("base64"));
          eviction = ledger;
          break;
        }
      }
    }
    if (!eviction) await Bun.sleep(1000);
  }
  expect(eviction).toBeDefined();
  if (!eviction) throw new Error("Core did not report physical proof eviction within the test deadline");
  expect(eviction.sequence).toBeGreaterThan(expiresAt);
  expect(createHash("sha256").update(eviction.headerXdr.header().toXDR()).digest("hex")).toBe(eviction.hash);
  expect(eviction.metadataXdr.v2().ledgerHeader().toXDR("base64")).toBe(eviction.headerXdr.toXDR("base64"));
  const expired = await server.getLedgerEntries(dataKey);
  expect(expired.latestLedger).toBeGreaterThanOrEqual(eviction.sequence);
  if (expired.entries[0]?.liveUntilLedgerSeq !== undefined) expect(expired.entries[0].liveUntilLedgerSeq).toBeLessThan(expired.latestLedger);
  expect(await snapshot()).toEqual(before);

  const restored = await lifecycle([dataKey], { kind: "restore" });
  expect(restored.result.resultMetaXdr.switch()).toBe(4);
  const restoredEntries = restored.result.resultMetaXdr.v4().operations().flatMap(operation => operation.changes())
    .filter(change => change.switch().name === "ledgerEntryRestored").map(change => change.restored());
  expect(restoredEntries.some(entry => entry.data().switch().name === "contractData"
    && entry.data().contractData().contract().toXDR("base64") === dataKey.contractData().contract().toXDR("base64")
    && entry.data().contractData().key().toXDR("base64") === dataKey.contractData().key().toXDR("base64")
    && entry.data().contractData().val().toXDR("base64") === valueBefore)).toBe(true);
  const after = await server.getLedgerEntries(dataKey);
  expect(after.entries[0]!.val.contractData().val().toXDR("base64")).toBe(valueBefore);
  expect(after.entries[0]!.liveUntilLedgerSeq!).toBeGreaterThan(expiresAt);
  await rejectSimulation(f.relayer, claim(), 6102);
  expect(await snapshot()).toEqual(before);
  const next = { ...check, proof: randomBytes(32) };
  const nextPaid = await f.call(0, f.manager, "claim", { caller: f.caller.publicKey(), checks: [next] }, [f.caller, f.signer]);
  expect((await snapshot()).slice(0, 4)).toEqual([98_980n, 2_000n, 20n, 2_000n]);
  const nextIncluded = await server.getTransaction(nextPaid.hash);
  expect(nextIncluded.status).toBe(rpc.Api.GetTransactionStatus.SUCCESS);
  if (nextIncluded.status !== "SUCCESS") throw new Error("New proof did not produce an included successful claim");
  const evidence = { scope: "Local Protocol 28 with shortened maximum TTL; unchanged current core Wasm", image: process.env.FUUL_ARCHIVAL_IMAGE,
    contracts: { manager: f.manager, factory: f.factory, project: f.project },
    networkPassphrase: PASSPHRASE, archivalSettingsXdr: settings.entries[0]!.val.toXDR("base64"),
    claim: paid.hash, claimLedger: paid.ledger, expiresAt, evictionLedger: eviction.sequence, evictionHash: eviction.hash,
    claimEnvelopeXdr: included.envelopeXdr.toXDR("base64"), claimResultXdr: included.resultXdr.toXDR("base64"),
    claimMetaXdr: included.resultMetaXdr.toXDR("base64"),
    keyXdr, ttlKeyXdr: ttlKey.toXDR("base64"), originalValueXdr: valueBefore,
    evictionHeaderXdr: eviction.headerXdr.toXDR("base64"), evictionMetadataXdr: eviction.metadataXdr.toXDR("base64"),
    restoreHash: restored.hash, restoreLedger: restored.result.ledger,
    restoreEnvelopeXdr: restored.result.envelopeXdr.toXDR("base64"), restoreResultXdr: restored.result.resultXdr.toXDR("base64"),
    restoreMetaXdr: restored.result.resultMetaXdr.toXDR("base64"), replayContractCode: 6102, nextClaim: nextPaid.hash,
    nextClaimEnvelopeXdr: nextIncluded.envelopeXdr.toXDR("base64"), nextClaimResultXdr: nextIncluded.resultXdr.toXDR("base64"),
    nextClaimMetaXdr: nextIncluded.resultMetaXdr.toXDR("base64"),
    maintenance, retainedKeys: retained.map(key => key.toXDR("base64")), absentKeys: absentKeys.map(key => key.toXDR("base64")) };
  const directory = new URL("../../../.local/core-archival/", import.meta.url);
  await mkdir(directory, { recursive: true });
  await writeFile(new URL(`${paid.hash}.json`, directory), JSON.stringify(evidence, null, 2) + "\n");
  console.info(JSON.stringify({ stage: "core-proof-archival", project: f.project, claim: paid.hash, expiresAt,
    evictionLedger: eviction.sequence, evictionHash: eviction.hash, restoreHash: restored.hash, restoreLedger: restored.result.ledger,
    replayContractCode: 6102, nextClaim: nextPaid.hash }));
}, 360_000);
