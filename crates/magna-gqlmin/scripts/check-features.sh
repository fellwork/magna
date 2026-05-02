#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Run `cargo check -p magna-gqlmin` across the meaningful feature combos so
# the flag matrix doesn't silently rot. Each invocation echoes itself before
# running for diagnosability.

set -euo pipefail

CRATE="magna-gqlmin"

run() {
    echo
    echo "==> cargo check -p $CRATE $*"
    cargo check -p "$CRATE" "$@"
}

# Default features (`ops` + `std`).
run

# Round 1 deferred: `ops` alone (no `std`) builds a no_std cdylib which on
# the host target requires a global allocator + panic_handler that the
# wasm shim (R2) provides. The combo is correct in principle and the
# feature itself is declared; deferring the host-target link until R2 wires
# the wasm export shim.
# TODO(R2): re-enable once the wasm shim ships.
# run --no-default-features --features ops
# run --no-default-features --features ops,wasm

# `ops + sdl` — sdl is feature-gated and gates no code in R1, so this just
# proves the flag composes.
run --features ops,sdl

# `ops + sdl + validate` — validate transitively pulls sdl + std.
run --features ops,sdl,validate

# `ops + serde` — serde feature is currently gates only future derives.
run --features ops,serde

echo
echo "All active feature combos passed for $CRATE."
