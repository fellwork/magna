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
//! The full schema-aware rule set is deferred to R6.
//!
//! Gated behind the `validate` feature which implies `std`. The validator
//! is host-side tooling — it is intentionally not built into the wasm
//! runtime.
//!
//! ## R5 (span-indexed AST)
//!
//! AST list fields are `NodeRange` slices into `Document::nodes`. The
//! validator threads `&Document` through every walker to project the
//! correct typed slice via `Document::directives(...)`,
//! `Document::selections(...)`, etc.

use std::collections::{BTreeMap, BTreeSet};
use std::vec::Vec;

use crate::lex::Span;
use crate::parse::{
    Definition, Document, FragmentDefinition, NodeRange, ObjectField, OperationDefinition,
    Selection, Value,
};

/// A validation finding, returned by [`validate_operations`].
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
pub fn validate_operations<'src>(doc: &Document<'src>) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let mut fragments_by_name: BTreeMap<&str, &FragmentDefinition<'src>> = BTreeMap::new();
    let mut fragment_use_counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut fragment_defs_in_order: Vec<&FragmentDefinition<'src>> = Vec::new();
    let mut operations: Vec<&OperationDefinition<'src>> = Vec::new();

    for def in &doc.definitions() {
        match def {
            Definition::Operation(op) => operations.push(op),
            Definition::Fragment(frag) => {
                fragments_by_name.insert(frag.name.value, frag);
                fragment_use_counts.insert(frag.name.value, 0);
                fragment_defs_in_order.push(frag);
            }
        }
    }

    check_unique_operation_names(&operations, &mut errors);

    for op in &operations {
        check_variables_for_operation(doc, op, &fragments_by_name, &mut errors);
    }

    for op in &operations {
        walk_fragment_spreads(
            doc,
            op.selection_set.selections,
            &fragments_by_name,
            &mut fragment_use_counts,
            &mut errors,
        );
    }
    for frag in &fragment_defs_in_order {
        walk_fragment_spreads(
            doc,
            frag.selection_set.selections,
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

fn check_unique_operation_names<'src>(
    operations: &[&OperationDefinition<'src>],
    errors: &mut Vec<ValidationError>,
) {
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

fn check_variables_for_operation<'src>(
    doc: &Document<'src>,
    op: &OperationDefinition<'src>,
    fragments_by_name: &BTreeMap<&'src str, &FragmentDefinition<'src>>,
    errors: &mut Vec<ValidationError>,
) {
    let mut declared_order: Vec<&'src str> = Vec::new();
    let mut declared_spans: BTreeMap<&'src str, Span> = BTreeMap::new();
    for vd in &doc.variable_definitions(op.variable_definitions) {
        if declared_spans.insert(vd.name.value, vd.name.span).is_none() {
            declared_order.push(vd.name.value);
        }
    }

    let mut used: BTreeMap<&'src str, Span> = BTreeMap::new();
    let mut visited_frags: BTreeSet<&'src str> = BTreeSet::new();
    collect_uses_in_directives(doc, op.directives, &mut used);
    collect_uses_in_selection_set(
        doc,
        op.selection_set.selections,
        fragments_by_name,
        &mut visited_frags,
        &mut used,
    );
    for vd in &doc.variable_definitions(op.variable_definitions) {
        if let Some(dv) = &vd.default_value {
            collect_uses_in_value(doc, dv, &mut used);
        }
        collect_uses_in_directives(doc, vd.directives, &mut used);
    }

    for (name, span) in &used {
        if !declared_spans.contains_key(name) {
            errors.push(ValidationError {
                rule: RULE_NO_UNDEFINED_VARIABLES,
                span: *span,
                message: "variable is used but not declared in the operation",
            });
        }
    }

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

fn collect_uses_in_selection_set<'src>(
    doc: &Document<'src>,
    selections: NodeRange,
    fragments_by_name: &BTreeMap<&'src str, &FragmentDefinition<'src>>,
    visited_frags: &mut BTreeSet<&'src str>,
    out: &mut BTreeMap<&'src str, Span>,
) {
    for sel in &doc.selections(selections) {
        match sel {
            Selection::Field(f) => {
                for arg in &doc.arguments(f.arguments) {
                    collect_uses_in_value(doc, &arg.value, out);
                }
                collect_uses_in_directives(doc, f.directives, out);
                if let Some(inner) = &f.selection_set {
                    collect_uses_in_selection_set(
                        doc,
                        inner.selections,
                        fragments_by_name,
                        visited_frags,
                        out,
                    );
                }
            }
            Selection::FragmentSpread(fs) => {
                collect_uses_in_directives(doc, fs.directives, out);
                if visited_frags.insert(fs.name.value) {
                    if let Some(frag) = fragments_by_name.get(fs.name.value) {
                        collect_uses_in_directives(doc, frag.directives, out);
                        collect_uses_in_selection_set(
                            doc,
                            frag.selection_set.selections,
                            fragments_by_name,
                            visited_frags,
                            out,
                        );
                    }
                }
            }
            Selection::InlineFragment(inl) => {
                collect_uses_in_directives(doc, inl.directives, out);
                collect_uses_in_selection_set(
                    doc,
                    inl.selection_set.selections,
                    fragments_by_name,
                    visited_frags,
                    out,
                );
            }
        }
    }
}

fn collect_uses_in_directives<'src>(
    doc: &Document<'src>,
    directives: NodeRange,
    out: &mut BTreeMap<&'src str, Span>,
) {
    for d in &doc.directives(directives) {
        for a in &doc.arguments(d.arguments) {
            collect_uses_in_value(doc, &a.value, out);
        }
    }
}

fn collect_uses_in_value<'src>(
    doc: &Document<'src>,
    v: &Value<'src>,
    out: &mut BTreeMap<&'src str, Span>,
) {
    match v {
        Value::Variable(name) => {
            out.entry(name.value).or_insert(name.span);
        }
        Value::List(items) => {
            for item in &doc.list_values(*items) {
                collect_uses_in_value(doc, item, out);
            }
        }
        Value::Object(fields) => {
            for ObjectField { value, .. } in &doc.object_fields(*fields) {
                collect_uses_in_value(doc, value, out);
            }
        }
        _ => {}
    }
}

// --- KnownFragmentNames / NoUnusedFragments -----------------------------

fn walk_fragment_spreads<'src>(
    doc: &Document<'src>,
    selections: NodeRange,
    fragments_by_name: &BTreeMap<&'src str, &FragmentDefinition<'src>>,
    use_counts: &mut BTreeMap<&'src str, usize>,
    errors: &mut Vec<ValidationError>,
) {
    for sel in &doc.selections(selections) {
        match sel {
            Selection::Field(f) => {
                if let Some(inner) = &f.selection_set {
                    walk_fragment_spreads(
                        doc,
                        inner.selections,
                        fragments_by_name,
                        use_counts,
                        errors,
                    );
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
                walk_fragment_spreads(
                    doc,
                    inl.selection_set.selections,
                    fragments_by_name,
                    use_counts,
                    errors,
                );
            }
        }
    }
}
