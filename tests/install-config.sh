#!/usr/bin/env bash
set -euo pipefail
repo_dir=$(cd "$(dirname "$0")/.." && pwd)
source "$repo_dir/deploy/configure-env.sh"
test_dir=$(mktemp -d "$repo_dir/.install-config.XXXXXX")
trap 'rm -rf "$test_dir"' EXIT
configure_env "$test_dir/.env" "$repo_dir/.env.example" > /dev/null
(
  source "$test_dir/.env"
  [ "${#AETHERIA_SESSION_SECRET}" -ge 64 ]
  [ "${#AETHERIA_ENCRYPTION_KEY}" -eq 32 ]
  [ "${#INITIAL_PASSWORD}" -ge 12 ]
  [ "$INITIAL_PASSWORD" != password ]
)
[ "$(stat -c '%a' "$test_dir/.env")" = 600 ]
cp "$test_dir/.env" "$test_dir/original"
configure_env "$test_dir/.env" "$repo_dir/.env.example" > /dev/null
cmp "$test_dir/.env" "$test_dir/original"
if INITIAL_PASSWORD=password configure_env "$test_dir/rejected" "$repo_dir/.env.example" > /dev/null 2>&1; then
  printf 'weak password was accepted\n' >&2
  exit 1
fi
[ ! -e "$test_dir/rejected" ]
# a failed secret command must not leave a partial configuration
if (openssl() { return 1; }; configure_env "$test_dir/failed" "$repo_dir/.env.example") > /dev/null 2>&1; then
  printf 'failed secret generation was accepted\n' >&2
  exit 1
fi
[ ! -e "$test_dir/failed" ]
printf 'installer configuration checks passed\n'
