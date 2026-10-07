#!/usr/bin/env bash
# Read the local deployment record and optionally compare it with the network.
set -euo pipefail

usage() {
  printf '%s\n' \
    'Usage: bash scripts/status.sh [--verify]' \
    '' \
    'Shows public Mainnet deployment data from .keys/mainnet/deployment.env.' \
    'By default this reads local files only and never loads private keys.' \
    '--verify checks deployed code and contract links through the configured RPC.' \
    'Verification simulates read-only calls and sends no transactions.'
}
die() { printf '%s\n' "$1" >&2; exit 1; }
network=mainnet verify=false
for arg in "$@"; do
  if [ "$arg" = --help ]; then usage; exit 0; fi
done
while [ "$#" -gt 0 ]; do
  case $1 in
    --verify) verify=true; shift ;;
    *) die 'Unknown option. See --help.' ;;
  esac
done
root=$(git rev-parse --show-toplevel) || die 'Run from the Fuul repository root.'
[ "$PWD" = "$root" ] && [ -f contracts/fuul-manager/Cargo.toml ] ||
  die 'Run from the Fuul repository root.'
directory="$root/.keys/$network"
record="$directory/deployment.env"
for path in "$root/.keys" "$directory" "$directory/stellar" "$record"; do
  [ ! -L "$path" ] || die 'Refusing a symbolic link in the deployment storage path.'
done
[ -f "$record" ] || die "No deployment.env for $network. Run the deploy command on this host first."

# Parse only fixed public fields. Never source a record as shell code.
load_field() {
  local name=$1 line value= found=false
  while IFS= read -r line; do
    case $line in
      "export $name="*)
        [ "$found" = false ] || die "Duplicate $name in deployment.env."
        value=${line#*=}; found=true ;;
    esac
  done < "$record"
  [ "$found" = true ] || die "Missing $name in deployment.env."
  # Bash printf %q writes an empty field as two literal single quotes.
  if [ "$value" = "''" ]; then value=; fi
  case $name in
    FUUL_NETWORK) [[ $value = mainnet ]] ;;
    DEPLOYMENT_STATUS) [[ $value = in_progress || $value = complete ]] ;;
    DEPLOYMENT_STEP) [[ $value =~ ^[a-z][a-z0-9_]*$ ]] ;;
    FUUL_KEY_COUNT) [[ $value =~ ^([1-9]|1[0-9]|20)$ ]] ;;
    *_KEY) [[ $value =~ ^fuul-([1-9]|1[0-9]|20)$ ]] ;;
    *_HASH) [[ -z $value || $value =~ ^[0-9a-fA-F]{64}$ ]] ;;
    INITIAL_CURRENCY_LIMIT) [[ $value =~ ^[1-9][0-9]*$ ]] ;;
    *) [[ -z $value || $value =~ ^[CG][A-Z2-7]{55}$ ]] ;;
  esac || die "Invalid $name in deployment.env."
  printf -v "$name" '%s' "$value"
}
for name in DEPLOYMENT_STATUS DEPLOYMENT_STEP FUUL_NETWORK FUUL_KEY_COUNT \
  ADMIN PAUSER UNPAUSER SIGNER FACTORY_ADMIN COLLECTOR PROJECT_ADMIN \
  ADMIN_KEY PAUSER_KEY UNPAUSER_KEY SIGNER_KEY FACTORY_ADMIN_KEY COLLECTOR_KEY PROJECT_ADMIN_KEY \
  NATIVE CURRENCY INITIAL_CURRENCY_LIMIT MANAGER FACTORY PROJECT \
  MANAGER_HASH FACTORY_HASH PROJECT_HASH; do
  load_field "$name"
done
[ "$FUUL_NETWORK" = "$network" ] || die 'The record belongs to another network.'

printf 'Deployment on %s\n  Status: %s\n  Last step: %s\n' "$network" "$DEPLOYMENT_STATUS" "$DEPLOYMENT_STEP"
printf '  Network: %s\n  Identities: %s\n' "$FUUL_NETWORK" "$FUUL_KEY_COUNT"
printf '\nPublic roles (alias and address):\n'
for role in ADMIN PAUSER UNPAUSER SIGNER FACTORY_ADMIN COLLECTOR PROJECT_ADMIN; do
  alias_name="${role}_KEY"
  printf '  %s  %s  %s\n' "$role" "${!alias_name}" "${!role}"
done
printf '\nContract addresses and build hashes:\n'
# A complete record without a Project comes from deploy.sh --skip-project.
missing='(pending)'
[ "$DEPLOYMENT_STATUS" != complete ] || missing='(not created)'
for name in NATIVE CURRENCY INITIAL_CURRENCY_LIMIT MANAGER FACTORY PROJECT MANAGER_HASH FACTORY_HASH PROJECT_HASH; do
  printf '  %s=%s\n' "$name" "${!name:-$missing}"
done
printf '\nLocal files:\n  Stellar CLI: .keys/%s/stellar/\n  Keys: .keys/%s/keys.env (private)\n  Record: .keys/%s/deployment.env\n' \
  "$network" "$network" "$network"

if [ "$verify" = false ]; then
  printf '\nLocal record only. Run with --verify to check the deployed contracts.\n'
  exit 0
fi
[ "$DEPLOYMENT_STATUS" = complete ] || die 'The deployment is incomplete. Reconcile the last step before network verification.'
command -v stellar >/dev/null || die 'Install Stellar CLI 27.1.0 to verify the deployment.'
[[ $(stellar --version 2>/dev/null) == 'stellar 27.1.0 '* ]] || die 'Use Stellar CLI 27.1.0 to verify the deployment.'
[ -d "$directory/stellar" ] || die 'The Stellar CLI configuration is missing on this host.'
net=(--network fuul --config-dir "$directory/stellar")
source_args=(--source "$ADMIN_KEY" "${net[@]}")
scalar() {
  local value=$1
  if [[ $value == \"*\" ]]; then value=${value#\"}; value=${value%\"}; fi
  [[ $value =~ ^[A-Za-z0-9]+$ ]] || die 'Unexpected contract readback.'
  printf '%s' "$value"
}
printf '\nChecking deployed contracts (read-only):\n'
if [ -z "$PROJECT" ]; then
  result=$(stellar contract invoke --id "$FACTORY" "${source_args[@]}" --send=no -- project_wasm_hash 2>/dev/null) ||
    die 'Could not read Factory.project_wasm_hash. Check the configured RPC and network.'
  [ "$(scalar "$result")" = "$PROJECT_HASH" ] || die 'Factory.project_wasm_hash differs from the deployment record.'
  printf '  [ok] Factory Project WASM hash verified\n'
  deployed=(MANAGER FACTORY)
else
  result=$(stellar contract invoke --id "$PROJECT" "${source_args[@]}" --send=no -- factory 2>/dev/null) ||
    die 'Could not read Project.factory. Check the configured RPC and network.'
  [ "$(scalar "$result")" = "$FACTORY" ] || die 'Project.factory differs from the saved Factory.'
  printf '  [ok] Project Factory link verified\n'
  deployed=(MANAGER FACTORY PROJECT)
fi
result=$(stellar contract invoke --id "$MANAGER" "${source_args[@]}" --send=no -- required_signers 2>/dev/null) ||
  die 'Could not read Manager.required_signers. Check the configured RPC and network.'
[ "$(scalar "$result")" = 1 ] || die 'Manager signer quorum differs from the saved deployment profile.'
printf '  [ok] Manager signer quorum verified\n'
result=$(stellar contract invoke --id "$FACTORY" "${source_args[@]}" --send=no -- has_manager_role --account "$MANAGER" 2>/dev/null) ||
  die 'Could not read Factory.has_manager_role. Check the configured RPC and network.'
[ "$result" = true ] || die 'Factory does not recognize the saved Manager.'
printf '  [ok] Factory Manager role verified\n'
for name in "${deployed[@]}"; do
  expected="${name}_HASH"
  actual=$(stellar contract info hash --id "${!name}" "${net[@]}" 2>/dev/null) ||
    die "Could not read $name code hash. Check the configured RPC and network."
  [ "$actual" = "${!expected}" ] || die "$name code hash differs from the deployment record."
done
printf '  [ok] Deployed code hashes verified\n'
