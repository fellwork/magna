// SPDX-License-Identifier: MIT OR Apache-2.0
//! Operations-only GraphQL validation rules (R4, step 9 partial).
//!
//! These five rules can be checked from the executable document alone — no
//! schema is required. They are the subset of the GraphQL spec's static
//! validation rules that don't depend on type information:
//!
//! 1. `NoUndefinedVariables` (5.8.3)
//! 2. `NoUnusedVariables`     (5.8.4)
//! 3. `NoUnusedFragments`     (5.5.1.4)
//! 4. `KnownFragmentNames`    (5.5.2.1)
//! 5. `UniqueOperationNames`  (5.2.1.1)
//!
//! The full schema-aware rule set (FieldsOnCorrectType, KnownTypeNames,
//! ScalarLeafs, ArgumentsOfCorrectType, etc.) is deferred to R6 once the
//! SDL parser lands.
//!
//! Gated behind the `validate` feature which implies `std`. The validator
//! is host-side tooling — it is intentionally not built into the wasm
//! runtime.
//!
//! ## R3-aftermath note
//!
//! The AST carries two lifetimes (`'src` over the input text, `'bump` over
//! the bumpalo arena). The validator just walks borrowed shared references,
//! so it doesn't care which arena owns the children — it threads both
//! lifetimes through opaquely.

use std::collections::{BTreeMap, BTreeSet};
use std::vec::Vec;

use crate::lex::Span;
use crate::parse::{
    Argument, Directive, Document, FragmentDefinition, FragmentSpread, InlineFragment,
    ObjectField, OperationDefinition, Selection, SelectionSet, Value,
};

/// A validation finding, returned by [`validate_operations`].
///
/// `rule` is one of the rule-name string constants exported below
/// (`RULE_NO_UNDEFINED_VARIABLES`, etc.) so callers can cheaply branch
/// on the rule identifier without parsing the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub rule: &'static str,
    pub span: Span,
    pub message: &'static str,
}

pub const RULE_NO_UNDEFINED_VARIABLES: &str = "NoUndefinedVariables";
pub const RULE_NO_UNUSED_VARIABLES: &str = "NoUnusedVariables";
pub const RULE_NO_UNUSED_FRAGMENTS: &str = "NoUnusedFragments";
pub const RULE_KNOWN_FRAGMENT_NAMES: &str = "KnownFragmentNames";
pub const RULE_UNIQUE_OPERATION_NAMES: &str = "UniqueOperationNames";

/// Run all five operations-only validation rules over the document.
///
/// The returned `Vec` is empty iff the document is valid under these rules.
/// Errors are emitted in a stable, document-order traversal so callers can
/// rely on the ordering for UI rendering.
pub fn validate_operations<'src, 'bump>(
    doc: &Document<'src, 'bump>,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Index fragment definitions by name for `KnownFragmentNames` and to
    // support `NoUnusedFragments`. We also collect operations for the
    // variable-rules pass and the unique-names check.
    let mut fragments_by_name: BTreeMap<&str, &FragmentDefinition<'src, 'bump>> =
        BTreeMap::new();
    // Track usage counts so we can emit a `NoUnusedFragments` error per
    // unused fragment in document order.
    let mut fragment_use_counts: BTreeMap<&str, usize> = BTreeMap::new();
    // Order-preserving list of fragment definitions for the unused pass.
    let mut fragment_defs_in_order: Vec<&FragmentDefinition<'src, 'bump>> = Vec::new();
    let mut operations: Vec<&OperationDefinition<'src, 'bump>> = Vec::new();

    for def in &doc.definitions {
        match def {
            crate::parse::Definition::Operation(op) => operations.push(op),
            crate::parse::Definition::Fragment(frag) => {
                fragments_by_name.insert(frag.name.value, frag);
                fragment_use_counts.insert(frag.name.value, 0);
                fragment_defs_in_order.push(frag);
            }
        }
    }

    // --- Rule 5: UniqueOperationNames -----------------------------------
    //
    // Two operations may not share a name. An anonymous (unnamed) operation
    // is permitted only if it is the SOLE operation in the document.
    check_unique_operation_names(&operations, &mut errors);

    // --- Rules 1+2 (variables): per-operation pass ----------------------
    //
    // For each operation, collect declared vars, walk the operation's
    // selection set + every transitively spread fragment, gather used
    // variable names, then diff.
    for op in &operations {
        check_variables_for_operation(op, &fragments_by_name, &mut errors);
    }

    // --- Rules 3+4 (fragments): document-wide pass ----------------------
    //
    // Walk every operation AND every fragment body, recording each fragment
    // spread. Spreads to undefined fragments => KnownFragmentNames. Defined
    // fragments with zero spreads => NoUnusedFragments.
    for op in &operations {
        walk_fragment_spreads(
            &op.selection_set,
            &fragments_by_name,
            &mut fragment_use_counts,
            &mut errors,
        );
    }
    for frag in &fragment_defs_in_order {
        walk_fragment_spreads(
            &frag.selection_set,
            &fragments_by_name,
            &mut fragment_use_counts,
            &mut errors,
        );
    }
    for frag in &fragment_defs_in_order {
        if fragment_use_counts
            .get(frag.name.value)
            .copied()
            .unwrap_or(0)
            == 0
        {
            errors.push(ValidationError {
                rule: RULE_NO_UNUSED_FRAGMENTS,
                span: frag.span,
                message: "fragment is defined but never spread",
            });
        }
    }

    errors
}

// --- UniqueOperationNames -----------------------------------------------

fn check_unique_operation_names<'src, 'bump>(
    operations: &[&OperationDefinition<'src, 'bump>],
    errors: &mut Vec<ValidationError>,
) {
    // Anonymous-only-if-sole rule: if any op is anonymous AND there's more
    // than one operation, flag every anonymous op.
    if operations.len() > 1 {
        for op in operations {
            if op.name.is_none() {
                errors.push(ValidationError {
                    rule: RULE_UNIQUE_OPERATION_NAMES,
                    span: op.span,
                    message: "anonymous operation is only allowed when it is the sole operation",
                });
            }
        }
    }

    // Named duplicates: emit one error per duplicate (every occurrence after
    // the first).
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for op in operations {
        if let Some(n) = op.name {
            if !seen.insert(n.value) {
                errors.push(ValidationError {
                    rule: RULE_UNIQUE_OPERATION_NAMES,
                    span: n.span,
                    message: "operation name is reused; operation names must be unique",
                });
            }
        }
    }
}

// --- NoUndefinedVariables / NoUnusedVariables ---------------------------

fn check_variables_for_operation<'src, 'bump>(
    op: &OperationDefinition<'src, 'bump>,
    fragments_by_name: &BTreeMap<&'src str, &FragmentDefinition<'src, 'bump>>,
    errors: &mut Vec<ValidationError>,
) {
    // Declared variables (preserve declaration order via Vec; lookup via
    // BTreeMap from name -> span for the error report).
    let mut declared_order: Vec<&'src str> = Vec::new();
    let mut declared_spans: BTreeMap<&'src str, Span> = BTreeMap::new();
    for vd in &op.variable_definitions {
        if declared_spans.insert(vd.name.value, vd.name.span).is_none() {
            declared_order.push(vd.name.value);
        }
    }

    // Walk the operation body + transitively-spread fragment bodies,
    // collecting every variable reference and the span where it appeared.
    let mut used: BTreeMap<&'src str, Span> = BTreeMap::new();
    let mut visited_frags: BTreeSet<&'src str> = BTreeSet::new();
    collect_variable_uses_in_directives(&op.directives, &mut used);
    collect_variable_uses_in_selection_set(
        &op.selection_set,
        fragments_by_name,
        &mut visited_frags,
        &mut used,
    );
    // Default values can also reference variables (rare but legal grammar
    // before validation; we err on the side of detecting them too).
    for vd in &op.variable_definitions {
        if let Some(dv) = &vd.default_value {
            collect_variable_uses_in_value(dv, &mut used);
        }
        for d in &vd.directives {
            collect_variable_uses_in_directive(d, &mut used);
        }
    }

    // NoUndefinedVariables: every used name must be declared.
    for (name, span) in &used {
        if !declared_spans.contains_key(name) {
            errors.push(ValidationError {
                rule: RULE_NO_UNDEFINED_VARIABLES,
                span: *span,
                message: "variable is used but not declared in the operation",
            });
        }
    }

    // NoUnusedVariables: every declared name must be used.
    for name in &declared_order {
        if !used.contains_key(name) {
            let span = declared_spans[name];
            errors.push(ValidationError {
                rule: RULE_NO_UNUSED_VARIABLES,
                span,
                message: "variable is declared but never used",
            });
        }
    }
}

fn collect_variable_uses_in_selection_set<'src, 'bump>(
    selection_set: &SelectionSet<'src, 'bump>,
    fragments_by_name: &BTreeMap<&'src str, &FragmentDefinition<'src, 'bump>>,
    visited_frags: &mut BTreeSet<&'src str>,
    out: &mut BTreeMap<&'src str, Span>,
) {
    for sel in &selection_set.selections {
        match sel {
            Selection::Field(f) => {
                for arg in &f.arguments {
                    collect_variable_uses_in_argument(arg, out);
                }
                for d in &f.directives {
                    collect_variable_uses_in_directive(d, out);
                }
                if let Some(inner) = &f.selection_set {
                    collect_variable_uses_in_selection_set(
                        inner,
                        fragments_by_name,
                        visited_frags,
                        out,
                    );
                }
            }
            Selection::FragmentSpread(FragmentSpread { name, directives }) => {
                for d in directives {
                    collect_variable_uses_in_directive(d, out);
                }
                // Recurse into the referenced fragment body (but only once
                // per fragment to avoid cycles — invalid cycles are a
                // separate validation rule we don't ship in R4).
                if visited_frags.insert(name.value) {
                    if let Some(frag) = fragments_by_name.get(name.value) {
                        collect_variable_uses_in_directives(&frag.directives, out);
                        collect_variable_uses_in_selection_set(
                            &frag.selection_set,
                            fragments_by_name,
                            visited_frags,
                            out,
                        );
                    }
                }
            }
            Selection::InlineFragment(InlineFragment {
                directives,
                selection_set,
                ..
            }) => {
                collect_variable_uses_in_directives(directives, out);
                collect_variable_uses_in_selection_set(
                    selection_set,
                    fragments_by_name,
                    visited_frags,
                    out,
                );
            }
        }
    }
}

fn collect_variable_uses_in_directives<'src, 'bump>(
    directives: &[Directive<'src, 'bump>],
    out: &mut BTreeMap<&'src str, Span>,
) {
    for d in directives {
        collect_variable_uses_in_directive(d, out);
    }
}

fn collect_variable_uses_in_directive<'src, 'bump>(
    d: &Directive<'src, 'bump>,
    out: &mut BTreeMap<&'src str, Span>,
) {
    for a in &d.arguments {
        collect_variable_uses_in_argument(a, out);
    }
}

fn collect_variable_uses_in_argument<'src, 'bump>(
    a: &Argument<'src, 'bump>,
    out: &mut BTreeMap<&'src str, Span>,
) {
    collect_variable_uses_in_value(&a.value, out);
}

fn collect_variable_uses_in_value<'src, 'bump>(
    v: &Value<'src, 'bump>,
    out: &mut BTreeMap<&'src str, Span>,
) {
    match v {
        Value::Variable(name) => {
            // Record only the FIRST occurrence (BTreeMap preserves it via
            // insert-only semantics). This makes error spans deterministic.
            out.entry(name.value).or_insert(name.span);
        }
        Value::List(items) => {
            for item in items {
                collect_variable_uses_in_value(item, out);
            }
        }
        Value::Object(fields) => {
            for ObjectField { value, .. } in fields {
                collect_variable_uses_in_value(value, out);
            }
        }
        // Scalars / Null / Enum / String / Bool / Int / Float — no vars.
        _ => {}
    }
}

// --- KnownFragmentNames / NoUnusedFragments -----------------------------

fn walk_fragment_spreads<'src, 'bump>(
    selection_set: &SelectionSet<'src, 'bump>,
    fragments_by_name: &BTreeMap<&'src str, &FragmentDefinition<'src, 'bump>>,
    use_counts: &mut BTreeMap<&'src str, usize>,
    errors: &mut Vec<ValidationError>,
) {
    for sel in &selection_set.selections {
        match sel {
            Selection::Field(f) => {
                if let Some(inner) = &f.selection_set {
                    walk_fragment_spreads(inner, fragments_by_name, use_counts, errors);
                }
            }
            Selection::FragmentSpread(spread) => {
                let name = spread.name.value;
                if fragments_by_name.contains_key(name) {
                    *use_counts.entry(name).or_insert(0) += 1;
                } else {
                    errors.push(ValidationError {
                        rule: RULE_KNOWN_FRAGMENT_NAMES,
                        span: spread.name.span,
                        message: "fragment spread references an undefined fragment",
                    });
                }
            }
            Selection::InlineFragment(inl) => {
                walk_fragment_spreads(&inl.selection_set, fragments_by_name, use_counts, errors);
            }
        }
    }
}
