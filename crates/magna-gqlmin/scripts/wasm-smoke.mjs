#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Smoke test for the magna-gqlmin wasm runtime.
//
// Usage:
//   node scripts/wasm-smoke.mjs [path/to/gqlmin.opt.wasm]
//
// Default wasm path: /tmp/gqlmin.opt.wasm (produced by the build pipeline).
//
// Exits 0 on success, 1 on failure.
//
// This script exercises the actual .wasm artifact — it does NOT test the
// Rust-native parse path. It validates:
//   1. gqlmin_alloc / gqlmin_free work (no crash)
//   2. gqlmin_parse returns tag=0 for a valid document
//   3. gqlmin_parse returns tag=1 + kind=34 (EmptySelectionSet) for { }
//   4. gqlmin_result_free can be called without crash

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

// ---------------------------------------------------------------------------
// Wasm path resolution
// ---------------------------------------------------------------------------

const __dir = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = process.argv[2] ?? "/tmp/gqlmin.opt.wasm";

// ---------------------------------------------------------------------------
// Corpus inputs
// ---------------------------------------------------------------------------

// From tests/corpus/simple_query.graphql
const SIMPLE_QUERY = "{ hello }";

// From tests/corpus/empty_selection_error.graphql
// "query Q {\n}" should produce EmptySelectionSet (kind=34)
const EMPTY_SELECTION = "query Q {\n}";

// EmptySelectionSet discriminant (see src/error.rs)
const KIND_EMPTY_SELECTION_SET = 34;

// ---------------------------------------------------------------------------
// Load and instantiate
// ---------------------------------------------------------------------------

const wasmBytes = readFileSync(wasmPath);
const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const wasm = instance.exports;

const memory = /** @type {WebAssembly.Memory} */ (wasm.memory);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Encode a JS string as UTF-8 and write into wasm linear memory. */
function encodeUtf8(str) {
  const encoded = new TextEncoder().encode(str);
  const ptr = wasm.gqlmin_alloc(encoded.length);
  if (ptr === 0) throw new Error("gqlmin_alloc returned null");
  new Uint8Array(memory.buffer).set(encoded, ptr);
  return { ptr, len: encoded.length };
}

/**
 * Read the result buffer returned by gqlmin_parse.
 * Layout: [u8 tag][u32-le payload_len][...payload]
 * Returns { tag, spanStart?, spanEnd?, kind? }
 */
function readResult(resultPtr) {
  const buf = new Uint8Array(memory.buffer);
  const view = new DataView(memory.buffer);

  const tag = buf[resultPtr];
  const payloadLen = view.getUint32(resultPtr + 1, /* littleEndian */ true);

  if (tag === 0) {
    // Success: no payload beyond the header.
    return { tag: 0 };
  } else {
    // Error: [u32-le span_start][u32-le span_end][u8 kind]
    const spanStart = view.getUint32(resultPtr + 5, true);
    const spanEnd = view.getUint32(resultPtr + 9, true);
    const kind = buf[resultPtr + 13];
    return { tag: 1, spanStart, spanEnd, kind };
  }
}

// ---------------------------------------------------------------------------
// Test 1: valid document → tag = 0
// ---------------------------------------------------------------------------

{
  const { ptr, len } = encodeUtf8(SIMPLE_QUERY);
  const resultPtr = wasm.gqlmin_parse(ptr, len);

  if (resultPtr === 0) {
    console.error("FAIL test 1: gqlmin_parse returned null for valid input");
    process.exit(1);
  }

  const result = readResult(resultPtr);

  if (result.tag !== 0) {
    console.error(
      `FAIL test 1: expected tag=0 (success), got tag=${result.tag} kind=${result.kind} ` +
        `span=${result.spanStart}..${result.spanEnd}`
    );
    process.exit(1);
  }

  wasm.gqlmin_result_free(resultPtr);
  wasm.gqlmin_free(ptr, len);

  console.log("PASS test 1: valid document parsed successfully (tag=0)");
}

// ---------------------------------------------------------------------------
// Test 2: empty selection set → tag = 1, kind = 34 (EmptySelectionSet)
// ---------------------------------------------------------------------------

{
  const { ptr, len } = encodeUtf8(EMPTY_SELECTION);
  const resultPtr = wasm.gqlmin_parse(ptr, len);

  if (resultPtr === 0) {
    console.error("FAIL test 2: gqlmin_parse returned null for error input");
    process.exit(1);
  }

  const result = readResult(resultPtr);

  if (result.tag !== 1) {
    console.error(
      `FAIL test 2: expected tag=1 (error), got tag=${result.tag}`
    );
    process.exit(1);
  }

  if (result.kind !== KIND_EMPTY_SELECTION_SET) {
    console.error(
      `FAIL test 2: expected kind=${KIND_EMPTY_SELECTION_SET} (EmptySelectionSet), got kind=${result.kind}`
    );
    process.exit(1);
  }

  wasm.gqlmin_result_free(resultPtr);
  wasm.gqlmin_free(ptr, len);

  console.log(
    `PASS test 2: empty selection error detected (tag=1, kind=${result.kind}, ` +
      `span=${result.spanStart}..${result.spanEnd})`
  );
}

// ---------------------------------------------------------------------------
// Done
// ---------------------------------------------------------------------------

console.log("smoke: ok");
