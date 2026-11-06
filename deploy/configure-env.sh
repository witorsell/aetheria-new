#!/usr/bin/env bash

configure_env() (
  set -euo pipefail
  umask 077
  local env_file="${1:-.env}"
  local example_file="${2:-.env.example}"
  [ ! -e "$env_file" ] || return 0

  local session_secret encryption_key initial_password temporary_file
  session_secret=$(openssl rand -base64 64 | tr -d '\n') || return 1
  encryption_key=$(openssl rand -base64 24) || return 1
  initial_password="${INITIAL_PASSWORD:-$(openssl rand -base64 24)}"
  if [ "${#initial_password}" -lt 12 ] || [ "$initial_password" = password ]; then
    printf 'INITIAL_PASSWORD must contain at least 12 characters\n' >&2
    return 1
  fi
  # keep replacement values safe for sed and dotenv
  if [[ "$initial_password" == *[!a-zA-Z0-9+/=._-]* ]]; then
    printf 'installer INITIAL_PASSWORD must use letters, digits, or +/=._-\n' >&2
    return 1
  fi
  temporary_file=$(mktemp "${env_file}.XXXXXX")
  trap 'rm -f "$temporary_file"' EXIT
  sed -e "s#^AETHERIA_SESSION_SECRET=.*#AETHERIA_SESSION_SECRET=${session_secret}#" \
      -e "s#^AETHERIA_ENCRYPTION_KEY=.*#AETHERIA_ENCRYPTION_KEY=${encryption_key}#" \
      -e "s#^INITIAL_PASSWORD=.*#INITIAL_PASSWORD=${initial_password}#" \
      "$example_file" > "$temporary_file"
  mv "$temporary_file" "$env_file"
  printf 'initial admin password: %s\n' "$initial_password"
)
