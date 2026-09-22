import { contract,scValToNative,StrKey } from "@stellar/stellar-sdk";
import { expect,test } from "bun:test";
import { assertLocalUrl,BASE } from "./localnet.js";

test("local endpoint guard rejects public hosts, credentials and alternate ports", () => {
  expect(() => assertLocalUrl(BASE)).not.toThrow();
  for (const value of ["https://soroban-testnet.stellar.org", "https://soroban.stellar.org", "http://127.0.0.1:1", `${BASE.replace("http://", "http://user:secret@")}`, "http://127.0.0.1.example.com:18010", `${BASE}/#redirect`]) {
    expect(() => assertLocalUrl(value)).toThrow();
  }
});

test("foundation claim encoding matches current Wasm ABI without generating keys", async () => {
  const account = StrKey.encodeEd25519PublicKey(Buffer.alloc(32));
  const id = StrKey.encodeContract(Buffer.alloc(32));
  const spec = contract.Spec.fromWasm(Buffer.from(await Bun.file(new URL("../../../target/wasm32v1-none/release/fuul_manager.wasm", import.meta.url)).bytes()));
  const cooldown = spec.funcArgsToScVals("set_claim_cooldown", { caller: account, period: (1n << 128n) - 1n })[1]!;
  expect(cooldown.switch().name).toBe("scvU128");
  expect(scValToNative(cooldown)).toBe((1n << 128n) - 1n);
  const encoded = spec.funcArgsToScVals("claim", { caller: account, checks: [{
    project_address: id, to: account, currency: id, currency_type: { tag: "StellarAsset", values: undefined },
    amount: 100_000n, reason: { tag: "EndUserPayout", values: undefined }, token_id: (1n << 256n) - 1n,
    deadline: (1n << 256n) - 1n, proof: Buffer.alloc(32), signers: [account],
  }] });
  expect(encoded).toHaveLength(2);
  expect(encoded[1]!.vec()).toHaveLength(1);
});
