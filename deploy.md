# Deploy Fuul on Stellar

This guide uses Stellar CLI **27.1.0**, Rust **1.92.0**, and Bash or Zsh.
The automatic workflow below is for **Mainnet**. The numbered manual steps afterward also cover Testnet and custom configurations. Run all commands from this repository's root.

> **Optional documentation help:** If the client uses an AI assistant, [Stellar Raven MCP](https://developers.stellar.org/docs/build/building-with-ai#raven-mcp-server) can search Stellar documentation while they follow this guide. Connect it in their own AI client using Stellar's instructions and complete the sign-in there. Raven is not required to run the scripts or deploy contracts.

## Mainnet at a glance

| Step | What the client does | Result |
| --- | --- | --- |
| Prepare | Install the required tools and choose the RPC, asset, limit and Project URI | Ready to run the scripts |
| 1. Keys | Run `bash scripts/deploy.sh --prepare-keys` | Shows seven public addresses and saves private keys under `.keys/mainnet/` |
| 2. XLM | Create and fund those seven accounts through the client's custody or provider | Accounts exist on Mainnet and `ADMIN` has XLM for fees |
| 3. Deploy | Run `deploy.sh` with the RPC, asset, limit and URI | Manager, Factory and Project deployed with IDs and hashes saved |
| 4. Check | Run `bash scripts/status.sh`, then add `--verify` | Inspect the local record and compare it with Mainnet |
| 5. Failure | Read the last step and private diagnostic log | Retry before any upload or reconcile a partially submitted deployment |

## Automatic Mainnet deployment

`scripts/deploy.sh` handles key preparation and deployment in two runs. Mainnet does not have Friendbot. Without an already funded account that the script is allowed to spend from, the client must fund the new accounts between runs.

### Before you start

- Work from the repository root on the machine where the client will keep `.keys/`. The scripts do not connect to or copy files onto that machine for you.
- Install Git, Bash **3.2+**, Stellar CLI **27.1.0**, Rust/Cargo **1.92.0**, and `wasm32v1-none` (`rustup target add wasm32v1-none`). See [Stellar's setup guide](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup) if needed. `bash scripts/deploy.sh --help` does not create keys or contact Mainnet.
- Decide who owns and backs up the keys, and obtain XLM to create and fund the new Mainnet accounts. Prepare an approved [Mainnet RPC](https://developers.stellar.org/docs/data/apis/rpc/providers) URL, an accepted asset contract ID (`C...`), its positive limit in integer base units, and a Project URI.
- The automatic profile uses **one claim signer, quorum 1, no KYC** and the contracts' default fees. Use the manual constructor instructions below if that is not the intended configuration.

### 1. Prepare identities offline

```sh
bash scripts/deploy.sh --prepare-keys
```

This creates seven distinct Mainnet identities by default. Use `--key-count 1–20` if needed. It prints each alias, **public** address and assigned role, plus the storage locations. It never prints private keys, funds accounts or contacts the network. Repeating this step loads the same identities instead of replacing them.

The private keys are stored under `.keys/mainnet/stellar/` and in `.keys/mainnet/keys.env`. Only the client should inspect or back up these files. If they need to view one secret locally, `stellar keys secret fuul-1 --config-dir .keys/mainnet/stellar` prints that identity's private key. It must not be shared in deployment logs or screenshots.

### 2. Fund the accounts

Have the client create and fund the displayed `G...` accounts with XLM through their approved exchange or custody process. Stellar requires a [minimum account balance](https://developers.stellar.org/docs/learn/fundamentals/lumens#minimum-balance), and `ADMIN` also needs XLM for transaction fees. The deployer checks that all accounts exist, but an account lookup cannot prove that the payer has enough XLM for the complete deployment. [Stellar's account creation guide](https://developers.stellar.org/docs/build/guides/transactions/create-account) explains why generating keys alone does not create an account on Mainnet.

This funding step requires XLM from an existing account or provider controlled by the client. It cannot be automated here without an authorized funded source. The automatic script never spends from an unspecified account.

### 3. Deploy and verify

Choose the approved Mainnet RPC, accepted asset contract, currency limit in base units and Project URI. Then run the **same script**:

```sh
bash scripts/deploy.sh --project-uri 'ipfs://your-project' \
  --rpc-url "$RPC_URL" --currency "$CURRENCY" \
  --currency-limit "$INITIAL_CURRENCY_LIMIT"
```

To deploy Manager and Factory **without creating a Project**, replace `--project-uri` with `--skip-project`:

```sh
bash scripts/deploy.sh --skip-project \
  --rpc-url "$RPC_URL" --currency "$CURRENCY" \
  --currency-limit "$INITIAL_CURRENCY_LIMIT"
```

The Project WASM is still uploaded because the Factory stores its hash. The `PROJECT_ADMIN` account does not need to exist unless another role shares its key. Verification reads `Factory.project_wasm_hash` instead of a Project, and the record keeps `PROJECT` empty. Create Projects later with the command in [step 5](#5-create-a-project-and-verify-it).

This command first checks that **all generated accounts exist**, then builds and tests the workspace. Only after those checks does it reserve a deployment record and send real Mainnet transactions: upload the three production WASM files, deploy Manager and Factory, and create one Project. It verifies their code hashes and relationships. The final summary shows contract IDs, hashes, the record and a private diagnostic log. It does not fund the Project or run the SDK demo. `NO_COLOR=1` disables progress colors. An external custody or hardware-signing process would need a different integration.

| Saved item | Location | Use later |
| --- | --- | --- |
| Stellar CLI identities | `.keys/mainnet/stellar/` | Refer to `fuul-N` through `--config-dir .keys/mainnet/stellar` |
| Private exports and role mappings | `.keys/mainnet/keys.env` | Reload with `source scripts/keys.sh mainnet` |
| Deployment progress and public IDs | `.keys/mainnet/deployment.env` | Source after completion or inspect a stopped run before retrying |
| Public per-contract records | `deployments/mainnet/FuulManager.json`, `FuulFactory.json` and `FuulProject.json` (when created) | Commit them, like the EVM repository's `deployments/` |
| Private command diagnostics | `.keys/deploy-mainnet.log.<random>` | Diagnose a stopped run without sharing the unredacted file |

### 4. Check the result

Run these commands from the same repository checkout where the Mainnet deployment record and Stellar CLI configuration were saved:

```sh
# Local record only. No RPC call and no private keys loaded.
bash scripts/status.sh

# Compare the saved IDs and hashes with the configured Mainnet RPC.
bash scripts/status.sh --verify
```

The local view shows the last completed step, role aliases and public addresses, currency, contract IDs and code hashes. A partial deployment stays marked `in_progress`. `--verify` only works after completion. It simulates read-only calls to check the Project's Factory, Manager quorum, Factory's Manager role and all three deployed code hashes. For a `--skip-project` record, `PROJECT` shows `(not created)` and `--verify` checks the Factory's stored Project WASM hash plus the Manager and Factory code hashes. A failed check exits with an error. Neither command sends transactions. The local view does not load keys. Verification invokes Stellar CLI with its configured identity alias.

If you run from another machine, the local record and CLI network configuration must be available there. This script does not copy private files or establish remote access.

**Deployment records:** after verification, the script writes one JSON file per contract under `deployments/mainnet/` with the address, deployer, network passphrase, WASM hash, constructor parameters, the upload and deploy transaction hashes, ledger, charged fee in stroops and a UTC timestamp. Transaction hashes come from the CLI's signing output. An upload of code that already exists on the network sends no transaction and records `null`. Ledger and fee come from `stellar tx fetch fee` and are `null` if that lookup fails. A new deployment refuses to start while any of these files exist; move the previous records first. To inspect a recorded transaction, Stellar CLI 27.1.0 can fetch its result and events:

```sh
stellar tx fetch result --hash "$TX_HASH" --network fuul \
  --config-dir .keys/mainnet/stellar --output json-formatted
stellar tx fetch events --hash "$TX_HASH" --network fuul \
  --config-dir .keys/mainnet/stellar --output json-formatted
```

These commands need the configured network from step 2 and an actual `TX_HASH`. Save the result in the client's private deployment records if transaction-level evidence is required. The automatic deployment does not invent or capture hashes that its CLI commands do not expose.

### 5. If deployment stops

Missing accounts, build errors or failed tests happen **before** the first upload and leave no `deployment.env`. Fix the prerequisite and rerun the same deployment command. Once uploads begin, the script reserves `.keys/mainnet/deployment.env`, writes progress before each transaction and saves known public IDs/hashes after successful responses. Directories are private (`700`) and the record is `600`. Only a fully verified deployment has `DEPLOYMENT_STATUS=complete`.

An existing record, including a manual or partial record, is refused before loading keys or changing network configuration. After acquiring the lock and before loading keys, the script creates one private `600` log at `.keys/deploy-mainnet.log.<random>`. It retains CLI stderr and key-helper/build/test diagnostics, including on failure. The console shows the last step, record path and private log path without exposing diagnostic content. Preflight diagnostics before the log exists remain suppressed. Logs may contain sensitive provider errors or RPC credentials. Inspect them privately and do not share them unredacted.

A transaction can succeed even when the client receives an error: **do not delete the record and blindly rerun**. Use the private log and saved IDs to reconcile the last operation and finish manually. There is no automatic retry or resume. An interrupted process may leave `.keys/.deploy-mainnet.lock`. Remove it only after confirming no deployment is running and reconciling the attempt.

After completion, load values into your own shell for later SDK use:

```sh
source scripts/keys.sh mainnet
source .keys/mainnet/deployment.env
```

The summary and record contain public deployment values, not secrets. Keep the surrounding `.keys` directory private as described in step 2. The JSON files under `deployments/mainnet/` hold transaction hashes and constructor arguments, but not receipts or events. The on-chain check above confirms current contract state, not every transaction's history.

Offline regression tests use isolated temporary Git repositories and fake Stellar/Cargo/Rust commands. No network or real credentials are used:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_mainnet_deploy.py' -v
```

References: [Stellar deployment tutorial](https://developers.stellar.org/docs/build/smart-contracts/getting-started/deploy-to-testnet) and [CLI reference](https://developers.stellar.org/docs/tools/cli/stellar-cli). The helper targets CLI 27.1.0: Project creation uses `--send=yes`, readbacks use `--send=no`, and uploads use `--optimize=false` to preserve the compared local bytes.

## 1. Build and test

```sh
rustup target add wasm32v1-none
stellar contract build --locked
cargo test --workspace --locked
```

The build includes test fixtures. Deploy only `fuul_manager.wasm`, `fuul_factory.wasm`, and `fuul_project.wasm` from `target/wasm32v1-none/release/`.
Record their SHA-256 values:

```sh
export WASM=target/wasm32v1-none/release
shasum -a 256 "$WASM/fuul_manager.wasm" \
  "$WASM/fuul_factory.wasm" "$WASM/fuul_project.wasm"
```

Reproduce release hashes on the recorded platform. A build on another platform requires a new artifact comparison and Testnet check.

## 2. Create identities and select the network

```sh
source scripts/keys.sh
```

The helper asks for **testnet or mainnet** and **1–20 addresses**; the default is seven.
It exports `FUUL_NETWORK`, `FUUL_KEYS_CONFIG`, `FUUL_KEY_COUNT`, and each `FUUL_KEY_N_PUBLIC` / `FUUL_KEY_N_SECRET`.
It assigns the first seven keys to `ADMIN`, `PAUSER`, `UNPAUSER`, `SIGNER`, `FACTORY_ADMIN`, `COLLECTOR`, and `PROJECT_ADMIN`.
Each role also has a `_KEY` CLI alias and a `_SECRET` value. With fewer than seven keys, roles share identities.

This follows Fuul’s [create-keys workflow](https://github.com/fuul-protocol/protocol-contracts-v2/tree/main/scripts/create-keys): generate local credentials, retain them, and display only public addresses.
The helper uses the Stellar CLI to create independent Ed25519 keypairs. Ethereum keys and derivation paths do not apply.
Stellar CLI identities and `keys.env` keep the private keys locally and make them available to subsequent deployment commands.

Keys stay in `.keys/<network>/`, with directory permissions `700` and file permissions `600`.
The helper adds a local `.git/info/exclude` rule; it does not modify `.gitignore`.
Do not share `.keys`, upload it, force-add it to Git, or display secrets while recording a video.
Run the helper again to reload existing identities without replacing them.
A child process cannot export variables into its parent, so use **`source`**, not `bash scripts/keys.sh`.

Configure the network:

```sh
if [ "$FUUL_NETWORK" = testnet ]; then
  export RPC_URL=https://soroban-testnet.stellar.org
  export NETWORK_PASSPHRASE='Test SDF Network ; September 2015'
else
  : "${RPC_URL:?Set RPC_URL to your Mainnet Soroban RPC endpoint}"
  export NETWORK_PASSPHRASE='Public Global Stellar Network ; September 2015'
fi
stellar network add fuul --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --config-dir "$FUUL_KEYS_CONFIG"
```

On **Testnet**, fund the generated accounts through Friendbot:

```sh
if [ "$FUUL_NETWORK" = testnet ]; then
  for i in $(seq 1 "$FUUL_KEY_COUNT"); do
    stellar keys fund "fuul-$i" --network fuul \
      --config-dir "$FUUL_KEYS_CONFIG"
  done
fi
```

On **Mainnet**, create and fund the accounts through your approved custody process. Friendbot is unavailable.
Approve role ownership, signer quorum, fees, currency limits and KYC before deployment.
The file-based keys are a bootstrap option; use Fuul's approved custody arrangement for production authority.
Complete the independent review before committing production funds. Mainnet transactions spend real XLM.

## 3. Select the initial currency

Manager starts with native XLM and one additional asset. They must have different contract addresses.

```sh
export NATIVE=$(stellar contract id asset --asset native \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG")
```

For **Testnet**, create a disposable asset contract. It is used as the initial additional currency; the SDK demo below pays native XLM.

```sh
if [ "$FUUL_NETWORK" = testnet ]; then
  export CURRENCY=$(stellar contract asset deploy --asset "DEMO:$ADMIN" \
    --source "$ADMIN_KEY" --network fuul --config-dir "$FUUL_KEYS_CONFIG")
fi
```

For **Mainnet**, set `CURRENCY` to the verified contract address of the approved asset.
Set the initial limit in its integer base units, and confirm its decimals and required trustlines.
The example below is 100 tokens only for an asset with seven decimals.

```sh
: "${CURRENCY:?Set the accepted asset contract address}"
export INITIAL_CURRENCY_LIMIT=1000000000
```

## 4. Upload and deploy Manager and Factory

Upload the three release artifacts, then compare each returned hash with step 1:

```sh
export PROJECT_HASH=$(stellar contract upload \
  --wasm "$WASM/fuul_project.wasm" --source "$ADMIN_KEY" \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG" --instruction-leeway 3000000)
export MANAGER_HASH=$(stellar contract upload \
  --wasm "$WASM/fuul_manager.wasm" --source "$ADMIN_KEY" \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG" --instruction-leeway 3000000)
export FACTORY_HASH=$(stellar contract upload \
  --wasm "$WASM/fuul_factory.wasm" --source "$ADMIN_KEY" \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG" --instruction-leeway 3000000)

export MANAGER=$(stellar contract deploy --wasm-hash "$MANAGER_HASH" \
  --source "$ADMIN_KEY" --network fuul --config-dir "$FUUL_KEYS_CONFIG" -- \
  --admin "$ADMIN" --pauser "$PAUSER" --unpauser "$UNPAUSER" \
  --initial_required_signers 1 --claim_signers "[\"$SIGNER\"]" \
  --accepted_currency "$CURRENCY" --native_asset "$NATIVE" \
  --initial_kyc_validator null --initial_currency_limit "$INITIAL_CURRENCY_LIMIT")

export FACTORY=$(stellar contract deploy --wasm-hash "$FACTORY_HASH" \
  --source "$ADMIN_KEY" --network fuul --config-dir "$FUUL_KEYS_CONFIG" -- \
  --admin "$FACTORY_ADMIN" --manager "$MANAGER" \
  --fee_collector "$COLLECTOR" --project_wasm_hash "$PROJECT_HASH")
```

This example uses one claim signer and no KYC provider. Change those constructor inputs to the approved production configuration.
Factory administrators control Project upgrades. Project administrators control Project settings and withdrawals.
CLI `--inclusion-fee` is not an all-in resource-fee ceiling; review the transaction cost before production signing.

## 5. Create a Project and verify it

```sh
export PROJECT=$(stellar contract invoke --id "$FACTORY" \
  --source "$ADMIN_KEY" --network fuul --config-dir "$FUUL_KEYS_CONFIG" -- \
  create_fuul_project --project_admin "$PROJECT_ADMIN" \
  --project_info_uri 'ipfs://your-project' --kyc_required false | tr -d '"')

stellar contract invoke --id "$MANAGER" --source "$ADMIN_KEY" \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG" --send=no -- required_signers
stellar contract invoke --id "$FACTORY" --source "$ADMIN_KEY" \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG" --send=no -- contract_tracker
stellar contract invoke --id "$PROJECT" --source "$ADMIN_KEY" \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG" --send=no -- factory
```

The last result must match `FACTORY`. Verify code hashes, roles, fees and currency limits before funding.
Save the public deployment values so a new shell can restore them after `source scripts/keys.sh`:

```sh
printf 'export MANAGER=%s\nexport FACTORY=%s\nexport PROJECT=%s\nexport NATIVE=%s\nexport CURRENCY=%s\nexport MANAGER_HASH=%s\nexport FACTORY_HASH=%s\nexport PROJECT_HASH=%s\n' \
  "$MANAGER" "$FACTORY" "$PROJECT" "$NATIVE" "$CURRENCY" \
  "$MANAGER_HASH" "$FACTORY_HASH" "$PROJECT_HASH" \
  > ".keys/$FUUL_NETWORK/deployment.env"
```

Keep transaction hashes and the Factory's `ProjectCreated` event with the deployment record.
Reads use simulation and do not count as submitted transactions.

## 6. Exercise the SDK and record a Testnet demo

Clone the [SDK repository](https://github.com/eloizxyz/protocol-sdk-stellar) beside this repository.
Keep the same shell so the exported identities and addresses remain available.

```sh
cd ../protocol-sdk-stellar
bun install --frozen-lockfile
bun run build
export CURRENCY="$NATIVE"
node examples/demo.mjs status
node examples/demo.mjs fund 100000000 --submit
export PROOF=$(node -e 'process.stdout.write(require("node:crypto").randomBytes(32).toString("hex"))')
node examples/demo.mjs claim 10000000 "$PROOF" --submit
node examples/demo.mjs status
```

This funds the Project with 10 XLM and claims 1 XLM for `PROJECT_ADMIN`, plus the configured fees.
Retain `PROOF` with the payment record. Repeating the claim with the same proof must fail with error **6102**, without a second payment:

```sh
node examples/demo.mjs claim 10000000 "$PROOF" --submit
node examples/demo.mjs pause --submit
node examples/demo.mjs status
node examples/demo.mjs unpause --submit
```

For the video, show the network, contract addresses, initial balance, confirmed funding and claim hashes, updated balance, replay rejection and pause state.
Do not display environment dumps or key files. The example prints public results only and checks deployed code before use.
It stores a transaction hash before signing in `.local/demo/`; after an uncertain outcome, run `node examples/demo.mjs reconcile` instead of repeating the write.
For pause/unpause recovery, use `reconcile PAUSER` or `reconcile UNPAUSER`.
The example needs quorum one; production applications must provide the full approved signer set.

On **Mainnet**, `status` is read-only. Every example write requires both `--submit --mainnet`, the approved `CURRENCY`, and a reviewed base-unit amount.
Do not run the Testnet demonstration amounts with production funds.

## Multi-currency behavior

One Factory can create Projects funded in different assets. A single Project can also hold and pay multiple assets.
There is **no per-Project currency allowlist** in the contracts. Enforce that business rule in the claim-signing backend if required.
The Manager authorizes currencies through a nonzero limit; its per-currency cooldown limit is shared across all its Projects.
Native claim fees remain in XLM, regardless of the reward currency.

To enable another verified asset, return to the contracts repository and set `SECOND_CURRENCY` and `SECOND_LIMIT`:

```sh
stellar contract invoke --id "$MANAGER" --source "$ADMIN_KEY" \
  --network fuul --config-dir "$FUUL_KEYS_CONFIG" -- \
  add_currency_limit --caller "$ADMIN" --token "$SECOND_CURRENCY" --limit "$SECOND_LIMIT"
```

Fund the relevant Project in that asset, prepare recipient and collector trustlines where needed, and pass the asset as `currency` in SDK claims.
To change an existing limit, use `set_currency_token_limit` instead of `add_currency_limit`.
Only sign Projects verified against the approved Factory; keep a batch within that Factory.
