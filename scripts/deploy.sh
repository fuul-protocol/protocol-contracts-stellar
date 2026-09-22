#!/usr/bin/env bash
# Execute from the repository root. Keys are loaded only inside this process.
if [ "${BASH_SOURCE[0]}" != "$0" ]; then
  printf '%s\n' 'Use: bash scripts/deploy.sh --help (do not source this script).' >&2
  return 1
fi
case $- in
  *x*) set +x; printf '%s\n' 'Disable shell tracing before deployment.' >&2; exit 1 ;;
esac
set -euo pipefail
umask 077

usage() {
  printf '%s\n' \
    'Usage: bash scripts/deploy.sh --prepare-keys [--key-count 1-20]' \
    '       bash scripts/deploy.sh --project-uri VALUE --rpc-url URL --currency ID --currency-limit VALUE' \
    '' \
    'Mainnet only. Prepare keys offline, then fund their public addresses externally.' \
    'The deployment command builds, tests, deploys and verifies on Mainnet.' \
    'Profile: one claim signer, quorum 1, no KYC. Deployment sends real transactions.' \
    '' \
    '  --prepare-keys          Generate or reload Mainnet identities without contacting RPC' \
    '  --project-uri VALUE     Nonempty Project metadata URI (prompted on a terminal)' \
    '  --key-count VALUE       1–20 identities when preparing keys (default 7)' \
    '  --rpc-url VALUE         Mainnet Soroban RPC endpoint' \
    '  --currency VALUE        Verified additional asset contract ID' \
    '  --currency-limit VALUE  Positive U256 base-unit limit' \
    '  --help                  Show help without loading keys or running tools' \
    '' \
    'Mainnet needs existing funded accounts. This script does not fund accounts.' \
    'Requires Stellar CLI 27.1.0, Rust/Cargo 1.92.0, wasm32v1-none, Git, Bash 3.2+.' \
    'Existing deployment.env records are refused. Partial runs require manual reconciliation.' \
    'See deploy.md for prerequisites, custody, records and offline tests.'
}

die() { printf '%s\n' "$1" >&2; exit 1; }
# Never echo CLI diagnostics: RPC URLs and provider errors can contain credentials.
diagnostics=/dev/null
stellar() { command stellar "$@" 2>>"$diagnostics" || return $?; }

network=mainnet prepare_keys=false project_uri= rpc_url= currency= currency_limit= key_count=7
for arg in "$@"; do
  if [ "$arg" = --help ]; then usage; exit 0; fi
done
while [ "$#" -gt 0 ]; do
  if [ "$1" = --prepare-keys ]; then prepare_keys=true; shift; continue; fi
  [ "$#" -ge 2 ] || die 'Every option requires a value. See --help.'
  case $1 in
    --project-uri) project_uri=$2 ;;
    --key-count) key_count=$2 ;;
    --rpc-url) rpc_url=$2 ;;
    --currency) currency=$2 ;;
    --currency-limit) currency_limit=$2 ;;
    *) die 'Unknown option. See --help.' ;;
  esac
  shift 2
done

prompt_missing() {
  local name=$1 label=$2 value
  if [ -z "${!name}" ] && [ -t 0 ]; then
    printf '%s: ' "$label"
    IFS= read -r value || die 'Input ended before configuration was complete.'
    printf -v "$name" '%s' "$value"
  fi
}
case $key_count in [1-9]|1[0-9]|20) ;; *) die '--key-count must be 1–20.' ;; esac
if [ "$prepare_keys" = false ]; then
  prompt_missing project_uri 'Project metadata URI'
  [[ $project_uri =~ [^[:space:]] ]] || die 'Specify a nonempty --project-uri.'
  prompt_missing rpc_url 'Mainnet RPC URL'
  prompt_missing currency 'Mainnet accepted currency contract'
  prompt_missing currency_limit 'Mainnet currency limit in base units'
  [ -n "$rpc_url" ] && [ -n "$currency" ] && [ -n "$currency_limit" ] ||
    die 'Mainnet requires explicit --rpc-url, --currency and --currency-limit.'
  case $rpc_url in https://?*|http://?*) ;; *) die '--rpc-url must be an HTTP(S) endpoint.' ;; esac
  [[ ! $rpc_url =~ [[:space:]] ]] || die '--rpc-url must not contain whitespace.'
fi
passphrase='Public Global Stellar Network ; September 2015'
address() { [[ $1 =~ ^C[A-Z2-7]{55}$ ]] || die 'Expected a contract address from the CLI or --currency.'; }
hash() { [[ $1 =~ ^[0-9a-fA-F]{64}$ ]] || die 'Expected a plain 64-hex WASM hash.'; }
# CLI scalars are either bare values or JSON strings of this restricted alphabet.
scalar() {
  local value=$1
  if [[ $value == \"*\" ]]; then value=${value#\"}; value=${value%\"}; fi
  [[ $value =~ ^[A-Za-z0-9]+$ ]] || die 'Expected a scalar CLI result.'
  printf '%s' "$value"
}
if [ "$prepare_keys" = false ]; then
  address "$currency"
  [[ $currency_limit =~ ^[1-9][0-9]*$ ]] || die '--currency-limit must be a positive integer.'
  max_u256=115792089237316195423570985008687907853269984665640564039457584007913129639935
  if [ "${#currency_limit}" -gt "${#max_u256}" ] ||
     { [ "${#currency_limit}" -eq "${#max_u256}" ] && [[ $currency_limit > $max_u256 ]]; }; then
    die '--currency-limit exceeds U256.'
  fi
fi

# 1. Validate the checkout, storage and pinned toolchain before loading keys.
root=$(git rev-parse --show-toplevel) || die 'Run from the Fuul repository root.'
[ "$PWD" = "$root" ] && [ -f contracts/fuul-manager/Cargo.toml ] ||
  die 'Run from the Fuul repository root.'
key_dir="$root/.keys/$network"
record="$key_dir/deployment.env"
for path in "$root/.keys" "$key_dir" "$key_dir/keys.env" "$key_dir/stellar" "$record"; do
  [ ! -L "$path" ] || die 'Refusing a symbolic link in the deployment/key storage path.'
done
[ "$prepare_keys" = true ] || [ ! -e "$record" ] ||
  die 'deployment.env already exists. Reconcile it manually; no deployment was started.'
[ -z "$(git ls-files -- .keys)" ] || die '.keys contains tracked files; remove them from Git first.'
if [ "$prepare_keys" = false ] && [ ! -f "$key_dir/keys.env" ]; then
  die 'Mainnet identities are missing. Run bash scripts/deploy.sh --prepare-keys first, fund their public addresses, then deploy.'
fi

color= reset=
if [ -t 1 ] && [ -z "${NO_COLOR+x}" ]; then color=$'\033[1;36m'; reset=$'\033[0m'; fi
progress() { printf '\n%s[%s/8] %s%s\n' "$color" "$1" "$2" "$reset"; }
completed() { printf '  %s[ok]%s %s\n' "$color" "$reset" "$1"; }
progress 1 'Preflight'
for tool in stellar git mktemp chmod mv mkdir rmdir rm; do
  command -v "$tool" >/dev/null || die 'Missing a required tool; see --help.'
done
[[ $(stellar --version) == 'stellar 27.1.0 '* ]] || die 'Install Stellar CLI 27.1.0.'
if [ "$prepare_keys" = false ]; then
  for tool in cargo rustc rustup; do
    command -v "$tool" >/dev/null || die 'Missing a required tool; see --help.'
  done
  [[ $(rustc --version 2>/dev/null) == 'rustc 1.92.0 '* ]] || die 'Use the pinned Rust 1.92.0 toolchain.'
  [[ $(cargo --version 2>/dev/null) == 'cargo 1.92.0 '* ]] || die 'Use Cargo 1.92.0 from the pinned toolchain.'
  targets=$(rustup target list --installed 2>/dev/null)
  [[ $'\n'"$targets"$'\n' == *$'\nwasm32v1-none\n'* ]] ||
    die 'Install the build target first: rustup target add wasm32v1-none'
fi
completed 'Pinned tools ready'

# 2. Reuse the keys helper and reserve one private deployment record.
progress 2 'Load identities'
mkdir -p "$root/.keys"
chmod 700 "$root/.keys"
lock="$root/.keys/.deploy-$network.lock"
mkdir "$lock" 2>/dev/null || die 'Another deployment may be running; inspect the deployment lock before retrying.'
temporary= DEPLOYMENT_STATUS=in_progress DEPLOYMENT_STEP=load_keys
finish() {
  local status=$?
  trap - EXIT
  [ -z "$temporary" ] || rm -f "$temporary"
  rmdir "$lock" 2>/dev/null || :
  if [ "$status" -ne 0 ]; then
    if [ "$prepare_keys" = true ]; then
      printf 'Key preparation stopped. Inspect .keys/mainnet/ before retrying.\n' >&2
    elif [ ! -e "$record" ]; then
      printf 'Preflight stopped at: %s\nNo upload or deployment was sent. Check funding or tools and rerun.\n' \
        "$DEPLOYMENT_STEP" >&2
    else
      printf 'Deployment stopped at: %s\nRecord: %s\nDo not retry until the last operation is reconciled.\n' \
        "$DEPLOYMENT_STEP" "$record" >&2
    fi
    if [ "$diagnostics" != /dev/null ]; then
      printf 'Private diagnostics: %s\n' "$diagnostics" >&2
    fi
  fi
  exit "$status"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# Keep one private log without precreating the network's key directory.
diagnostics=$(mktemp "$root/.keys/deploy-$network.log.XXXXXX")
chmod 600 "$diagnostics"
existing_keys=false
[ ! -f "$key_dir/keys.env" ] || existing_keys=true
# shellcheck disable=SC1091
source scripts/keys.sh "$network" "$key_count" >>"$diagnostics" 2>&1 ||
  die 'Unable to load identities. Inspect the private diagnostics.'
[ "$FUUL_NETWORK" = "$network" ] && [ "$FUUL_KEYS_CONFIG" = "$key_dir/stellar" ] ||
  die 'Existing keys do not match the selected network/configuration.'
case $FUUL_KEY_COUNT in [1-9]|1[0-9]|20) ;; *) die 'Invalid key count in the existing keys file.' ;; esac
for role in ADMIN PAUSER UNPAUSER SIGNER FACTORY_ADMIN COLLECTOR PROJECT_ADMIN; do
  [[ ${!role} =~ ^G[A-Z2-7]{55}$ ]] || die 'Invalid public role address in the keys file.'
  alias_name="${role}_KEY"
  [[ ${!alias_name} =~ ^fuul-([1-9]|1[0-9]|20)$ ]] || die 'Invalid role alias in the keys file.'
done
chmod 700 "$key_dir" "$FUUL_KEYS_CONFIG"
chmod 600 "$key_dir/keys.env"
for ((i=1; i<=FUUL_KEY_COUNT; i++)); do
  public_name="FUUL_KEY_${i}_PUBLIC"
  [[ ${!public_name-} =~ ^G[A-Z2-7]{55}$ ]] || die 'Invalid public key address in the keys file.'
done
if [ "$existing_keys" = true ]; then
  key_action=loaded
  completed "Loaded $FUUL_KEY_COUNT Mainnet identities"
else
  key_action=created
  completed "Created $FUUL_KEY_COUNT Mainnet identities"
fi
printf '  Stellar CLI identities: .keys/%s/stellar/\n  Private key exports: .keys/%s/keys.env\n  Public addresses:\n' \
  "$network" "$network"
for ((i=1; i<=FUUL_KEY_COUNT; i++)); do
  public_name="FUUL_KEY_${i}_PUBLIC"
  completed "fuul-$i $key_action  ${!public_name}"
done
printf '  Contract roles:\n'
for role in ADMIN PAUSER UNPAUSER SIGNER FACTORY_ADMIN COLLECTOR PROJECT_ADMIN; do
  alias_name="${role}_KEY"
  printf '    %s  %s  %s\n' "$role" "${!alias_name}" "${!role}"
done
if [ "$prepare_keys" = true ]; then
  printf '\nFund these public addresses with XLM through the client custody process.\n'
  printf '%s\n' 'No accounts were funded and no deployment transactions were sent.'
  printf '%s\n' 'After funding, run the Mainnet deployment command in deploy.md from this checkout.'
  exit 0
fi
CURRENCY=$currency INITIAL_CURRENCY_LIMIT=$currency_limit
net=(--network fuul --config-dir "$FUUL_KEYS_CONFIG")
source_args=(--source "$ADMIN_KEY" "${net[@]}")

# 3. Check that Mainnet accounts exist before reserving a deployment record.
progress 3 'Check Mainnet accounts'
DEPLOYMENT_STEP=configure_network
stellar network add fuul --rpc-url "$rpc_url" --network-passphrase "$passphrase" \
  --config-dir "$FUUL_KEYS_CONFIG" >/dev/null
completed 'Network configured'
for ((i=1; i<=FUUL_KEY_COUNT; i++)); do
  DEPLOYMENT_STEP="check_account_$i"
  stellar ledger entry fetch account --account "fuul-$i" "${net[@]}" >/dev/null ||
    die 'Mainnet accounts must already exist and be funded. Check their XLM and RPC access.'
  completed "Account fuul-$i found"
done
DEPLOYMENT_STEP=accounts_ready

# 4. Build all workspace artifacts and run the same tests as the manual guide.
progress 4 'Build and test'
DEPLOYMENT_STEP=build
stellar contract build --locked >>"$diagnostics"
completed 'Contracts built'
DEPLOYMENT_STEP=test
cargo test --workspace --locked >>"$diagnostics" 2>&1
completed 'Contract tests passed'
WASM=target/wasm32v1-none/release
for name in project manager factory; do
  [ -f "$WASM/fuul_$name.wasm" ] || die 'Missing a production WASM after build.'
done

# 5. Select native XLM and the additional accepted asset.
progress 5 'Select currencies'
DEPLOYMENT_STEP=select_currencies
NATIVE=$(stellar contract id asset --asset native "${net[@]}")
address "$NATIVE"
completed "Native asset: $NATIVE"
[ "$CURRENCY" != "$NATIVE" ] || die 'The additional currency must differ from native XLM.'
completed "Accepted currency: $CURRENCY (initial limit: $INITIAL_CURRENCY_LIMIT)"

# Reserve progress only before the first on-chain write.
(set -C; : > "$record") 2>/dev/null || die 'A deployment record already exists; refusing to replace it.'
PROJECT_HASH= MANAGER_HASH= FACTORY_HASH= MANAGER= FACTORY= PROJECT=
save_record() {
  local name
  temporary=$(mktemp "$key_dir/.deployment.env.XXXXXX")
  {
    printf '# Public deployment values. in_progress is not a verified deployment.\n'
    for name in DEPLOYMENT_STATUS DEPLOYMENT_STEP FUUL_NETWORK FUUL_KEYS_CONFIG FUUL_KEY_COUNT \
      ADMIN PAUSER UNPAUSER SIGNER FACTORY_ADMIN COLLECTOR PROJECT_ADMIN \
      ADMIN_KEY PAUSER_KEY UNPAUSER_KEY SIGNER_KEY FACTORY_ADMIN_KEY COLLECTOR_KEY PROJECT_ADMIN_KEY \
      NATIVE CURRENCY INITIAL_CURRENCY_LIMIT PROJECT_HASH MANAGER_HASH FACTORY_HASH MANAGER FACTORY PROJECT; do
      printf 'export %s=%q\n' "$name" "${!name}"
    done
  } > "$temporary"
  chmod 600 "$temporary"
  mv -f "$temporary" "$record"
  temporary=
}
checkpoint() { DEPLOYMENT_STEP=$1; save_record; }
checkpoint ready_to_upload
printf '  Progress record: .keys/%s/deployment.env\n' "$network"

# 6. Upload only production WASM and compare the exact local bytes.
progress 6 'Upload three production contracts'
for name in project manager factory; do
  local_hash=$(stellar contract info hash --wasm "$WASM/fuul_$name.wasm" --config-dir "$FUUL_KEYS_CONFIG")
  hash "$local_hash"
  checkpoint "upload_$name"
  uploaded=$(stellar contract upload --wasm "$WASM/fuul_$name.wasm" "${source_args[@]}" \
    --instruction-leeway 3000000 --optimize=false)
  hash "$uploaded"
  case $name in
    project) PROJECT_HASH=$uploaded; label=Project ;;
    manager) MANAGER_HASH=$uploaded; label=Manager ;;
    factory) FACTORY_HASH=$uploaded; label=Factory ;;
  esac
  checkpoint "uploaded_$name"
  [ "$uploaded" = "$local_hash" ] || die 'Uploaded WASM hash differs from the local build.'
  completed "$label WASM uploaded: $uploaded"
done

# 7. Instantiate Manager/Factory, then submit Project creation explicitly.
progress 7 'Deploy Manager, Factory and Project'
checkpoint deploy_manager
MANAGER=$(stellar contract deploy --wasm-hash "$MANAGER_HASH" "${source_args[@]}" -- \
  --admin "$ADMIN" --pauser "$PAUSER" --unpauser "$UNPAUSER" \
  --initial_required_signers 1 --claim_signers "[\"$SIGNER\"]" \
  --accepted_currency "$CURRENCY" --native_asset "$NATIVE" \
  --initial_kyc_validator null --initial_currency_limit "$INITIAL_CURRENCY_LIMIT")
address "$MANAGER"
checkpoint manager_deployed
completed "Manager deployed: $MANAGER"
checkpoint deploy_factory
FACTORY=$(stellar contract deploy --wasm-hash "$FACTORY_HASH" "${source_args[@]}" -- \
  --admin "$FACTORY_ADMIN" --manager "$MANAGER" --fee_collector "$COLLECTOR" \
  --project_wasm_hash "$PROJECT_HASH")
address "$FACTORY"
checkpoint factory_deployed
completed "Factory deployed: $FACTORY"
checkpoint create_project
result=$(stellar contract invoke --id "$FACTORY" "${source_args[@]}" --send=yes -- \
  create_fuul_project --project_admin "$PROJECT_ADMIN" --project_info_uri "$project_uri" --kyc_required false)
PROJECT=$(scalar "$result")
address "$PROJECT"
checkpoint project_created
completed "Project created: $PROJECT"

# 8. Simulate readbacks and verify all deployed code before declaring completion.
progress 8 'Verify deployment'
checkpoint verify
result=$(stellar contract invoke --id "$PROJECT" "${source_args[@]}" --send=no -- factory)
[ "$(scalar "$result")" = "$FACTORY" ] || die 'Project.factory does not match the deployed Factory.'
completed 'Project Factory link verified'
result=$(stellar contract invoke --id "$MANAGER" "${source_args[@]}" --send=no -- required_signers)
[ "$(scalar "$result")" = 1 ] || die 'Manager signer quorum is not 1.'
completed 'Manager signer quorum verified'
result=$(stellar contract invoke --id "$FACTORY" "${source_args[@]}" --send=no -- has_manager_role --account "$MANAGER")
[ "$result" = true ] || die 'Factory does not recognize the deployed Manager.'
completed 'Factory Manager role verified'
for name in MANAGER FACTORY PROJECT; do
  expected="${name}_HASH"
  actual=$(stellar contract info hash --id "${!name}" "${net[@]}")
  hash "$actual"
  [ "$actual" = "${!expected}" ] || die 'Deployed WASM hash differs from the verified upload.'
done
completed 'Deployed code hashes verified'
DEPLOYMENT_STATUS=complete
checkpoint verified
printf '\nDeployment complete.\nMANAGER=%s\nFACTORY=%s\nPROJECT=%s\nNATIVE=%s\nCURRENCY=%s\nPROJECT_HASH=%s\nMANAGER_HASH=%s\nFACTORY_HASH=%s\nRecord: %s\n' \
  "$MANAGER" "$FACTORY" "$PROJECT" "$NATIVE" "$CURRENCY" "$PROJECT_HASH" "$MANAGER_HASH" "$FACTORY_HASH" "$record"
printf 'Private diagnostics: %s\n' "$diagnostics"
printf '%s\n' 'Transaction receipts: not collected. Save transaction hashes separately.'
printf '\nLoad these values in your shell (from the repository root):\nsource scripts/keys.sh %s\nsource .keys/%s/deployment.env\n' \
  "$network" "$network"
