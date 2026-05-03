# Investigation R5 — span-indexed AST design

Round: R5 phase 2 (design note for the structural rewrite chosen as
Option B post-R3).

## Background

R3 measured the bumpalo arena migration (Option A) at gz=17,490 — worse
than the R2 baseline of 15,375. The Vec-monomorphization collapse worked
(150 → 90 functions) but bumpalo's panic paths pulled in `core::str` Debug
formatting and the Unicode `printable.rs` tables, plus the bumpalo crate
itself. Net: +2,115 bytes gz vs R2.

R5 phase 1 reverted the bumpalo migration, leaving the project at
gz=15,298 with `Document<'src>` single-lifetime AST and `Vec<T>` collections.

Phase 3 of R5 must collapse the seven distinct `Vec<T>` monomorphizations
without re-introducing bumpalo's bloat.

## Approaches considered (per Director brief)

### Approach 2A — single typed-erased byte arena

`Document` owns a `Vec<u8>`; AST nodes live at byte offsets; collections
become `(start: u32, len: u32)` byte ranges. Accessors decode lazily.

- Pros: ONE `Vec<u8>`, one allocator instantiation, maximum compression.
- Cons: API ergonomics — every accessor takes `&Document`. Endianness
  and alignment are caller responsibilities. Lifetime juggling for the
  `&'src str` borrows embedded in nodes (would need to either restore via
  span re-slicing, or store packed `&'src str` which has alignment cost).
  Heaviest implementation burden of the three.

### Approach 2B — single typed Node arena (CHOSEN)

`Document` owns ONE `Vec<Node>` where `Node` is a tagged enum covering
every AST kind. All collections become `(start: u32, len: u32)` index
ranges into this single Vec. AST accessors return `Node` references or
thin wrappers.

- Pros: ONE `Vec<Node>` monomorphization. Strong typing preserved
  through the enum tag. Simpler than 2A — no offset/decode dance, no
  alignment concerns, no endianness. `&'src str` slices stay live in the
  enum variants without re-slicing. Uses only safe code in the parser hot
  path.
- Cons: per-node memory waste from the enum tag and from each variant
  being padded to the largest variant's size. Irrelevant for the
  wasm-size budget (which is code size, not data size at runtime).

### Approach 2C — custom inline bump allocator

A tiny ~50-line bump allocator over a single backing `Vec<u8>`, with
panic-free or panic-string-free error paths.

- Pros: closest to a "real" arena experience, low overhead, no crate
  dep.
- Cons: subtle correctness — every alloc/dealloc must be panic-string-
  free, every pointer-cast must be sound, lifetime variance must be
  manually correct. Easy to silently re-introduce `core::fmt`
  reachability via `unwrap()`/`assert!()`. The R3 bumpalo lesson is
  exactly this trap: an arena's panic paths dominate its size cost.
  Highest-risk path.

## Decision: Approach 2B

Rationale (from Director brief, confirmed by phase 1 work):

1. **Code-size win is the goal, not data compactness.** The wasm budget
   is bytes of compiled `.text`. A single `Vec<Node>` instantiation
   replaces seven `Vec<T>` instantiations regardless of `Node`'s in-memory
   layout. The enum-padding waste happens on the heap at runtime, which
   doesn't appear in the wasm size figure.

2. **Simpler than 2A.** No serialization layer; accessors return real
   Rust references with familiar lifetimes. The parser body changes from
   `out.push(value)` to `self.alloc(Node::Foo(value))`-shaped pushes
   plus a `(start, len)` range record at each list collection point.

3. **Lower-risk than 2C.** No new unsafe code beyond what already lives
   in the wasm shim. No new way to silently pull in panic strings.

4. **Public API stays single-lifetime.** `Document<'src>` already
   restored in phase 1; phase 3 keeps it.

## Layout sketch

The crux: every list field on every AST node becomes a `Range32` into
one shared `Vec<Node>`. The `Node` enum carries every AST shape that can
appear inside a list.

```rust
/// Index range into Document::nodes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct NodeRange {
    start: u32,
    len: u32,
}

/// Tagged element of the shared node arena. One Vec<Node> in the document.
enum Node<'src> {
    Definition(Definition<'src>),
    VariableDefinition(VariableDefinition<'src>),
    Directive(Directive<'src>),
    Argument(Argument<'src>),
    Selection(Selection<'src>),
    ObjectField(ObjectField<'src>),
    Value(Value<'src>),
}
```

Each AST struct that previously held `Vec<T>` now holds a `NodeRange`
plus an implicit `kind` (the variant of `Node` it points at; checked at
access time). For example:

```rust
pub struct OperationDefinition<'src> {
    pub kind: OperationKind,
    pub name: Option<Name<'src>>,
    pub variable_definitions: NodeRange, // -> Node::VariableDefinition slice
    pub directives: NodeRange,           // -> Node::Directive slice
    pub selection_set: SelectionSet<'src>,
    ...
}
```

`Type::List` and `Type::NonNull` previously held `Box<Type>` (one heap
allocation per wrapper). They become a single `u32` index into a small
companion `Vec<Type<'src>>` to preserve recursion without `Box`.
Alternative: keep `Box<Type>` — only two `Box` instantiations exist
(List, NonNull) and they reuse the same `Box` machinery, so the win
from collapsing is small. Phase 3 will keep `Box<Type>` initially and
re-evaluate if the size budget needs it.

`Value::List` and `Value::Object` are themselves recursive — their
elements live inside `Document::nodes` as `Node::Value(_)` and
`Node::ObjectField(_)`, accessed via the `NodeRange` pattern above.

### Public API access

The list fields can't be plain `&[T]` slices anymore (different `T`
per range). Two options:

- (a) Generated typed accessors:
  ```rust
  impl<'src> Document<'src> {
      pub fn directives(&self, r: NodeRange) -> impl Iterator<Item = &Directive<'src>>;
      pub fn arguments(&self, r: NodeRange) -> impl Iterator<Item = &Argument<'src>>;
      // ...
  }
  ```
  The corpus tests would need to thread `&doc` through every list-field
  access. Substantial test refactor.

- (b) **Hybrid: Node enum is fat, but we expose `Vec<T>` slices via
  range projection.** For each list-bearing collection (e.g., `directives`),
  we keep the field as `Vec<Directive<'src>>` on its parent struct but
  the *element types themselves* (Argument, Value, etc.) are unified
  through a single `Node` enum where it matters. Inspection of the
  R2 binary shows the seven distinct Vec types are: `Vec<Definition>`,
  `Vec<VariableDefinition>`, `Vec<Directive>`, `Vec<Argument>`,
  `Vec<Selection>`, `Vec<ObjectField>`, `Vec<Value>`. If we make these
  ALL `Vec<Node>` directly (with the enum tag), we get ONE
  monomorphization while keeping the existing `&[T]` access pattern via
  iterator adapters that filter by tag.

Re-evaluation: option (b) is the cleanest path. The collections become
`Vec<Node>` where `Node` is the enum above. Accessor methods on each
parent struct (`Document::definitions(&self) -> impl Iterator<Item =
&Definition>`) project the typed view. **The Vec instantiation count
drops from 7 to 1.**

Test/corpus impact: corpus tests today use `doc.definitions[0]`,
`f.directives.len()`, `op.selection_set.selections[i]`, etc. To keep
the test corpus stable, the parent structs expose:

```rust
impl<'src> OperationDefinition<'src> {
    pub fn directives(&self) -> impl Iterator<Item = &Directive<'src>>;
    // and a len() helper
}
```

Direct `[i]` indexing and `.len()` on the underlying `Vec<Node>` will be
exposed via thin newtype wrappers (`DirectiveSlice<'a, 'src>` etc.) that
preserve the existing call sites. This is the API plan for phase 3.

### Alternative simpler path

If the typed-accessor refactor is too invasive for one round, phase 3
can do the **minimum viable rewrite**: change every `Vec<T>` to
`Vec<Node>` BUT ALSO have parent structs hold typed `&[Node]` slices
that are then projected on read. This still gives ONE monomorphization
of `Vec<Node>` (and zero of `Vec<Definition>`, `Vec<Argument>`, etc).
This is the path phase 3 will implement.

## Risk assessment

- **Risk: bounds-check panic paths from `Vec::index` may still pull in
  the same panic-format machinery as bumpalo did.** Mitigation: use
  `get(i)` (returning `Option<&Node>`) in accessor methods rather than
  `[i]`. The parser builds nodes by `push`, never indexes — so the
  panic-format reachability profile should look like the R2 baseline
  (which didn't have these panic paths), not like the bumpalo binary.

- **Risk: enum-tag matching costs `match` arm instructions per access.**
  Mitigation: irrelevant for wasm size; matters only for runtime perf,
  which is not in this round's budget.

- **Risk: corpus tests break heavily.** Mitigation: keep test API
  compatible by hiding the `Node` enum behind typed accessor methods on
  parent structs. The corpus already uses `.iter()`, `.len()`, and `[i]`
  patterns — these can be preserved via wrapper types implementing
  `Index`, `len`, and `IntoIterator`.

## Rollback plan

If phase 3 measures gz higher than the R5 phase 1 baseline (15,298),
phase 3 is reverted and R5 is reported PARTIAL/FAILED. The structural
fix attempt counter advances to 2/5 either way.

## Acceptance for phase 3

- All 38 R1 tests still pass (lex + corpus).
- All 5 pretty tests still pass.
- All 12 validation tests still pass.
- wasm-smoke ABI test passes (tag=0 / tag=1+kind=34).
- gz size measured and recorded in SIZE.md regardless of outcome.

## Next-largest bloat (post phase 3, if budget not met)

If gz remains > 5,120, R6+ will attack the next-largest contributor.
Phase 3's measurement will include `wasm-objdump -h` and (if available)
`twiggy top` output to identify the next target. Likely candidates:
panic-string elimination, `dlmalloc` swap to `wee_alloc`, custom panic
handler with `core::fmt::Write` shim, or `build-std` (Option F, requires
user approval for nightly toolchain).
