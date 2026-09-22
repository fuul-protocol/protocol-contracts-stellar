import { Address,authorizeEntry,Keypair,Operation,rpc,scValToNative,StrKey,Transaction,TransactionBuilder,xdr } from "@stellar/stellar-sdk";
import assert from "node:assert/strict";
import { guardNetwork,PASSPHRASE,server } from "./localnet.js";

export type Receipt = { hash: string; ledger: number; status: "SUCCESS"; value?: xdr.ScVal; events: xdr.ContractEvent[] };
export const receipts: Receipt[] = [];

async function sendAndPoll(tx: Transaction, source: Keypair, extraSigners: Keypair[] = []) {
  await guardNetwork();
  tx.sign(source, ...extraSigners);
  // Exercise the serialized envelope boundary rather than passing the builder object.
  const envelope = TransactionBuilder.fromXDR(tx.toXDR(), PASSPHRASE);
  await guardNetwork();
  const sent = await server.sendTransaction(envelope);
  if (sent.hash !== tx.hash().toString("hex") || !["PENDING", "DUPLICATE"].includes(sent.status)) {
    throw new Error(`send-rejected: ${sent.status}; ${sent.hash}`);
  }
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const result = await server.getTransaction(sent.hash);
    if (result.status === "FAILED" || result.status === "SUCCESS") {
      console.info(JSON.stringify({ hash: sent.hash, ledger: result.ledger, status: result.status }));
      return { hash: sent.hash, result };
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw new Error(`inclusion-timeout: ${sent.hash}`);
}

export async function submit(tx: Transaction, source: Keypair, extraSigners: Keypair[] = []): Promise<Receipt> {
  const { hash, result } = await sendAndPoll(tx, source, extraSigners);
  if (result.status !== "SUCCESS") throw new Error(`included-FAILED: ${hash}; ledger ${result.ledger}`);
  const receipt: Receipt = { hash, ledger: result.ledger, status: result.status, value: result.returnValue, events: result.events.contractEventsXdr.flat() };
  receipts.push(receipt);
  return receipt;
}

export async function transaction(source: Keypair, operation: xdr.Operation): Promise<Transaction> {
  await guardNetwork();
  return new TransactionBuilder(await server.getAccount(source.publicKey()), { networkPassphrase: PASSPHRASE, fee: "100" })
    .addOperation(operation).setTimeout(90).build();
}

export async function prepare(source: Keypair, operation: xdr.Operation, signers: Keypair[] = []): Promise<Transaction> {
  const raw = await transaction(source, operation);
  const simulation = await server.simulateTransaction(raw);
  if (!rpc.Api.isSimulationSuccess(simulation) || !simulation.result) {
    throw new Error(`simulation-rejected: ${rpc.Api.isSimulationError(simulation) ? simulation.error : "missing result"}`);
  }
  await guardNetwork();
  const expiration = simulation.latestLedger + 100;
  simulation.result.auth = await Promise.all(simulation.result.auth.map(async entry => {
    if (entry.credentials().switch().name === "sorobanCredentialsSourceAccount") return entry;
    const address = Address.fromScAddress(entry.credentials().address().address()).toString();
    const signer = signers.find(key => key.publicKey() === address);
    if (!signer) throw new Error(`Missing test-only authorization signer for ${address}`);
    return authorizeEntry(entry, signer, expiration, PASSPHRASE);
  }));
  const authorized = rpc.assembleTransaction(raw, simulation).build();
  // Budget the actual signature verification before signing the outer envelope.
  const prepared = await server.prepareTransaction(authorized);
  // Test-only Friendbot funds: allow the constructor's initial 90-day storage rent.
  const maxFeeStroops = 250_000_000n;
  console.info(JSON.stringify({ stage: "prepared", feeStroops: prepared.fee, simulatedResourceFee: simulation.minResourceFee }));
  if (BigInt(prepared.fee) > maxFeeStroops) throw new Error(`Local transaction fee ${prepared.fee} exceeds test budget ${maxFeeStroops}`);
  return prepared;
}

export async function invoke(source: Keypair, operation: xdr.Operation, signers: Keypair[] = []): Promise<Receipt> {
  return submit(await prepare(source, operation, signers), source);
}

export async function rejectSimulation(source: Keypair, operation: xdr.Operation, contractCode: number): Promise<void> {
  const result = await server.simulateTransaction(await transaction(source, operation));
  assert(rpc.Api.isSimulationError(result), "Expected contract rejection during simulation");
  assert.match(result.error, new RegExp(`Error\\(Contract,\\s*#${contractCode}\\)`));
  console.info(JSON.stringify({ stage: "simulation-rejected", contractCode }));
}

// Rebuild only the outer sequence/time bounds; preserve the valid simulation's exact
// resource data, footprint, invocation, auth nonces and fee. Never re-simulate negatives.
export async function freshEnvelope(prepared: Transaction, source: Keypair, mutate?: (auth: xdr.SorobanAuthorizationEntry[]) => Promise<void>): Promise<Transaction> {
  await guardNetwork();
  const operation = prepared.operations[0]!;
  assert.equal(operation.type, "invokeHostFunction");
  if (operation.type !== "invokeHostFunction") throw new Error("Expected Soroban invocation");
  const auth = (operation.auth ?? []).map(entry => xdr.SorobanAuthorizationEntry.fromXDR(entry.toXDR()));
  if (mutate) await mutate(auth);
  const resources = prepared.toEnvelope().v1().tx().ext().sorobanData();
  const inclusionFee = BigInt(prepared.fee) - resources.resourceFee().toBigInt();
  return new TransactionBuilder(await server.getAccount(source.publicKey()), { networkPassphrase: PASSPHRASE, fee: inclusionFee.toString() })
    .addOperation(Operation.invokeHostFunction({ func: operation.func, auth }))
    .setSorobanData(resources)
    .setTimeout(90).build();
}

function diagnosticValues(value: xdr.ScVal): string[] {
  if (value.switch().name === "scvError") {
    const error = value.error();
    return [`${error.switch().name}:${error.switch().name === "sceContract" ? error.contractCode() : error.code().name}`];
  }
  if (value.switch().name === "scvString") return [value.str().toString()];
  if (value.switch().name === "scvVec") return (value.vec() ?? []).flatMap(diagnosticValues);
  return [];
}

export async function submitFailure(prepared: Transaction, source: Keypair, expected: { contractCode?: number; authMessage?: RegExp; attemptedTransfer?: { currency: string; from: string } }) {
  const { hash, result } = await sendAndPoll(prepared, source);
  assert.equal(result.status, "FAILED", "Negative must be included, not a simulation/send rejection");
  if (result.status !== "FAILED") throw new Error("Unexpected successful negative");
  assert.equal(result.resultXdr.result().switch().name, "txFailed");
  const operation = result.resultXdr.result().results()[0]!;
  assert.equal(operation.tr().invokeHostFunctionResult().switch().name, "invokeHostFunctionTrapped");
  const diagnostics = (result.diagnosticEventsXdr ?? []).flatMap(event => {
    const body = event.event().body().v0();
    return [...body.topics(), body.data()].flatMap(diagnosticValues);
  });
  console.info(JSON.stringify({ stage: "included-failure-diagnostics", hash, diagnostics }));
  if (expected.contractCode !== undefined) {
    assert(diagnostics.includes(`sceContract:${expected.contractCode}`), "Missing exact included contract-error diagnostic");
  } else {
    assert(diagnostics.some(value => value.startsWith("sceAuth:")), "Missing included native-auth diagnostic");
    if (expected.authMessage) assert.match(diagnostics.join("\n"), expected.authMessage);
  }
  assert.equal(result.events.contractEventsXdr.flat().length, 0, "Failed transaction must commit no business events");
  if (expected.attemptedTransfer) {
    const { currency, from } = expected.attemptedTransfer;
    assert((result.diagnosticEventsXdr ?? []).some(diagnostic => {
      const event = diagnostic.event();
      const hash = event.contractId();
      const topics = event.body().v0().topics();
      return hash instanceof Uint8Array && Buffer.from(hash).equals(StrKey.decodeContract(currency)) &&
        topics.length >= 2 && scValToNative(topics[0]!) === "transfer" && scValToNative(topics[1]!) === from;
    }), "Missing diagnostic proof of first-Project transfer before late rollback");
  }
  return { hash, ledger: result.ledger, status: result.status };
}
