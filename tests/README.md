# Testing

Run all commands from the **port repository root**.

## Prerequisites

Use Rust **1.92.0** and Stellar CLI **27.1.0**. E2E tests also require Bun **1.3.12**, Node.js **22**, and Docker with Compose. `rust-toolchain.toml` declares the Rust version, components, and `wasm32v1-none` target.

See [Stellar's development setup](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup) for installation instructions. Use the versions listed above.

Fetch dependencies before running the offline test commands:

```sh
cargo fetch --locked
```

E2E dependencies stay in `tests/e2e/`. They use the Stellar JavaScript SDK and do not require the Fuul SDK repository.

```sh
bun install --cwd tests/e2e --frozen-lockfile
bun run --cwd tests/e2e typecheck
```

## Test types

| Type | Location | Purpose |
| --- | --- | --- |
| Unit | `tests/unit/` | Check individual contract rules and shared-core behavior |
| Integration | `tests/integration/` | Test contract interactions and cross-contract protocol flows |
| Property (fuzz) | `tests/fuzz/` | Check invariants over generated inputs with Proptest |
| Regression | `tests/regression/` | Check authorization, settlement, assets and lifecycle against compiled contracts |
| End-to-end | `tests/e2e/` | Exercise contract flows through a local Stellar RPC node |

Unit, integration, property and regression tests run locally in the Soroban test host. E2E tests use a local Stellar network in Docker. Test-only contracts are in `tests/fixtures/` and `tests/upgrade/`.

## Quick start

```sh
stellar contract build --locked
cargo test --offline --locked --workspace
```

Build first. Several tests load the compiled WASM from `target/wasm32v1-none/release/`. The workspace build includes the required test fixtures.

To run one contract's complete suite:

```sh
cargo test --offline --locked -p fuul-factory --lib
cargo test --offline --locked -p fuul-project --lib
cargo test --offline --locked -p fuul-manager --lib
```

Each suite includes unit, integration and property tests. Manager also includes protocol and compiled-contract regression tests.

## Unit tests

Tests in `tests/unit/` check individual contract rules: configuration, roles, fees, claims, funds, and authorization. To run only unit tests, exclude the other modules:

```sh
cargo test --offline --locked -p fuul-factory --lib -- \
  --skip test::integration:: \
  --skip test::properties:: \
  --skip test::protocol:: \
  --skip regression::
```

Replace `fuul-factory` with `fuul-project` or `fuul-manager` for those contracts. For shared-core unit tests:

```sh
cargo test --offline --locked -p fuul-core --lib
```

## Integration tests

Tests in `tests/integration/` exercise contract interactions and guest WASM in the native Soroban test host, without a live RPC node. Cross-contract protocol flows live in `tests/integration/protocol/`.

```sh
cargo test --offline --locked -p fuul-factory --lib test::integration::
cargo test --offline --locked -p fuul-project --lib test::integration::
cargo test --offline --locked -p fuul-manager --lib test::integration::
cargo test --offline --locked -p fuul-manager --lib test::protocol::
```

The protocol module is separate from `test::integration::`.

## Regression tests

Tests in `tests/regression/` exercise compiled contracts, including authorization, multi-currency settlement, asset transfers and upgrades:

```sh
cargo test --offline --locked -p fuul-manager --lib regression::
```

## Property tests (fuzz)

Tests in `tests/fuzz/` check invariants over generated inputs using **Proptest**:

```sh
cargo test --offline --locked -p fuul-factory --lib test::properties::
cargo test --offline --locked -p fuul-project --lib test::properties::
cargo test --offline --locked -p fuul-manager --lib test::properties::
```

Set the case count and seed for a repeatable run:

```sh
PROPTEST_CASES=128 PROPTEST_RNG_SEED=42 \
  cargo test --offline --locked -p fuul-factory --lib test::properties::
```

Property tests disable file-based failure persistence. Deterministic regression cases live in the test source. Soroban may generate ignored `contracts/*/test_snapshots/` output during native runs.

## End-to-end tests

Tests in `tests/e2e/` deploy contracts and submit transactions through a local Stellar RPC node.

### Docker setup

1. [Install Docker with Compose](https://docs.docker.com/get-started/get-docker) and start the local daemon.
2. Check that Docker and Compose are available:

   ```sh
   docker info
   docker compose version
   ```

3. Install the locked E2E dependencies if you have not already:

   ```sh
   bun install --cwd tests/e2e --frozen-lockfile
   ```

4. Run the core suite:

   ```sh
   node tests/e2e/run.mjs
   ```

The runner builds the contracts and fixtures, starts the pinned Quickstart image on Protocol **28**, and runs the tests. It chooses a free local port and removes its own Docker stack and ledger volume on exit. No separate container startup is needed. Logs and results stay in ignored `.local/e2e/` directories.

For Docker and local network background, see [Stellar Quickstart](https://developers.stellar.org/docs/tools/quickstart) and its [getting-started guide](https://developers.stellar.org/docs/tools/quickstart/getting-started).

### Suites and protocols

| Command | Scope |
| --- | --- |
| `node tests/e2e/run.mjs` | Factory, Project, Manager, upgrades and RPC helpers |
| `node tests/e2e/run.mjs resource` | Claim workloads and network resource limits |
| `node tests/e2e/run.mjs archive` | Proof archival, restoration and replay rejection |

The core suite does not include resource or archival tests. Core and resource default to Protocol **28**. To run either suite on Protocol **27**:

```sh
FUUL_E2E_PROTOCOL=27 node tests/e2e/run.mjs
FUUL_E2E_PROTOCOL=27 node tests/e2e/run.mjs resource
```

Resource tests measure signed claim batches, exact settlement, multiple Projects, recipients, currencies, KYC, and signer counts.
The results describe the tested workloads and local network limits, not a universal batch size. Reports stay in `.local/resource/`.

Archival requires the [isolated archival node](e2e/core-archival-node/README.md) to be built locally first. It uses Protocol **28** and verifies paid-proof eviction, restoration, replay rejection and a subsequent valid claim. Results stay in `.local/core-archival/`.

## Individual tests

After building the workspace, list Rust tests and select one by its full name:

```sh
cargo test --offline --locked -p fuul-factory --lib -- --list
cargo test --offline --locked -p fuul-factory --lib FULL_TEST_NAME -- --exact --nocapture
```

Replace `FULL_TEST_NAME` with a full name from the listing. `--exact` selects that name and `--nocapture` displays test output. Change the package name to select another contract.

Rust test groups are `--lib` modules, **not** independent `cargo test --test` file targets. Arguments after `--` go to Rust's test harness.
