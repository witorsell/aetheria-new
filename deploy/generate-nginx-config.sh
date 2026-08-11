#!/usr/bin/env bash
# auto-generates deploy/nginx-aetheria.conf from .env variables (AETHERIA_BIND, MAX_UPLOAD_SIZE_MB, AETHERIA_DOMAIN)
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ -f "$ROOT_DIR/.env" ]; then
    set -a
    source "$ROOT_DIR/.env"
    set +a
fi

AETHERIA_BIND="${AETHERIA_BIND:-127.0.0.1:4310}"
MAX_UPLOAD_SIZE_MB="${MAX_UPLOAD_SIZE_MB:-25}"
AETHERIA_DOMAIN="${AETHERIA_DOMAIN:-aetheria.silt.im}"

TEMPLATE_FILE="$SCRIPT_DIR/nginx-aetheria.conf.template"
OUTPUT_FILE="$SCRIPT_DIR/nginx-aetheria.conf"

sed \
    -e "s|\${AETHERIA_BIND}|$AETHERIA_BIND|g" \
    -e "s|\${MAX_UPLOAD_SIZE_MB}|$MAX_UPLOAD_SIZE_MB|g" \
    -e "s|\${AETHERIA_DOMAIN}|$AETHERIA_DOMAIN|g" \
    "$TEMPLATE_FILE" > "$OUTPUT_FILE"

echo "generated $OUTPUT_FILE (bind: $AETHERIA_BIND, upload limit: ${MAX_UPLOAD_SIZE_MB}M, domain: $AETHERIA_DOMAIN)"
