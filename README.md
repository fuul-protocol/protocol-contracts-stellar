# Fuul Stellar contracts

Soroban contracts for Fuul rewards. The [TypeScript SDK](https://github.com/eloizxyz/protocol-sdk-stellar) is a separate repository.

- `contracts/fuul-manager`: claim authorization, currency limits, pause controls and native fees.
- `contracts/fuul-factory`: Project deployment, protocol roles and fee configuration.
- `contracts/fuul-project`: Project funds, claims, proof replay protection and withdrawals.
- `contracts/fuul-core`: shared interfaces, types, fees, access control and storage renewal.
- `tests`: unit, integration, property, compiled-contract regression, and network E2E tests, with test-only fixtures.

## Build and test

Use Rust **1.92.0** (pinned in `rust-toolchain.toml`) and Stellar CLI **27.1.0**.

```sh
rustup target add wasm32v1-none
stellar contract build --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

Build before testing: several tests load compiled Wasm files from `target/wasm32v1-none/release/`.
Only `fuul_manager.wasm`, `fuul_factory.wasm`, and `fuul_project.wasm` are production artifacts.
All other workspace contracts are test fixtures. Rebuild after source changes.
Use the recorded release platform when comparing byte hashes.

## Testing

See the [test guide](tests/README.md) for prerequisites, test types and commands to run individual suites.

Tests cover exact authorization, quorum, roles, fee arithmetic, multi-currency accounting, KYC, asset interfaces, rollback, replay, upgrades and storage lifetime.
`tests/fuzz` contains reproducible property tests that run with `cargo test`; increase cases with `PROPTEST_CASES` when required.
Unit tests use native contracts, while integration and regression tests also exercise compiled Wasm.

## Deploy and integrate

For [automatic Mainnet deployment](deploy.md#automatic-mainnet-deployment), prepare the identities offline first:

```sh
bash scripts/deploy.sh --prepare-keys
```

The client must fund the displayed Mainnet addresses before running the deployment command in [deploy.md](deploy.md). Afterward, `bash scripts/status.sh` shows the local record and `bash scripts/status.sh --verify` compares it with the configured Mainnet RPC. The guide also retains manual Testnet instructions and SDK examples.
A Factory can create Projects that pay different assets; currency limits belong to Manager and apply across Projects.
Project currency restrictions are an application policy, not a contract-level allowlist.

Manager and Factory upgrades require their current administrator. Project upgrades require a Factory administrator.
`keep_alive` renews code and instance storage; persistent proofs, roles and accounting entries have separate lifetimes.
Restore archived entries before use and retain transaction receipts.

Independent review and Fuul's production configuration approval remain required before Mainnet deployment.
