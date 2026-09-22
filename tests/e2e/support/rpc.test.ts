import { Account, Asset, Keypair, Operation, TransactionBuilder, xdr } from "@stellar/stellar-sdk";
import { expect, test } from "bun:test";
import { captiveCoreRead, LocalRpcServer } from "./rpc.js";

const transient = { code: -32603, message: "could not query captive core: http request failed with non-200 status code (404)" };
test("local read retries are limited to two exact captive Core failures", async () => {
  let calls = 0; const delays: number[] = [];
  expect(await captiveCoreRead(async () => { if (++calls < 3) throw transient; return "ready"; }, async ms => { delays.push(ms); })).toBe("ready");
  expect(calls).toBe(3); expect(delays).toEqual([250, 500]);
  calls = 0;
  await expect(captiveCoreRead(async () => { calls++; throw transient; }, async () => {})).rejects.toBe(transient);
  expect(calls).toBe(3);
  for (const error of [{ ...transient, code: -1 }, { ...transient, message: "missing account" }, new Error("404"), null]) {
    calls = 0;
    await expect(captiveCoreRead(async () => { calls++; throw error; }, async () => { throw new Error("Unexpected retry"); })).rejects.toBe(error);
    expect(calls).toBe(1);
  }
  const contractError = { error: "Error(Contract, #1)" };
  expect(await captiveCoreRead(async () => contractError)).toBe(contractError);
});

test.serial("local transport retries reads before account errors are masked, and never retries sends", async () => {
  const original = globalThis.fetch; const methods: string[] = [];
  const server = new LocalRpcServer("http://127.0.0.1:18010/soroban/rpc");
  expect(server.httpClient.defaults.timeout).toBe(15_000); expect(server.httpClient.defaults.maxRedirects).toBe(0);
  try {
    globalThis.fetch = Object.assign(async (_input: Parameters<typeof fetch>[0], init: Parameters<typeof fetch>[1]) => {
      const request = JSON.parse(String(init?.body)); methods.push(request.method);
      return Response.json({ jsonrpc: "2.0", id: request.id,
        ...(methods.length === 1 ? { error: transient } : { result: { entries: [], latestLedger: 100 } }),
      });
    }, { preconnect: original.preconnect });
    const key = Keypair.random();
    await expect(server.getAccount(key.publicKey())).rejects.toThrow("Account not found");
    expect(methods).toEqual(["getLedgerEntries", "getLedgerEntries"]);
    methods.length = 0;
    globalThis.fetch = Object.assign(async (_input: Parameters<typeof fetch>[0], init: Parameters<typeof fetch>[1]) => {
      const request = JSON.parse(String(init?.body)); methods.push(request.method);
      return Response.json({ jsonrpc: "2.0", id: request.id, error: transient });
    }, { preconnect: original.preconnect });
    const tx = new TransactionBuilder(new Account(key.publicKey(), "1"), { networkPassphrase: "Standalone Network ; February 2017", fee: "100" })
      .addOperation(Operation.payment({ destination: key.publicKey(), asset: Asset.native(), amount: "1" })).setTimeout(60).build(); tx.sign(key);
    await expect(server.sendTransaction(tx)).rejects.toMatchObject(transient);
    expect(methods).toEqual(["sendTransaction"]);
    methods.length = 0;
    await expect(server.getLedgerEntry(xdr.LedgerKey.account(new xdr.LedgerKeyAccount({ accountId: key.xdrAccountId() })))).rejects.toMatchObject(transient);
    expect(methods).toEqual(["getLedgerEntries", "getLedgerEntries", "getLedgerEntries"]);
  } finally { globalThis.fetch = original; }
});
