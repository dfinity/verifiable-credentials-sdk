#!/usr/bin/env bash
set -euo pipefail

# Make sure we always run from the issuer root
RP_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$RP_DIR"
echo $RP_DIR


# Build the frontend
cd frontend/
npm ci
npm run build 
cd ../

# Build the canister
cargo build --release --target wasm32-unknown-unknown --manifest-path ./Cargo.toml -j1
ic-wasm "../target/wasm32-unknown-unknown/release/dummy_relying_party.wasm" -o "./dummy_relying_party.wasm" shrink
ic-wasm dummy_relying_party.wasm -o dummy_relying_party.wasm metadata candid:service -f dummy_relying_party.did -v public
# indicate support for certificate version 1 and 2 in the canister metadata
ic-wasm dummy_relying_party.wasm -o dummy_relying_party.wasm metadata supported_certificate_versions -d "1,2" -v public
gzip --no-name --force "dummy_relying_party.wasm"
