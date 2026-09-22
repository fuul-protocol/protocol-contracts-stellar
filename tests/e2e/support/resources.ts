import { Transaction, xdr } from "@stellar/stellar-sdk";
import assert from "node:assert/strict";
import { guardNetwork, server } from "./localnet.js";

/** Observe the signed claim envelope and the local node's current resource settings. */
export async function measureClaimResources(transaction: Transaction) {
  await guardNetwork();
  const ids = [xdr.ConfigSettingId.configSettingContractComputeV0(),
    xdr.ConfigSettingId.configSettingContractLedgerCostV0(), xdr.ConfigSettingId.configSettingContractLedgerCostExtV0(),
    xdr.ConfigSettingId.configSettingContractBandwidthV0(), xdr.ConfigSettingId.configSettingContractDataKeySizeBytes()];
  const observed = await server.getLedgerEntries(...ids.map(configSettingId =>
    xdr.LedgerKey.configSetting(new xdr.LedgerKeyConfigSetting({ configSettingId }))));
  assert.equal(observed.entries.length, ids.length);
  const settings = new Map(observed.entries.map(entry => [entry.val.configSetting().switch().name, entry.val.configSetting()]));
  const compute = settings.get("configSettingContractComputeV0")!.contractCompute();
  const cost = settings.get("configSettingContractLedgerCostV0")!.contractLedgerCost();
  const maximum = {
    instructions: compute.txMaxInstructions().toBigInt(), diskReadBytes: BigInt(cost.txMaxDiskReadBytes()),
    writeBytes: BigInt(cost.txMaxWriteBytes()), diskReadEntries: BigInt(cost.txMaxDiskReadEntries()),
    writeEntries: BigInt(cost.txMaxWriteLedgerEntries()),
    footprintEntries: BigInt(settings.get("configSettingContractLedgerCostExtV0")!.contractLedgerCostExt().txMaxFootprintEntries()),
    transactionBytes: BigInt(settings.get("configSettingContractBandwidthV0")!.contractBandwidth().txMaxSizeBytes()),
    largestKeyBytes: BigInt(settings.get("configSettingContractDataKeySizeBytes")!.contractDataKeySizeBytes()),
  };
  const data = transaction.toEnvelope().v1().tx().ext().sorobanData();
  const resource = data.resources(), footprint = [...resource.footprint().readOnly(), ...resource.footprint().readWrite()];
  const archived = data.ext().switch() === 1 ? data.ext().resourceExt().archivedSorobanEntries().length : 0;
  const usage: Record<keyof typeof maximum, bigint> = {
    instructions: BigInt(resource.instructions()), diskReadBytes: BigInt(resource.diskReadBytes()), writeBytes: BigInt(resource.writeBytes()),
    diskReadEntries: BigInt(footprint.filter(key => !["contractData", "contractCode"].includes(key.switch().name)).length + archived),
    writeEntries: BigInt(resource.footprint().readWrite().length), footprintEntries: BigInt(footprint.length),
    transactionBytes: BigInt(transaction.toEnvelope().toXDR().length),
    largestKeyBytes: BigInt(Math.max(0, ...footprint.map(key => key.toXDR().length))),
  };
  const violations = (Object.keys(maximum) as (keyof typeof maximum)[]).filter(key => usage[key] > maximum[key])
    .map(resource => ({ resource, actual: usage[resource].toString(), maximum: maximum[resource].toString() }));
  return { ledger: observed.latestLedger, maximum, usage, violations,
    config: observed.entries.map(entry => ({ keyXdr: entry.key.toXDR("base64"), valueXdr: entry.val.toXDR("base64") })) };
}
