import { rpc, Transaction, TransactionBuilder, xdr } from "@stellar/stellar-sdk";
import { expect, test } from "bun:test";
import { randomBytes } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { address, fixture, read } from "../support/fixtures.js";
import { fundedAccount, guardNetwork, PASSPHRASE, PROTOCOL, server } from "../support/localnet.js";
import { prepare, submit } from "../support/transactions.js";

test.serial("signed claim batches respect measured network and fee bounds with exact accounting", async () => {
  await guardNetwork();
  const configIds = [xdr.ConfigSettingId.configSettingContractComputeV0(), xdr.ConfigSettingId.configSettingContractLedgerCostV0(),
    xdr.ConfigSettingId.configSettingContractBandwidthV0(), xdr.ConfigSettingId.configSettingContractEventsV0(),
    xdr.ConfigSettingId.configSettingContractLedgerCostExtV0()];
  const settings = await server.getLedgerEntries(...configIds.map(configSettingId => xdr.LedgerKey.configSetting(new xdr.LedgerKeyConfigSetting({ configSettingId }))));
  expect(settings.entries).toHaveLength(configIds.length);
  const bandwidth = settings.entries.find(e => e.val.configSetting().switch().name === "configSettingContractBandwidthV0")!
    .val.configSetting().contractBandwidth();
  const compute = settings.entries.find(e => e.val.configSetting().switch().name === "configSettingContractComputeV0")!
    .val.configSetting().contractCompute();
  const ledgerCost = settings.entries.find(e => e.val.configSetting().switch().name === "configSettingContractLedgerCostV0")!
    .val.configSetting().contractLedgerCost();
  const ledgerCostExt = settings.entries.find(e => e.val.configSetting().switch().name === "configSettingContractLedgerCostExtV0")!
    .val.configSetting().contractLedgerCostExt();
  console.info(JSON.stringify({ stage: "claim-capacity-settings", protocol: PROTOCOL, ledger: settings.latestLedger,
    txMaxInstructions: compute.txMaxInstructions().toString(), txMemoryLimit: compute.txMemoryLimit(), txMaxEnvelopeBytes: bandwidth.txMaxSizeBytes(),
    entries: settings.entries.map(e => ({ keyXdr: e.key.toXDR("base64"), valueXdr: e.val.toXDR("base64") })) }));
  const f = await fixture();
  const extraSigners = [await fundedAccount(), await fundedAccount()];
  for (const signer of extraSigners) await f.call(0, f.manager, "grant_role", { caller: f.admin.publicKey(), role: "claim_signer", account: signer.publicKey() }, [f.admin]);
  const snapshot = () => Promise.all([
    read(f.currency, "balance", [address(f.project)], f.relayer),
    read(f.currency, "balance", [address(f.recipient.publicKey())], f.relayer),
    read(f.currency, "balance", [address(f.collector.publicKey())], f.relayer),
    read(f.manager, "users_claims", [address(f.recipient.publicKey()), address(f.currency)], f.relayer),
  ]);
  expect(await snapshot()).toEqual([101_000n, 0n, 0n, 0n]);
  let totalPaid = 0n;
  const profiles: unknown[] = [];
  for (const signerCount of [1, 2, 3]) {
    if (signerCount > 1) await f.call(0, f.manager, "set_required_signers", { caller: f.admin.publicKey(), value: BigInt(signerCount) }, [f.admin]);
    const signers = [f.signer, ...extraSigners].slice(0, signerCount);
    const rows: Record<string, unknown>[] = [];
    for (const count of [1, 2, 4, 8, 16, 32, 64]) {
      const checks = Array.from({ length: count }, () => ({ project_address: f.project, to: f.recipient.publicKey(), currency: f.currency,
        currency_type: { tag: "StellarAsset", values: undefined }, amount: 100n, token_id: 0n,
        reason: { tag: "AffiliatePayout", values: undefined }, deadline: BigInt(Math.floor(Date.now() / 1000) + 600),
        proof: randomBytes(32), signers: signers.map(signer => signer.publicKey()) }));
      const before = await snapshot();
      const sequenceBefore = (await server.getAccount(f.relayer.publicKey())).sequenceNumber();
      let prepared: Transaction;
      try {
        prepared = await prepare(f.relayer, f.operation(0, f.manager, "claim", { caller: f.caller.publicKey(), checks }), [f.caller, ...signers]);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        // Record this specific upstream diagnostic without relabeling it as a budget error.
        // It occurs during record-mode authentication after the contract returns.
        const recordingDiagnostic = message.includes("Error(Auth, InvalidAction)") && message.includes("previous invocation is missing - no auth data to get");
        if (!recordingDiagnostic && !/Error\(Budget,\s*ExceededLimit\)|Local transaction fee [0-9]+ exceeds test budget/.test(message)) throw error;
        expect(count).toBeGreaterThan(1);
        expect(await snapshot()).toEqual(before);
        expect((await server.getAccount(f.relayer.publicKey())).sequenceNumber()).toBe(sequenceBefore);
        rows.push({ count, outcome: recordingDiagnostic ? "simulation-auth-recording-error" : message.includes("exceeds test budget") ? "fee-ceiling" : "simulation-budget", error: message, submitted: false });
        break;
      }
      const signed = TransactionBuilder.fromXDR(prepared.toXDR(), PASSPHRASE);
      expect(signed).toBeInstanceOf(Transaction);
      if (!(signed instanceof Transaction)) throw new Error("Expected a normal transaction");
      signed.sign(f.relayer);
      const envelopeBytes = signed.toEnvelope().toXDR().length;
      const data = prepared.toEnvelope().v1().tx().ext().sorobanData(), resources = data.resources();
      const footprint = [...resources.footprint().readOnly(), ...resources.footprint().readWrite()];
      const diskReadEntries = footprint.filter(key => !["contractData", "contractCode"].includes(key.switch().name)).length
        + (data.ext().switch() === 1 ? data.ext().resourceExt().archivedSorobanEntries().length : 0);
      const measured = { count, feeStroops: prepared.fee, instructions: resources.instructions(), envelopeBytes,
        diskReadEntries, diskReadBytes: resources.diskReadBytes(), writeBytes: resources.writeBytes(),
        readOnlyEntries: resources.footprint().readOnly().length, readWriteEntries: resources.footprint().readWrite().length };
      const limits = [
        ["envelopeBytes", envelopeBytes, bandwidth.txMaxSizeBytes()],
        ["instructions", resources.instructions(), Number(compute.txMaxInstructions().toBigInt())],
        ["writeEntries", resources.footprint().readWrite().length, ledgerCost.txMaxWriteLedgerEntries()],
        ["footprintEntries", footprint.length, ledgerCostExt.txMaxFootprintEntries()],
        ["diskReadEntries", diskReadEntries, ledgerCost.txMaxDiskReadEntries()],
        ["diskReadBytes", resources.diskReadBytes(), ledgerCost.txMaxDiskReadBytes()],
        ["writeBytes", resources.writeBytes(), ledgerCost.txMaxWriteBytes()],
      ] as const;
      const violations = limits.filter(([, actual, maximum]) => actual > maximum)
        .map(([resource, actual, maximum]) => ({ resource, actual, maximum }));
      if (violations.length > 0) {
        expect(await snapshot()).toEqual(before);
        expect((await server.getAccount(f.relayer.publicKey())).sequenceNumber()).toBe(sequenceBefore);
        rows.push({ ...measured, outcome: "network-resource-limit", violations, submitted: false });
        break;
      }
      expect(BigInt(resources.instructions())).toBeLessThanOrEqual(compute.txMaxInstructions().toBigInt());
      const receipt = await submit(prepared, f.relayer);
      totalPaid += BigInt(count);
      expect(await snapshot()).toEqual([101_000n - totalPaid * 101n, totalPaid * 100n, totalPaid, totalPaid * 100n]);
      const result = await server.getTransaction(receipt.hash);
      expect(result.status).toBe(rpc.Api.GetTransactionStatus.SUCCESS);
      if (result.status !== "SUCCESS") throw new Error("Measured batch is not included successfully");
      rows.push({ ...measured, outcome: "included-success", submitted: true, hash: receipt.hash, ledger: receipt.ledger,
        feeChargedStroops: result.resultXdr.feeCharged().toString(), envelopeXdr: result.envelopeXdr.toXDR("base64"),
        resultXdr: result.resultXdr.toXDR("base64"), resultMetaXdr: result.resultMetaXdr.toXDR("base64") });
      console.info(JSON.stringify({ stage: "signed-claim-capacity", protocol: PROTOCOL, signerCount, ...measured, hash: receipt.hash }));
    }
    expect(rows.some(row => row.outcome === "included-success" && row.count === 8)).toBe(true);
    profiles.push({ signerCount, rows });
  }
  const directory = new URL("../../../.local/resource/", import.meta.url);
  await mkdir(directory, { recursive: true });
  await writeFile(new URL(`claims-${PROTOCOL}-${f.project}.json`, directory), JSON.stringify({
    protocol: PROTOCOL, networkPassphrase: PASSPHRASE, scenario: "One Project, one SAC currency, one recipient, no KYC; 1, 2, and 3 real claim signers",
    feeCeilingStroops: "250000000", preparedWithActualAuthorizationSignatures: true,
    limitations: ["Other project, recipient, currency, or adapter combinations require separate measurements.", "Local measurements do not approve Mainnet capacity or a universal batch limit."],
    settings: { ledger: settings.latestLedger, entries: settings.entries.map(e => ({ keyXdr: e.key.toXDR("base64"), valueXdr: e.val.toXDR("base64") })) },
    contracts: { manager: f.manager, factory: f.factory, project: f.project }, profiles,
  }, null, 2) + "\n", { flag: "wx" });
}, 300_000);
