#!/usr/bin/env bash
# Source this file from the repository root to export the generated identities.
if [ -n "${ZSH_VERSION-}" ]; then
  case $ZSH_EVAL_CONTEXT in *:file) ;; *) printf '%s\n' 'Use: source scripts/keys.sh [testnet|mainnet] [count]' >&2; exit 1 ;; esac
elif [ -n "${BASH_VERSION-}" ]; then
  if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    printf '%s\n' 'Use: source scripts/keys.sh [testnet|mainnet] [count]' >&2
    exit 1
  fi
else
  printf '%s\n' 'Use Bash or Zsh.' >&2
  return 1
fi

_fuul_keys() {
  local root network count config file exclude i public secret role index
  case $- in *x*) printf '%s\n' 'Disable shell tracing (set +x) before loading keys.' >&2; return 1 ;; esac
  command -v stellar >/dev/null || { printf '%s\n' 'Install Stellar CLI 27.1.0 first.' >&2; return 1; }
  root=$(git rev-parse --show-toplevel) || return 1
  if [ "$PWD" != "$root" ] || [ ! -f contracts/fuul-manager/Cargo.toml ]; then
    printf '%s\n' 'Run this from the Fuul contracts repository root.' >&2
    return 1
  fi
  network=${1-}
  if [ -z "$network" ]; then
    printf 'Network [testnet/mainnet] (testnet): '
    IFS= read -r network || return 1
    network=${network:-testnet}
  fi
  case $network in testnet|mainnet) ;; *) printf '%s\n' 'Choose testnet or mainnet.' >&2; return 1 ;; esac
  config="$root/.keys/$network/stellar"
  file="$root/.keys/$network/keys.env"
  if [ -L "$root/.keys" ] || [ -L "$root/.keys/$network" ] || [ -L "$file" ]; then
    printf '%s\n' 'Refusing a symbolic link in the key storage path.' >&2
    return 1
  fi
  if [ -n "$(git ls-files -- .keys)" ]; then
    printf '%s\n' '.keys contains tracked files. Remove them from Git before generating credentials.' >&2
    return 1
  fi
  # Local clone protection only: the tracked .gitignore is unchanged.
  exclude=$(git rev-parse --git-path info/exclude) || return 1
  mkdir -p "$(dirname "$exclude")" || return 1
  if ! git check-ignore -q .keys/fuul-probe; then
    printf '\n/.keys/\n' >> "$exclude" || return 1
  fi
  if [ -f "$file" ]; then
    # This is the private shell file generated below, not a downloaded config.
    # shellcheck disable=SC1090
    . "$file" || return 1
    printf 'Loaded existing %s identities. No keys were replaced.\n' "$network"
    return 0
  fi
  if [ -d "$root/.keys/$network" ]; then
    printf '%s\n' 'An incomplete key directory already exists. Inspect it before retrying; no keys were replaced.' >&2
    return 1
  fi
  count=${2-}
  if [ -z "$count" ]; then
    printf 'How many addresses? [1-20] (7): '
    IFS= read -r count || return 1
    count=${count:-7}
  fi
  case $count in [1-9]|1[0-9]|20) ;; *) printf '%s\n' 'Count must be an integer from 1 to 20.' >&2; return 1 ;; esac
  if [ "$count" -lt 7 ]; then
    printf '%s\n' 'Fewer than seven addresses means some roles share a key.'
  fi
  (
    umask 077
    mkdir -p "$config" || exit 1
    chmod 700 "$root/.keys" "$root/.keys/$network" "$config" || exit 1
    for ((i=1; i<=count; i++)); do
      stellar keys generate "fuul-$i" --as-secret --config-dir "$config" --quiet >/dev/null || exit 1
    done
    {
      printf '# Private generated identities. Do not share or commit this file.\n'
      printf 'export FUUL_NETWORK=%s\n' "$network"
      printf 'export FUUL_KEYS_CONFIG=%q\n' "$config"
      printf 'export FUUL_KEY_COUNT=%s\n' "$count"
      for ((i=1; i<=count; i++)); do
        public=$(stellar keys address "fuul-$i" --config-dir "$config") || exit 1
        secret=$(stellar keys secret "fuul-$i" --config-dir "$config") || exit 1
        printf 'export FUUL_KEY_%s_PUBLIC=%s\n' "$i" "$public"
        printf 'export FUUL_KEY_%s_SECRET=%s\n' "$i" "$secret"
      done
      index=1
      for role in ADMIN PAUSER UNPAUSER SIGNER FACTORY_ADMIN COLLECTOR PROJECT_ADMIN; do
        i=$(( (index - 1) % count + 1 ))
        public=$(stellar keys address "fuul-$i" --config-dir "$config") || exit 1
        secret=$(stellar keys secret "fuul-$i" --config-dir "$config") || exit 1
        printf 'export %s=%s\n' "$role" "$public"
        printf 'export %s_KEY=fuul-%s\n' "$role" "$i"
        printf 'export %s_SECRET=%s\n' "$role" "$secret"
        index=$((index + 1))
      done
    } > "$file.tmp" || exit 1
    chmod 600 "$file.tmp" || exit 1
    mv "$file.tmp" "$file" || exit 1
  ) || return 1
  # shellcheck disable=SC1090
  . "$file" || return 1
  printf 'Created and loaded %s %s identities in .keys/%s/.\n' "$count" "$network" "$network"
  printf '%s\n' 'No accounts were funded and no transactions were sent.'
  printf 'ADMIN=%s\nSIGNER=%s\nPROJECT_ADMIN=%s\n' "$ADMIN" "$SIGNER" "$PROJECT_ADMIN"
}

if _fuul_keys "$@"; then
  unset -f _fuul_keys
  return 0
else
  unset -f _fuul_keys
  return 1
fi
