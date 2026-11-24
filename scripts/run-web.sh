#!/usr/bin/env bash
set -euo pipefail

CONFIG="${1:-config/example.yaml}"
BIND="${RAMA_BIND:-127.0.0.1:8090}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo no está disponible en PATH." >&2
  exit 1
fi

exec cargo run --release -- serve --config "$CONFIG" --bind "$BIND"
