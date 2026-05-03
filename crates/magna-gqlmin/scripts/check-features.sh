#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Run `cargo check -p magna-gqlmin` across the meaningful feature combos so
# the flag matrix doesn't silently rot. Each invocation echoes itself before
# running for diagnosability.
#
# Note on wasm combos: combos involving `wasm` or `wasm-bindgen` require the
# wasm32-unknown-unknown target and are checked with `--target wasm32-unknown-unknown`.
# The bare `--no-default-features --features ops` combo on wasm32 is NOT valid
# on its own: without `feature = "wasm"`, no global allocator or panic handler
# is wired. Therefore "ops,wasm" is the minimum wasm combo.

set -euo pipefail

CRATE="magna-gqlmin"

run() {
    echo
    echo "==> cargo check -p $CRATE $*"
    cargo check -p "$CRATE" "$@"
}

# Native no-std with `ops` alone is NOT a valid standalone combo: it has no
# global allocator or panic handler. The allocator lives behind `feature = "wasm"`.
# A consumer that wants `ops` in no-std must either also activate `wasm`, or
# provide their own allocator in their crate. This combo is omitted from the
# script intentionally — it will always fail to link. See docs/investigation-r2-wasm-size.md.
#
# TODO(R3): if a bump-allocator approach is adopted, a `no-std-alloc` feature
# that gates just the allocator (without the extern "C" exports) would make this
# combo valid on the host target too.

# Default features (`ops` + `std`).
run

# `ops + wasm` — pure no_std wasm build with dlmalloc allocator and extern "C"
# export shim. The wasm feature provides the global allocator and panic handler.
# Requires wasm32-unknown-unknown target.
echo
echo "==> cargo check -p $CRATE --target wasm32-unknown-unknown --no-default-features --features ops,wasm"
cargo check -p "$CRATE" --target wasm32-unknown-unknown --no-default-features --features "ops,wasm"

# `ops + sdl` — sdl is feature-gated and gates no code in R1/R2, so this just
# proves the flag composes.
run --features ops,sdl

# `ops + sdl + validate` — validate transitively pulls sdl + std.
run --features ops,sdl,validate

# `ops + serde` — serde feature currently gates only future derives.
run --features ops,serde

# `ops + pretty` — pretty requires std; gates no R1/R2 code but the feature
# must compose correctly. Flagged by Verifier R1 as missing.
run --features ops,pretty

# `wasm-bindgen` — escape hatch feature. Pulls in wasm transitively (hence dlmalloc)
# but adds no wasm-bindgen code in R2 (deferred). Verify the flag composes on wasm32.
echo
echo "==> cargo check -p $CRATE --target wasm32-unknown-unknown --no-default-features --features ops,wasm-bindgen"
cargo check -p "$CRATE" --target wasm32-unknown-unknown --no-default-features --features "ops,wasm-bindgen"

echo
echo "All active feature combos passed for $CRATE."
