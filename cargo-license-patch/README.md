# cargo-license-patch

This is a local, library-only patch of [cargo-license v0.5.1](https://github.com/onur/cargo-license) (MIT licensed, © 2016 Onur Aslan).

## Why

The upstream `cargo-license` 0.5.1 pulls in `atty` (via `clap` v3), which has an [unmaintained/vulnerability advisory](https://github.com/advisories/GHSA-g98v-hv3f-hcfr). Since downstream crates (`json-proof-token`, `zkryptium`) only use the **library** API — and `atty`/`clap` are only used by the CLI binary — this patch vendors the identical `lib.rs` while removing `atty`, `clap`, and `ansi_term` from the dependency list.

## What changed

- `src/lib.rs` — **unmodified** copy of upstream `cargo-license` 0.5.1 `src/lib.rs`
- `Cargo.toml` — stripped `atty`, `clap`, and `ansi_term` dependencies; no binary target
