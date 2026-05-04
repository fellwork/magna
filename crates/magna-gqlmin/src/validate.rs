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

#[cfg(feature = "sdl")]
use crate::parse::{
    sdl::{
        DirectiveDef, EnumTypeDef, FieldDef, InputObjectTypeDef, InputValueDef, InterfaceTypeDef,
        ObjectTypeDef, ScalarTypeDef, SchemaDocument, TypeSystemDefinition, UnionTypeDef,
    },
    Type,
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

// ========================================================================
// Schema-aware validation rules (R11, step 9 completion)
//
// These five rules require an SDL schema. Gated on `sdl + validate`.
// ========================================================================

#[cfg(feature = "sdl")]
pub const RULE_KNOWN_TYPE_NAMES: &str = "KnownTypeNames";
#[cfg(feature = "sdl")]
pub const RULE_FIELDS_ON_CORRECT_TYPE: &str = "FieldsOnCorrectType";
#[cfg(feature = "sdl")]
pub const RULE_SCALAR_LEAFS: &str = "ScalarLeafs";
#[cfg(feature = "sdl")]
pub const RULE_KNOWN_ARGUMENT_NAMES: &str = "KnownArgumentNames";
#[cfg(feature = "sdl")]
pub const RULE_ARGUMENTS_OF_CORRECT_TYPE: &str = "ArgumentsOfCorrectType";

/// Run all ten validation rules: the five operations-only rules from
/// [`validate_operations`] plus five schema-aware rules.
///
/// Schema-aware rules: [`KnownTypeNames`], [`FieldsOnCorrectType`],
/// [`ScalarLeafs`], [`KnownArgumentNames`], [`ArgumentsOfCorrectType`].
///
/// [`KnownTypeNames`]: RULE_KNOWN_TYPE_NAMES
/// [`FieldsOnCorrectType`]: RULE_FIELDS_ON_CORRECT_TYPE
/// [`ScalarLeafs`]: RULE_SCALAR_LEAFS
/// [`KnownArgumentNames`]: RULE_KNOWN_ARGUMENT_NAMES
/// [`ArgumentsOfCorrectType`]: RULE_ARGUMENTS_OF_CORRECT_TYPE
#[cfg(feature = "sdl")]
pub fn validate<'src>(
    doc: &Document<'src>,
    schema: &SchemaDocument<'src>,
) -> Vec<ValidationError> {
    let mut errors = validate_operations(doc);
    let index = SchemaIndex::build(schema);
    schema_aware::run_all(doc, &index, &mut errors);
    errors
}

#[cfg(feature = "sdl")]
#[derive(Clone, Copy)]
enum TypeKind {
    Object,
    Interface,
    Union,
    Enum,
    InputObject,
    Scalar,
}

#[cfg(feature = "sdl")]
struct SchemaIndex<'a, 'src> {
    /// Maps type name → kind.
    kinds: BTreeMap<&'src str, TypeKind>,
    objects: BTreeMap<&'src str, &'a ObjectTypeDef<'src>>,
    interfaces: BTreeMap<&'src str, &'a InterfaceTypeDef<'src>>,
    unions: BTreeMap<&'src str, &'a UnionTypeDef<'src>>,
    enums: BTreeMap<&'src str, &'a EnumTypeDef<'src>>,
    inputs: BTreeMap<&'src str, &'a InputObjectTypeDef<'src>>,
    #[allow(dead_code)]
    scalars: BTreeMap<&'src str, &'a ScalarTypeDef<'src>>,
    /// Directive definitions, by name (without the leading `@`).
    directives: BTreeMap<&'src str, &'a DirectiveDef<'src>>,
    /// Root operation type names.
    query: Option<&'src str>,
    mutation: Option<&'src str>,
    subscription: Option<&'src str>,
}

#[cfg(feature = "sdl")]
impl<'a, 'src> SchemaIndex<'a, 'src> {
    fn build(schema: &'a SchemaDocument<'src>) -> Self {
        let mut kinds: BTreeMap<&'src str, TypeKind> = BTreeMap::new();
        let mut objects = BTreeMap::new();
        let mut interfaces = BTreeMap::new();
        let mut unions = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut inputs = BTreeMap::new();
        let mut scalars = BTreeMap::new();
        let mut directives = BTreeMap::new();
        let mut query: Option<&'src str> = None;
        let mut mutation: Option<&'src str> = None;
        let mut subscription: Option<&'src str> = None;

        // GraphQL built-in scalars are implicitly available even when not
        // declared in the SDL document.
        for &name in &["Int", "Float", "String", "Boolean", "ID"] {
            kinds.insert(name, TypeKind::Scalar);
        }

        for def in &schema.definitions {
            match def {
                TypeSystemDefinition::Schema(s) => {
                    for op in &s.operation_types {
                        let target = op.named_type.name.value;
                        match op.operation {
                            "query" => query = Some(target),
                            "mutation" => mutation = Some(target),
                            "subscription" => subscription = Some(target),
                            _ => {}
                        }
                    }
                }
                TypeSystemDefinition::Object(o) => {
                    kinds.insert(o.name.value, TypeKind::Object);
                    objects.insert(o.name.value, o);
                }
                TypeSystemDefinition::Interface(i) => {
                    kinds.insert(i.name.value, TypeKind::Interface);
                    interfaces.insert(i.name.value, i);
                }
                TypeSystemDefinition::Union(u) => {
                    kinds.insert(u.name.value, TypeKind::Union);
                    unions.insert(u.name.value, u);
                }
                TypeSystemDefinition::Enum(e) => {
                    kinds.insert(e.name.value, TypeKind::Enum);
                    enums.insert(e.name.value, e);
                }
                TypeSystemDefinition::InputObject(i) => {
                    kinds.insert(i.name.value, TypeKind::InputObject);
                    inputs.insert(i.name.value, i);
                }
                TypeSystemDefinition::Scalar(s) => {
                    kinds.insert(s.name.value, TypeKind::Scalar);
                    scalars.insert(s.name.value, s);
                }
                TypeSystemDefinition::Directive(d) => {
                    directives.insert(d.name.value, d);
                }
            }
        }

        // Defaults if no explicit `schema { ... }` block.
        if query.is_none() && objects.contains_key("Query") {
            query = Some("Query");
        }
        if mutation.is_none() && objects.contains_key("Mutation") {
            mutation = Some("Mutation");
        }
        if subscription.is_none() && objects.contains_key("Subscription") {
            subscription = Some("Subscription");
        }

        Self {
            kinds,
            objects,
            interfaces,
            unions,
            enums,
            inputs,
            scalars,
            directives,
            query,
            mutation,
            subscription,
        }
    }

    fn root_for(&self, kind: crate::parse::OperationKind) -> Option<&'src str> {
        match kind {
            crate::parse::OperationKind::Query => self.query,
            crate::parse::OperationKind::Mutation => self.mutation,
            crate::parse::OperationKind::Subscription => self.subscription,
        }
    }

    fn knows_type(&self, name: &str) -> bool {
        self.kinds.contains_key(name)
    }

    /// Return field defs for an Object/Interface, else None.
    fn fields_of<'b>(&'b self, type_name: &str) -> Option<&'b [FieldDef<'src>]> {
        if let Some(o) = self.objects.get(type_name) {
            return Some(&o.fields);
        }
        if let Some(i) = self.interfaces.get(type_name) {
            return Some(&i.fields);
        }
        None
    }

    fn lookup_field<'b>(&'b self, type_name: &str, field: &str) -> Option<&'b FieldDef<'src>> {
        self.fields_of(type_name)?.iter().find(|f| f.name.value == field)
    }

    /// Whether `type_name` is a leaf (scalar/enum).
    fn is_leaf(&self, type_name: &str) -> bool {
        matches!(self.kinds.get(type_name), Some(TypeKind::Scalar) | Some(TypeKind::Enum))
    }

    /// Whether `type_name` is composite (Object/Interface/Union).
    fn is_composite(&self, type_name: &str) -> bool {
        matches!(
            self.kinds.get(type_name),
            Some(TypeKind::Object) | Some(TypeKind::Interface) | Some(TypeKind::Union)
        )
    }
}

#[cfg(feature = "sdl")]
mod schema_aware {
    use super::*;
    use crate::parse::{Argument, Directive, FragmentDefinition, OperationKind};

    pub(super) fn run_all<'src>(
        doc: &Document<'src>,
        index: &SchemaIndex<'_, 'src>,
        errors: &mut Vec<ValidationError>,
    ) {
        // Build fragment lookup once for KnownTypeNames + FieldsOnCorrectType walks.
        let mut frags_by_name: BTreeMap<&'src str, &FragmentDefinition<'src>> = BTreeMap::new();
        for def in &doc.definitions() {
            if let Definition::Fragment(f) = def {
                frags_by_name.insert(f.name.value, f);
            }
        }

        // KnownTypeNames over operations + fragments.
        for def in &doc.definitions() {
            match def {
                Definition::Operation(op) => {
                    check_known_types_in_variable_defs(doc, op, index, errors);
                    check_known_types_in_selection_set(
                        doc,
                        op.selection_set.selections,
                        index,
                        errors,
                    );
                }
                Definition::Fragment(f) => {
                    check_named_type(&f.type_condition.name.value, f.type_condition.name.span, index, errors);
                    check_known_types_in_selection_set(
                        doc,
                        f.selection_set.selections,
                        index,
                        errors,
                    );
                }
            }
        }

        // FieldsOnCorrectType + ScalarLeafs + KnownArgumentNames + ArgumentsOfCorrectType.
        for def in &doc.definitions() {
            if let Definition::Operation(op) = def {
                let parent = index.root_for(op.kind);
                check_directives(doc, op.directives, index, errors);
                if let Some(parent) = parent {
                    walk_fields(
                        doc,
                        parent,
                        op.selection_set.selections,
                        index,
                        &frags_by_name,
                        errors,
                    );
                }
            }
        }
        for def in &doc.definitions() {
            if let Definition::Fragment(f) = def {
                let parent = f.type_condition.name.value;
                check_directives(doc, f.directives, index, errors);
                if index.knows_type(parent) {
                    walk_fields(
                        doc,
                        parent,
                        f.selection_set.selections,
                        index,
                        &frags_by_name,
                        errors,
                    );
                }
            }
        }
    }

    // --- KnownTypeNames -------------------------------------------------

    fn check_named_type(
        name: &str,
        span: Span,
        index: &SchemaIndex<'_, '_>,
        errors: &mut Vec<ValidationError>,
    ) {
        if !index.knows_type(name) {
            errors.push(ValidationError {
                rule: RULE_KNOWN_TYPE_NAMES,
                span,
                message: "type referenced is not defined in the schema",
            });
        }
    }

    fn check_type_ref<'src>(
        ty: &Type<'src>,
        index: &SchemaIndex<'_, 'src>,
        errors: &mut Vec<ValidationError>,
    ) {
        match ty {
            Type::Named(nt) => check_named_type(nt.name.value, nt.name.span, index, errors),
            Type::List(inner) => check_type_ref(inner, index, errors),
            Type::NonNull(inner) => check_type_ref(inner, index, errors),
        }
    }

    fn check_known_types_in_variable_defs<'src>(
        doc: &Document<'src>,
        op: &OperationDefinition<'src>,
        index: &SchemaIndex<'_, 'src>,
        errors: &mut Vec<ValidationError>,
    ) {
        for vd in &doc.variable_definitions(op.variable_definitions) {
            check_type_ref(&vd.var_type, index, errors);
        }
    }

    fn check_known_types_in_selection_set<'src>(
        doc: &Document<'src>,
        selections: NodeRange,
        index: &SchemaIndex<'_, 'src>,
        errors: &mut Vec<ValidationError>,
    ) {
        for sel in &doc.selections(selections) {
            match sel {
                Selection::Field(f) => {
                    if let Some(inner) = &f.selection_set {
                        check_known_types_in_selection_set(doc, inner.selections, index, errors);
                    }
                }
                Selection::InlineFragment(inl) => {
                    if let Some(tc) = &inl.type_condition {
                        check_named_type(tc.name.value, tc.name.span, index, errors);
                    }
                    check_known_types_in_selection_set(
                        doc,
                        inl.selection_set.selections,
                        index,
                        errors,
                    );
                }
                Selection::FragmentSpread(_) => {}
            }
        }
    }

    // --- FieldsOnCorrectType + ScalarLeafs + KnownArgumentNames +
    //     ArgumentsOfCorrectType (combined walker) ----------------------

    fn walk_fields<'src>(
        doc: &Document<'src>,
        parent_type: &'src str,
        selections: NodeRange,
        index: &SchemaIndex<'_, 'src>,
        frags_by_name: &BTreeMap<&'src str, &FragmentDefinition<'src>>,
        errors: &mut Vec<ValidationError>,
    ) {
        for sel in &doc.selections(selections) {
            match sel {
                Selection::Field(f) => {
                    let field_name = f.name.value;
                    // __typename is always valid on composite types.
                    if field_name == "__typename" {
                        continue;
                    }
                    let field_def = index.lookup_field(parent_type, field_name);
                    let field_def = match field_def {
                        Some(fd) => fd,
                        None => {
                            // Unions don't expose fields directly — use inline
                            // fragments to narrow. Anything else is an error.
                            errors.push(ValidationError {
                                rule: RULE_FIELDS_ON_CORRECT_TYPE,
                                span: f.name.span,
                                message: "field is not declared on the parent type",
                            });
                            continue;
                        }
                    };

                    // ScalarLeafs.
                    let unwrapped = unwrap_type_name(&field_def.field_type);
                    if let Some(tn) = unwrapped {
                        let leaf = index.is_leaf(tn);
                        let composite = index.is_composite(tn);
                        let has_set = f.selection_set.is_some();
                        if leaf && has_set {
                            errors.push(ValidationError {
                                rule: RULE_SCALAR_LEAFS,
                                span: f.name.span,
                                message: "leaf field must not have a selection set",
                            });
                        }
                        if composite && !has_set {
                            errors.push(ValidationError {
                                rule: RULE_SCALAR_LEAFS,
                                span: f.name.span,
                                message: "field of composite type must have a selection set",
                            });
                        }
                    }

                    // KnownArgumentNames + ArgumentsOfCorrectType (field args).
                    check_field_args(doc, f.arguments, field_def, index, errors);

                    // Field-level directives.
                    check_directives(doc, f.directives, index, errors);

                    // Recurse if the field has a selection set and the
                    // unwrapped type is composite.
                    if let (Some(inner), Some(tn)) = (&f.selection_set, unwrapped) {
                        if index.is_composite(tn) {
                            walk_fields(doc, tn, inner.selections, index, frags_by_name, errors);
                        }
                    }
                }
                Selection::InlineFragment(inl) => {
                    let next_parent = match &inl.type_condition {
                        Some(tc) => tc.name.value,
                        None => parent_type,
                    };
                    check_directives(doc, inl.directives, index, errors);
                    if index.knows_type(next_parent) {
                        walk_fields(
                            doc,
                            next_parent,
                            inl.selection_set.selections,
                            index,
                            frags_by_name,
                            errors,
                        );
                    }
                }
                Selection::FragmentSpread(fs) => {
                    check_directives(doc, fs.directives, index, errors);
                    // Don't recurse into fragment definitions here — they're
                    // walked at the top level using their own type
                    // condition.
                    let _ = frags_by_name;
                }
            }
        }
    }

    /// Strip `NonNull`/`List` wrappers and return the innermost Named name
    /// (or `None` if the chain is malformed).
    fn unwrap_type_name<'src>(ty: &Type<'src>) -> Option<&'src str> {
        match ty {
            Type::Named(nt) => Some(nt.name.value),
            Type::List(inner) => unwrap_type_name(inner),
            Type::NonNull(inner) => unwrap_type_name(inner),
        }
    }

    fn check_field_args<'src>(
        doc: &Document<'src>,
        args: NodeRange,
        field_def: &FieldDef<'src>,
        index: &SchemaIndex<'_, 'src>,
        errors: &mut Vec<ValidationError>,
    ) {
        for arg in &doc.arguments(args) {
            let arg_def = field_def.arguments.iter().find(|a| a.name.value == arg.name.value);
            match arg_def {
                None => errors.push(ValidationError {
                    rule: RULE_KNOWN_ARGUMENT_NAMES,
                    span: arg.name.span,
                    message: "argument is not declared on the field",
                }),
                Some(a) => {
                    if !value_compatible(&arg.value, &a.value_type, index) {
                        errors.push(ValidationError {
                            rule: RULE_ARGUMENTS_OF_CORRECT_TYPE,
                            span: arg.name.span,
                            message: "argument value does not match the declared type",
                        });
                    }
                }
            }
        }
    }

    fn check_directives<'src>(
        doc: &Document<'src>,
        directives: NodeRange,
        index: &SchemaIndex<'_, 'src>,
        errors: &mut Vec<ValidationError>,
    ) {
        for d in &doc.directives(directives) {
            let dir_def = match index.directives.get(d.name.value) {
                Some(dd) => *dd,
                None => {
                    // Unknown directives — `KnownDirectives` is a separate
                    // rule (out of scope this round). We still need to
                    // walk arg values for KnownArgumentNames, but without
                    // a directive def we can't check arg names. Accept
                    // arguments silently.
                    continue;
                }
            };
            check_directive_args(doc, d, dir_def, index, errors);
        }
    }

    fn check_directive_args<'src>(
        doc: &Document<'src>,
        d: &Directive<'src>,
        dir_def: &DirectiveDef<'src>,
        index: &SchemaIndex<'_, 'src>,
        errors: &mut Vec<ValidationError>,
    ) {
        for arg in &doc.arguments(d.arguments) {
            let _: &Argument<'_> = arg; // explicit type annotation
            let arg_def: Option<&InputValueDef<'src>> = dir_def
                .arguments
                .iter()
                .find(|a| a.name.value == arg.name.value);
            match arg_def {
                None => errors.push(ValidationError {
                    rule: RULE_KNOWN_ARGUMENT_NAMES,
                    span: arg.name.span,
                    message: "argument is not declared on the directive",
                }),
                Some(a) => {
                    if !value_compatible(&arg.value, &a.value_type, index) {
                        errors.push(ValidationError {
                            rule: RULE_ARGUMENTS_OF_CORRECT_TYPE,
                            span: arg.name.span,
                            message: "directive argument value does not match the declared type",
                        });
                    }
                }
            }
        }
    }

    /// Coarse-grained literal-vs-type compatibility check.
    ///
    /// Variable references skip the check (we don't track variable types
    /// in this round). The check is intentionally lenient: scalar names
    /// other than the five built-ins accept any literal-of-string-or-
    /// equivalent kind.
    fn value_compatible<'src>(
        v: &Value<'src>,
        ty: &Type<'src>,
        index: &SchemaIndex<'_, 'src>,
    ) -> bool {
        // Variable: assume compatible (full check requires var-type
        // resolution; tracked under the standalone VariablesInAllowedPosition
        // rule which is out of scope this round).
        if matches!(v, Value::Variable(_)) {
            return true;
        }
        // NonNull: literal must not be Null; otherwise check inner.
        if let Type::NonNull(inner) = ty {
            if matches!(v, Value::Null) {
                return false;
            }
            return value_compatible(v, inner, index);
        }
        // Null is allowed for any nullable type.
        if matches!(v, Value::Null) {
            return true;
        }
        // List: a list literal must match element type; scalar literal
        // coerces to a 1-element list.
        if let Type::List(inner) = ty {
            return match v {
                Value::List(_) => true, // shallow check; deep check would
                                        // walk Document::list_values
                _ => value_compatible(v, inner, index),
            };
        }
        // Named: dispatch on kind.
        let Type::Named(nt) = ty else {
            return true;
        };
        let name = nt.name.value;
        match index.kinds.get(name).copied() {
            None => true, // unknown type — KnownTypeNames already flagged
            Some(TypeKind::Scalar) => match name {
                "Int" => matches!(v, Value::Int(_)),
                "Float" => matches!(v, Value::Float(_) | Value::Int(_)),
                "Boolean" => matches!(v, Value::Boolean(_)),
                "String" | "ID" => matches!(v, Value::String(_) | Value::Int(_)),
                _ => true, // custom scalar — accept any literal kind
            },
            Some(TypeKind::Enum) => matches!(v, Value::Enum(_)),
            Some(TypeKind::InputObject) => matches!(v, Value::Object(_)),
            // Composite types (Object/Interface/Union) cannot be input
            // values at all; the schema itself is malformed if a field/
            // directive arg declares one. Reject.
            Some(TypeKind::Object) | Some(TypeKind::Interface) | Some(TypeKind::Union) => false,
        }
    }

    // Silence unused-import warnings on this branch in some feature combos.
    #[allow(dead_code)]
    fn _touch_unused(_: &ObjectField<'_>) {}
    #[allow(dead_code)]
    fn _touch_kind(_: OperationKind) {}
}
