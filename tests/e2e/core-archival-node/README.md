# Isolated Core archival fixture

This test node permits a maximum entry lifetime of 128 ledgers.
It lets the unchanged Fuul Wasm reach physical archival within a short local test.
The single Core patch changes the validation floor for this network setting.
It changes neither the Soroban host sources nor the contract Wasm.

Use this image only with `node tests/e2e/run.mjs archive`.
It is not a node configuration for public Testnet or Mainnet.
The delivery package records the tested image and binary.

## Build

The source is Stellar Core `v28.0.1`, commit `947aad8413c189d85504acf72207e85eeda9b021`.
`core-submodules.txt` records its pinned dependency revisions.
`build-provenance.json` specifies the build. The delivery package contains its execution record.

From the port repository root, create a fresh local build directory:

```sh
mkdir -p .local/core-archival-build
cp tests/e2e/core-archival-node/Dockerfile tests/e2e/core-archival-node/.dockerignore tests/e2e/core-archival-node/test-only-ttl.patch tests/e2e/core-archival-node/build-provenance.json .local/core-archival-build/
git clone --no-checkout https://github.com/stellar/stellar-core.git .local/core-archival-build/stellar-core
git -C .local/core-archival-build/stellar-core checkout --detach 947aad8413c189d85504acf72207e85eeda9b021
git -C .local/core-archival-build/stellar-core submodule update --init --recursive
git -C .local/core-archival-build/stellar-core apply --check ../test-only-ttl.patch
git -C .local/core-archival-build/stellar-core apply ../test-only-ttl.patch
docker build --progress plain --tag fuul-core-archival:20260919-v28.0.1 .local/core-archival-build
```

The build needs approximately 4 GiB of Docker memory and can take more than 30 minutes.
Rust LTO and debug information are disabled to reduce memory use.
The build preserves each protocol's dependency graph. Its cache names belong to this fixture.

The runner resolves the local tag to an immutable image ID and verifies its source-revision label.
It requires a local Unix Docker socket, uses a unique loopback port and stack name, and removes its own ledger volume afterward.
It never pulls or publishes the custom image.

```sh
node tests/e2e/run.mjs archive
```

The standard CI suite uses the official Quickstart image. This custom-node test is a separate qualification command.
