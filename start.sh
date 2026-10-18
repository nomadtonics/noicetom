#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"
NOICETOM_STORAGE_PATH=${NOICETOM_STORAGE_PATH:-./data/noicetom.sqlite}
SERVER_SECRET=${SERVER_SECRET:-dev-secret}
cargo run --release
