import { Address, StrKey, xdr } from "@stellar/stellar-sdk";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";

/** Hashes the local Rust sources and manifests used by the three core contracts. */
export async function readCoreBuildInputSha256(repoRoot: string): Promise<string> {
  const paths = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"];
  async function sources(directory: string): Promise<string[]> {
    const entries = await readdir(join(repoRoot, directory), { withFileTypes: true });
    return (await Promise.all(entries.map(entry => {
      const path = `${directory}/${entry.name}`;
      return entry.isDirectory() ? sources(path) : Promise.resolve(entry.isFile() && entry.name.endsWith(".rs") ? [path] : []);
    }))).flat();
  }
  for (const directory of ["contracts/fuul-factory", "contracts/fuul-manager", "contracts/fuul-project", "contracts/fuul-core"]) {
    paths.push(`${directory}/Cargo.toml`);
    paths.push(...await sources(`${directory}/src`));
  }
  const inputs = await Promise.all(paths.sort().map(async path => [path, createHash("sha256").update(await readFile(join(repoRoot, path))).digest("hex")]));
  return createHash("sha256").update(JSON.stringify(inputs)).digest("hex");
}

/** Factory tracker encoding is BE u64 below 2^64, then BE u128 up to uint96. */
export function deriveProjectContractId(factoryContractId: string, trackerBeforeCreate: number | bigint, networkId: string): string {
  if (!StrKey.isValidContract(factoryContractId)) throw new Error("Invalid Factory contract ID");
  if (typeof trackerBeforeCreate === "number" && (!Number.isSafeInteger(trackerBeforeCreate) || trackerBeforeCreate < 0)) {
    throw new Error("Factory tracker must be a nonnegative safe integer or bigint");
  }
  const value = BigInt(trackerBeforeCreate);
  if (value < 0n || value >= (1n << 96n)) throw new Error("Factory tracker exceeds uint96");
  if (!/^[0-9a-f]{64}$/.test(networkId)) throw new Error("Invalid network ID");
  const tracker = Buffer.alloc(value < (1n << 64n) ? 8 : 16);
  tracker.writeBigUInt64BE(value & ((1n << 64n) - 1n), tracker.length - 8);
  if (tracker.length === 16) tracker.writeBigUInt64BE(value >> 64n, 0);
  const salt = createHash("sha256").update(tracker).digest();
  const preimage = xdr.HashIdPreimage.envelopeTypeContractId(new xdr.HashIdPreimageContractId({
    networkId: Buffer.from(networkId, "hex"),
    contractIdPreimage: xdr.ContractIdPreimage.contractIdPreimageFromAddress(new xdr.ContractIdPreimageFromAddress({
      address: new Address(factoryContractId).toScAddress(), salt,
    })),
  }));
  return StrKey.encodeContract(createHash("sha256").update(preimage.toXDR()).digest());
}
