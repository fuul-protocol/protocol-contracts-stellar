import { measureClaimResources } from "../support/resources.js";
import { Account, Address, Asset, Contract, contract, nativeToScVal, Operation, rpc, scValToNative, Transaction, TransactionBuilder, xdr } from "@stellar/stellar-sdk";
import { expect, test } from "bun:test";
import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { address, amount, fixture, read } from "../support/fixtures.js";
import { fundedAccount, guardNetwork, PASSPHRASE, PROTOCOL, server } from "../support/localnet.js";
import { invoke, prepare, submit, transaction } from "../support/transactions.js";

type Fixture = Awaited<ReturnType<typeof fixture>>;
type Kind = "StellarAsset" | "NonFungible" | "MultiToken";
type Currency = { name: string; id: string; kind: Kind };
type Project = { id: string; bps: bigint; nativeFee: bigint };
type Check = {
  project_address: string; to: string; currency: string; currency_type: { tag: Kind; values: undefined };
  amount: bigint; token_id: bigint; reason: { tag: "AffiliatePayout"; values: undefined };
  deadline: bigint; proof: Buffer; signers: string[];
};
type Snapshot = { ledgerBefore: number; ledgerAfter: number; values: Record<string, bigint | string | boolean> };
type Query = { id: string; method: string; args: xdr.ScVal[]; select?: (value: any) => unknown };
const u32 = (value: bigint) => nativeToScVal(Number(value), { type: "u32" });
const json = (value: unknown) => JSON.stringify(value, (_, item) => typeof item === "bigint" ? item.toString() : item, 2) + "\n";
const key = (...parts: (string | bigint)[]) => parts.join(":");

// Read-only observations share a source sequence, with a separate Account object
// per simulation. No source signing or submission occurs in this observer.
async function snapshot(f: Fixture, checks: Check[]): Promise<Snapshot> {
  await guardNetwork();
  const queries = new Map<string, Query>();
  const balance = (currency: string, owner: string) => queries.set(key("balance", currency, owner),
    { id: currency, method: "balance", args: [address(owner)] });
  balance(f.native, f.caller.publicKey());
  balance(f.native, f.collector.publicKey());
  for (const check of checks) {
    const { currency, project_address: project, to, token_id: id } = check;
    if (check.currency_type.tag === "StellarAsset") {
      for (const owner of [project, to, f.collector.publicKey()]) balance(currency, owner);
    } else if (check.currency_type.tag === "NonFungible") {
      queries.set(key("owner", currency, id), { id: currency, method: "owner_of", args: [u32(id)] });
    } else {
      for (const owner of [project, to]) queries.set(key("multi", currency, owner, id),
        { id: currency, method: "balance", args: [address(owner), u32(id)] });
    }
    queries.set(key("user", currency, to), { id: f.manager, method: "users_claims", args: [address(to), address(currency)] });
    for (const field of ["claim_limit_per_cooldown", "cumulative_claim_per_cooldown", "claim_cooldown_period_started"]) {
      queries.set(key("limit", currency, field), { id: f.manager, method: "currency_limits", args: [address(currency)],
        select: value => value[field] });
    }
    queries.set(key("proof", project, check.proof.toString("hex")),
      { id: project, method: "claimed_proofs", args: [nativeToScVal(check.proof)] });
  }
  const ledgerBefore = (await server.getLatestLedger()).sequence;
  const source = await server.getAccount(f.relayer.publicKey());
  // Cache identical getters so each currency-limit record is observed once.
  const observations = new Map<string, Promise<unknown>>();
  const values = Object.fromEntries(await Promise.all([...queries].map(async ([name, query]) => {
    const operation = new Contract(query.id).call(query.method, ...query.args);
    const encoded = operation.toXDR("base64");
    if (!observations.has(encoded)) observations.set(encoded, (async () => {
      const tx = new TransactionBuilder(new Account(source.accountId(), source.sequenceNumber()), { networkPassphrase: PASSPHRASE, fee: "100" })
        .addOperation(operation).setTimeout(90).build();
      const result = await server.simulateTransaction(tx);
      assert(rpc.Api.isSimulationSuccess(result) && result.result, `Snapshot getter failed: ${query.method}`);
      return scValToNative(result.result.retval);
    })());
    const raw = await observations.get(encoded)!;
    const value = query.select ? query.select(raw) : raw;
    assert(["bigint", "string", "boolean"].includes(typeof value), `Unexpected snapshot value: ${name}`);
    return [name, value as bigint | string | boolean];
  })));
  return { ledgerBefore, ledgerAfter: (await server.getLatestLedger()).sequence, values };
}

// Independent accounting uses only the workload and explicitly configured fees.
// The source relayer pays gas; the caller's native balance measures business fees.
function expectedSettlement(f: Fixture, projects: Project[], checks: Check[], before: Snapshot) {
  const expected = { ...before.values };
  const add = (name: string, delta: bigint) => {
    assert.equal(typeof expected[name], "bigint", `Missing numeric observation: ${name}`);
    expected[name] = (expected[name] as bigint) + delta;
  };
  for (const check of checks) {
    const project = projects.find(item => item.id === check.project_address)!;
    assert(project);
    const { currency, to, amount: quantity, token_id: id } = check;
    const proofKey = key("proof", project.id, check.proof.toString("hex"));
    assert.equal(expected[proofKey], false, "Each measured claim must use a fresh proof");
    expected[proofKey] = true;
    add(key("user", currency, to), quantity);
    add(key("limit", currency, "cumulative_claim_per_cooldown"), quantity);
    add(key("balance", f.native, f.caller.publicKey()), -project.nativeFee);
    add(key("balance", f.native, f.collector.publicKey()), project.nativeFee);
    if (check.currency_type.tag === "StellarAsset") {
      const fee = quantity * project.bps / 10_000n;
      add(key("balance", currency, project.id), -quantity - fee);
      add(key("balance", currency, to), quantity);
      add(key("balance", currency, f.collector.publicKey()), fee);
    } else if (check.currency_type.tag === "NonFungible") {
      assert.equal(expected[key("owner", currency, id)], project.id);
      expected[key("owner", currency, id)] = to;
    } else {
      add(key("multi", currency, project.id, id), -1n);
      add(key("multi", currency, to, id), 1n);
    }
  }
  return expected;
}

async function adapter(f: Fixture, folder: string, name: string, constructor: object, evidence: unknown[]): Promise<string> {
  const wasm = await readFile(new URL(`../../../target/wasm32v1-none/release/${name}.wasm`, import.meta.url));
  const hash = createHash("sha256").update(wasm).digest();
  const sources = await Promise.all(["Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
    `tests/fixtures/${folder}/Cargo.toml`, `tests/fixtures/${folder}/src/lib.rs`].map(async path =>
    ({ path, sha256: createHash("sha256").update(await readFile(new URL(`../../../${path}`, import.meta.url))).digest("hex") })));
  const spec = contract.Spec.fromWasm(wasm);
  const upload = await invoke(f.relayer, Operation.uploadContractWasm({ wasm }));
  expect(scValToNative(upload.value!)).toEqual(hash);
  const uploaded = await server.getLedgerEntries(xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({ hash })));
  expect(uploaded.entries).toHaveLength(1);
  expect(createHash("sha256").update(uploaded.entries[0]!.val.contractCode().code()).digest()).toEqual(hash);
  const deployed = await invoke(f.relayer, Operation.createCustomContract({ address: new Address(f.relayer.publicKey()), wasmHash: hash,
    constructorArgs: spec.funcArgsToScVals("__constructor", constructor) }));
  const id = scValToNative(deployed.value!) as string;
  evidence.push({ name, id, wasmSha256: hash.toString("hex"), sources, upload: upload.hash, deploy: deployed.hash });
  return id;
}

function preparationBoundary(error: unknown): string | undefined {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("Error(Auth, InvalidAction)") && message.includes("previous invocation is missing - no auth data to get")) return "simulation-auth-recording-error";
  if (/Error\(Budget,\s*ExceededLimit\)/.test(message)) return "simulation-budget";
  if (/Local transaction fee [0-9]+ exceeds test budget/.test(message)) return "fee-ceiling";
  return undefined;
}

test.serial("mixed claim workloads settle exactly within the measured signed resource bounds", async () => {
  const f = await fixture();
  const rows: Record<string, unknown>[] = [], adapters: unknown[] = [], profiles: Record<string, unknown>[] = [];
  const directory = new URL("../../../.local/resource/", import.meta.url);
  await mkdir(directory, { recursive: true });
  const report: Record<string, unknown> = { protocol: PROTOCOL, networkPassphrase: PASSPHRASE,
    startedAt: new Date().toISOString(), completed: false, feeCeilingStroops: "250000000",
    contracts: { manager: f.manager, factory: f.factory }, adapters, profiles, rows,
    limitations: ["Power-of-two samples, plus five claims for all assets, establish observed successes and boundaries, not an exact maximum or a universal batch cap.",
      "KYC uses a test provider with one allowed recipient. Other adapters and custody configurations require separate qualification.",
      "State getters observe a ledger window, not an atomic state proof. All Projects share one Factory and fee collector.",
      "Local network measurements do not approve Mainnet capacity. Setup mints are separate from measured claim effects."] };
  try {
    const additionalSigners = [await fundedAccount(), await fundedAccount()];
    for (const signer of additionalSigners) await f.call(0, f.manager, "grant_role",
      { caller: f.admin.publicKey(), role: "claim_signer", account: signer.publicKey() }, [f.admin]);
    const recipients = [f.recipient, await fundedAccount(), await fundedAccount(), await fundedAccount()];
    for (const recipient of recipients.slice(1)) await submit(await transaction(f.relayer,
      Operation.changeTrust({ asset: new Asset("MGRTEST", f.admin.publicKey()), source: recipient.publicKey() })), f.relayer, [recipient]);
    const projects: Project[] = [{ id: f.project, bps: 100n, nativeFee: 20_000n }];
    for (let i = 1; i < 4; i++) {
      const result = await f.call(1, f.factory, "create_fuul_project", { project_admin: f.admin.publicKey(),
        project_info_uri: `ipfs://capacity-project-${i}`, kyc_required: false }, [f.admin]);
      projects.push({ id: scValToNative(result.value!) as string, bps: [100n, 250n, 1_000n, 0n][i]!, nativeFee: [20_000n, 40_000n, 0n, 10_000n][i]! });
    }
    for (const project of projects) {
      if (project.bps !== 100n) await f.call(1, f.factory, "set_project_claim_fee",
        { caller: f.admin.publicKey(), project: project.id, value_bps: Number(project.bps) }, [f.admin]);
      if (project.nativeFee !== 20_000n) await f.call(1, f.factory, "set_native_user_claim_fee",
        { caller: f.admin.publicKey(), project: project.id, value: project.nativeFee }, [f.admin]);
      const information = await read(f.factory, "get_fees_information", [address(project.id)], f.relayer) as any;
      expect(information.fees.project_claim_fee).toBe(Number(project.bps));
      expect(information.fees.native_user_claim_fee).toBe(project.nativeFee);
      expect(information.fee_collector).toBe(f.collector.publicKey());
    }
    const fungible = await adapter(f, "fungible-token", "fuul_e2e_fungible_fixture", { admin: f.admin.publicKey(), decimals: 7 }, adapters);
    const nft = await adapter(f, "non-fungible-token", "fuul_e2e_nft_fixture", { issuer: f.admin.publicKey() }, adapters);
    const multi = await adapter(f, "multi-token", "fuul_e2e_multi_token_fixture", { issuer: f.admin.publicKey() }, adapters);
    const kyc = await adapter(f, "kyc-provider", "fuul_e2e_kyc_fixture",
      { admin: f.admin.publicKey(), expected_recipient: f.recipient.publicKey(), mode: 1 }, adapters);
    const currencies: Currency[] = [
      { name: "SAC", id: f.currency, kind: "StellarAsset" }, { name: "native", id: f.native, kind: "StellarAsset" },
      { name: "custom-fungible", id: fungible, kind: "StellarAsset" },
      { name: "NFT", id: nft, kind: "NonFungible" }, { name: "multitoken", id: multi, kind: "MultiToken" },
    ];
    // The Manager constructor already registers both SAC and native currency.
    for (const currency of currencies.slice(2)) await f.call(0, f.manager, "add_currency_limit",
      { caller: f.admin.publicKey(), token: currency.id, limit: 1_000_000_000_000n }, [f.admin]);
    for (const project of projects) {
      for (const currency of [f.currency, fungible]) await invoke(f.relayer,
        new Contract(currency).call("mint", address(project.id), amount(2_000_000n)), [f.admin]);
      await invoke(f.relayer, new Contract(f.native).call("transfer", address(f.admin.publicKey()), address(project.id), amount(2_000_000n)), [f.admin]);
      for (const id of [0n, 1n, 2n, 3n]) await invoke(f.relayer,
        new Contract(multi).call("mint", address(project.id), u32(id), amount(1_000n)), [f.admin]);
    }
    Object.assign(report, { projects, currencies, caller: f.caller.publicKey(), collector: f.collector.publicKey(),
      relayer: f.relayer.publicKey(), recipients: recipients.map(item => item.publicKey()),
      claimSigners: [f.signer, ...additionalSigners].map(item => item.publicKey()) });
    const workloads = [
      { name: "multiple-projects", projects: 4, recipients: 1, currencies: [currencies[0]!], kyc: false },
      { name: "multiple-recipients", projects: 1, recipients: 4, currencies: [currencies[0]!], kyc: false },
      { name: "mixed-fungible", projects: 1, recipients: 1, currencies: currencies.slice(0, 3), kyc: false },
      { name: "combined-fungible", projects: 4, recipients: 4, currencies: currencies.slice(0, 3), kyc: false },
      { name: "NFT", projects: 4, recipients: 4, currencies: [currencies[3]!], kyc: false },
      { name: "multitoken", projects: 4, recipients: 4, currencies: [currencies[4]!], kyc: false },
      { name: "all-assets", projects: 4, recipients: 4, currencies, kyc: false },
      { name: "KYC-mixed-fungible", projects: 4, recipients: 1, currencies: currencies.slice(0, 3), kyc: true },
    ];
    let nextNftId = 1n, requiredSigners = 1;
    for (const workload of workloads) {
      if (workload.kyc) {
        await f.call(0, f.manager, "set_kyc_validator", { caller: f.admin.publicKey(), validator: kyc }, [f.admin]);
        expect(await read(kyc, "is_user_kyc_registered", [address(f.recipient.publicKey())], f.relayer)).toBe(true);
        for (const project of projects) await f.call(2, project.id, "set_kyc_required", { caller: f.admin.publicKey(), required: true }, [f.admin]);
      }
      for (const signerCount of [1, 3]) {
        if (requiredSigners !== signerCount) await f.call(0, f.manager, "set_required_signers",
          { caller: f.admin.publicKey(), value: BigInt(signerCount) }, [f.admin]);
        requiredSigners = signerCount;
        const signers = [f.signer, ...additionalSigners].slice(0, signerCount);
        const profileRows: Record<string, unknown>[] = [];
        profiles.push({ workload: workload.name, signerCount, maximumRequestedProjects: workload.projects,
          maximumRequestedRecipients: workload.recipients, currencies: workload.currencies.map(item => item.name), kyc: workload.kyc, rows: profileRows });
        const counts = workload.name === "all-assets" ? [1, 2, 4, 5, 8, 16, 32] : [1, 2, 4, 8, 16, 32];
        for (const count of counts) {
          const checks: Check[] = Array.from({ length: count }, (_, i) => {
            const currency = workload.currencies[i % workload.currencies.length]!;
            return { project_address: projects[i % workload.projects]!.id, to: recipients[i % workload.recipients]!.publicKey(),
              currency: currency.id, currency_type: { tag: currency.kind, values: undefined },
              amount: currency.kind === "StellarAsset" ? 137n : 1n,
              token_id: currency.kind === "NonFungible" ? nextNftId++ : BigInt(i % 4),
              reason: { tag: "AffiliatePayout", values: undefined }, deadline: BigInt(Math.floor(Date.now() / 1000) + 600),
              proof: randomBytes(32), signers: signers.map(item => item.publicKey()) };
          });
          for (const check of checks.filter(item => item.currency_type.tag === "NonFungible")) await invoke(f.relayer,
            new Contract(nft).call("mint", address(check.project_address), u32(check.token_id)), [f.admin]);
          const before = await snapshot(f, checks);
          const expected = expectedSettlement(f, projects, checks, before);
          const sequenceBefore = (await server.getAccount(f.relayer.publicKey())).sequenceNumber();
          const row: Record<string, unknown> = { workload: workload.name, signerCount, count,
            distinctProjects: new Set(checks.map(item => item.project_address)).size,
            distinctRecipients: new Set(checks.map(item => item.to)).size,
            distinctCurrencies: new Set(checks.map(item => item.currency)).size,
            checks: checks.map(item => ({ ...item, proof: item.proof.toString("hex") })), before, expected, sequenceBefore, submitted: false };
          rows.push(row); profileRows.push(row);
          let prepared: Transaction;
          try { prepared = await prepare(f.relayer, f.operation(0, f.manager, "claim", { caller: f.caller.publicKey(), checks }), [f.caller, ...signers]); }
          catch (error) {
            const boundary = preparationBoundary(error);
            if (!boundary) throw error;
            const after = await snapshot(f, checks);
            const sequenceAfter = (await server.getAccount(f.relayer.publicKey())).sequenceNumber();
            Object.assign(row, { outcome: boundary, error: error instanceof Error ? error.message : String(error), after, sequenceAfter });
            expect(after.values).toEqual(before.values);
            expect(sequenceAfter).toBe(sequenceBefore);
            break;
          }
          const signed = TransactionBuilder.fromXDR(prepared.toXDR(), PASSPHRASE);
          assert(signed instanceof Transaction);
          signed.sign(f.relayer);
          const resources = await measureClaimResources(signed);
          Object.assign(row, { resources, envelopeXdr: signed.toXDR(), feeStroops: signed.fee, hash: signed.hash().toString("hex") });
          if (resources.violations.length > 0) {
            const after = await snapshot(f, checks);
            const sequenceAfter = (await server.getAccount(f.relayer.publicKey())).sequenceNumber();
            Object.assign(row, { outcome: "network-resource-limit", after, sequenceAfter });
            expect(after.values).toEqual(before.values);
            expect(sequenceAfter).toBe(sequenceBefore);
            break;
          }
          row.submitted = true;
          // submit signs the same prepared transaction once. The measured copy includes that signature.
          const receipt = await submit(prepared, f.relayer);
          const result = await server.getTransaction(receipt.hash);
          expect(result.status).toBe(rpc.Api.GetTransactionStatus.SUCCESS);
          if (result.status !== "SUCCESS") throw new Error("Measured claim was not included successfully");
          Object.assign(row, { outcome: "included-success", ledger: result.ledger, feeChargedStroops: result.resultXdr.feeCharged().toString(),
            includedEnvelopeXdr: result.envelopeXdr.toXDR("base64"), resultXdr: result.resultXdr.toXDR("base64"),
            resultMetaXdr: result.resultMetaXdr.toXDR("base64") });
          expect(result.envelopeXdr.toXDR("base64")).toBe(signed.toXDR());
          const after = await snapshot(f, checks);
          row.after = after;
          expect(after.values).toEqual(expected);
          const sequenceAfter = (await server.getAccount(f.relayer.publicKey())).sequenceNumber();
          row.sequenceAfter = sequenceAfter;
          expect(BigInt(sequenceAfter)).toBe(BigInt(sequenceBefore) + 1n);
          console.info(json({ stage: "claim-workload", protocol: PROTOCOL, workload: workload.name, signerCount, count,
            outcome: row.outcome, hash: row.hash, usage: resources.usage }).trim());
        }
        expect(profileRows.some(row => row.outcome === "included-success")).toBe(true);
      }
    }
    report.completed = true;
  } catch (error) {
    report.failure = error instanceof Error ? error.message : String(error);
    throw error;
  } finally {
    report.finishedAt = new Date().toISOString();
    const output = new URL(`matrix-${PROTOCOL}-${f.project}.json`, directory);
    await writeFile(output, json(report), { flag: "wx" });
    console.info(json({ stage: "claim-workload-report", output: output.pathname, completed: report.completed }).trim());
  }
}, 900_000);
