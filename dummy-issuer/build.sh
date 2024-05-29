#!/usr/bin/env bash
set -euo pipefail

# Make sure we always run from the issuer root
ISSUER_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$ISSUER_DIR"

# Build the canister
cargo build --release --target wasm32-unknown-unknown --manifest-path ./Cargo.toml -j1
ic-wasm "../target/wasm32-unknown-unknown/release/dummy_issuer.wasm" -o "./dummy_issuer.wasm" shrink
ic-wasm dummy_issuer.wasm -o dummy_issuer.wasm metadata candid:service -f dummy_issuer.did -v public
# indicate support for certificate version 1 and 2 in the canister metadata
ic-wasm dummy_issuer.wasm -o dummy_issuer.wasm metadata supported_certificate_versions -d "1,2" -v public
gzip --no-name --force "dummy_issuer.wasm"
