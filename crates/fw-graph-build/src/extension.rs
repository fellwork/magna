//! `SchemaExtension` — the extension point for injecting custom Query/Mutation
//! fields, type registrations, and resolver wiring into `build_schema`.
//!
//! This is the primary Tier 1 extension surface. External consumers (like
//! `fw-resolvers` inside fellwork-api, or future Magna users) implement this
//! trait to add domain-specific fields without modifying fw-graph-build.
//!
//! # Lifecycle
//!
//! - An extension is consumed by `build_schema` only — it is *not* live during
//!   request handling. Resolver closures the extension wires get cloned into
//!   field handlers and outlive the extension itself.
//! - Each `build_schema` invocation calls every extension's hooks in this order:
//!   1. `register_types` for every extension (in slice order)
//!   2. `extend_query` for every extension (in slice order)
//!   3. `extend_mutation` for every extension, but only if the schema has
//!      mutations enabled (any resource has insert/update/delete behavior)
//! - Extensions must be `Send + Sync`. The resulting `Schema` is `Send + Sync`
//!   and resolver closures may run concurrently across threads.
//! - Hot-reload (re-running `build_schema` on schema change) requires
//!   reconstructing extensions; `build_schema` does not retain references.
//!
//! # Relationship to `fw_graph_config::Plugin`
//!
//! `SchemaExtension` operates at the *schema phase* on live
//! `async-graphql::dynamic` types — it wires actual resolver closures.
//! `fw_graph_config::Plugin` operates at the *gather phase* (modifying
//! introspection output) and at a pre-async-graphql SDL-fragment level.
//! They layer: a single feature can use both.

use async_graphql::dynamic::{Field, Object, SchemaBuilder, Type};

/// Context passed to a [`SchemaExtension`] hook so it can register types and
/// add fields. Internals are private — all mutation is via methods.
///
/// Which methods are usable depends on the phase:
///
/// | Phase             | `register_type` | `query_field` | `mutation_field` |
/// |-------------------|:---------------:|:-------------:|:----------------:|
/// | `register_types`  | ✓               | panics        | panics           |
/// | `extend_query`    | ✓               | ✓             | panics           |
/// | `extend_mutation` | ✓               | panics        | ✓ (or no-op*)    |
///
/// \* `mutation_field` is a no-op if the schema has no mutations enabled
/// (no resource declares INSERT/UPDATE/DELETE behavior). Extensions can call
/// it unconditionally; it will silently drop the field.
pub struct ExtensionContext<'a> {
    /// Internal — the in-progress schema builder. Held as `Option` because
    /// `SchemaBuilder::register` consumes by value and returns `Self`; we
    /// take/replace under `&mut self` to keep the public API method-based.
    builder: Option<SchemaBuilder>,
    /// In-progress Query root. `Some` only during `extend_query`.
    query: Option<&'a mut Object>,
    /// In-progress Mutation root. `Some` only during `extend_mutation`.
    mutation: Option<&'a mut Object>,
}

impl<'a> ExtensionContext<'a> {
    /// Register a custom type definition with the schema. Accepts anything
    /// `SchemaBuilder::register` accepts (object types, input types, enums,
    /// scalars, interfaces, unions). Available in every phase.
    ///
    /// # Example
    /// ```ignore
    /// fn register_types(&self, ctx: &mut ExtensionContext<'_>) {
    ///     let my_type = Object::new("MyType").field(
    ///         Field::new("id", TypeRef::named_nn(TypeRef::ID), /* resolver */)
    ///     );
    ///     ctx.register_type(my_type);
    /// }
    /// ```
    pub fn register_type(&mut self, ty: impl Into<Type>) {
        let builder = self.take_builder("register_type");
        self.builder = Some(builder.register(ty));
    }

    /// Add a field to the Query root. Only valid inside `extend_query`.
    /// Panics if called from `register_types` or `extend_mutation`.
    ///
    /// # Example
    /// ```ignore
    /// fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
    ///     ctx.query_field(my_field_one());
    ///     ctx.query_field(my_field_two());
    /// }
    /// ```
    pub fn query_field(&mut self, field: Field) {
        let q = self.query.as_mut().expect(
            "ExtensionContext::query_field called outside extend_query phase — \
             move type registration to register_types or mutation fields to extend_mutation",
        );
        replace_with(*q, |obj| obj.field(field));
    }

    /// Add a field to the Mutation root. Only valid inside `extend_mutation`.
    /// Panics if called from `register_types` or `extend_query`.
    ///
    /// If the schema has no mutations enabled, `extend_mutation` itself is
    /// never invoked, so this method is unreachable in that case.
    pub fn mutation_field(&mut self, field: Field) {
        let m = self.mutation.as_mut().expect(
            "ExtensionContext::mutation_field called outside extend_mutation phase — \
             move type registration to register_types or query fields to extend_query",
        );
        replace_with(*m, |obj| obj.field(field));
    }

    /// Internal helper: take the builder out of the context, panicking with a
    /// phase-specific message if it was already consumed (which would only
    /// happen if internal invariants were violated).
    fn take_builder(&mut self, caller: &'static str) -> SchemaBuilder {
        self.builder.take().unwrap_or_else(|| {
            panic!(
                "ExtensionContext::{caller} called with no builder — \
                 internal invariant violated in fw-graph-build"
            )
        })
    }
}

/// Internal helper for the `mem::replace` dance on `&mut Object`.
/// `async_graphql::dynamic::Object::field` is by-value, returning `Self`; this
/// lets us chain through a mutable reference. Not part of the public API —
/// extension authors use `ctx.query_field` / `ctx.mutation_field` instead.
pub(crate) fn replace_with<F>(obj: &mut Object, f: F)
where
    F: FnOnce(Object) -> Object,
{
    let placeholder = Object::new("_ExtensionReplaceWithPlaceholder");
    let current = std::mem::replace(obj, placeholder);
    *obj = f(current);
}

/// Trait implemented by consumers to add custom schema content.
///
/// Called during `build_schema` in three phases: `register_types`, then
/// `extend_query`, then (if mutations are enabled) `extend_mutation`. Within
/// each phase, every extension's hook runs in slice order before the next
/// phase begins. This lets extensions safely reference each other's
/// registered types in their field signatures.
///
/// # Implementing hooks
///
/// ```ignore
/// use fw_graph_build::{ExtensionContext, SchemaExtension};
/// use async_graphql::dynamic::{Field, Object, TypeRef, FieldFuture};
///
/// struct MyExtension;
///
/// impl SchemaExtension for MyExtension {
///     fn name(&self) -> &str { "my-extension" }
///
///     fn register_types(&self, ctx: &mut ExtensionContext<'_>) {
///         let my_type = Object::new("MyType")
///             .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), /* ... */));
///         ctx.register_type(my_type);
///     }
///
///     fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
///         ctx.query_field(Field::new(
///             "myField",
///             TypeRef::named_nn("MyType"),
///             |_| FieldFuture::from_value(None),
///         ));
///     }
/// }
/// ```
pub trait SchemaExtension: Send + Sync {
    /// Human-readable name for logging and debugging.
    fn name(&self) -> &str;

    /// Register custom type definitions (object types, input types, enums,
    /// etc.) via [`ExtensionContext::register_type`]. Called before
    /// `extend_query` and `extend_mutation`. Calling
    /// [`ExtensionContext::query_field`] / [`ExtensionContext::mutation_field`]
    /// here panics — those belong in their respective phases.
    fn register_types(&self, _ctx: &mut ExtensionContext<'_>) {}

    /// Add fields to the Query root via [`ExtensionContext::query_field`].
    /// Called after every extension's `register_types`, so types registered
    /// by other extensions are referenceable in field type signatures.
    fn extend_query(&self, _ctx: &mut ExtensionContext<'_>) {}

    /// Add fields to the Mutation root via [`ExtensionContext::mutation_field`].
    /// Only invoked if the schema has at least one resource declaring INSERT,
    /// UPDATE, or DELETE behavior — otherwise an extension's
    /// `extend_mutation` hook is silently skipped.
    fn extend_mutation(&self, _ctx: &mut ExtensionContext<'_>) {}
}

/// Phase identifier used by [`run_extension_phase`].
#[derive(Clone, Copy)]
pub(crate) enum Phase {
    RegisterTypes,
    ExtendQuery,
    ExtendMutation,
}

/// Run a single extension phase across all extensions, returning the updated
/// `SchemaBuilder`. Centralizes the take/replace dance and the per-phase
/// `ExtensionContext` shape so the three phases share one implementation.
///
/// `query` and `mutation` are passed mutably so the phase runner can hand
/// them to `ExtensionContext` only for the phase that needs them — preserving
/// the per-phase API restrictions documented on `ExtensionContext`.
pub(crate) fn run_extension_phase(
    builder: SchemaBuilder,
    query: &mut Object,
    mutation: Option<&mut Object>,
    extensions: &[Box<dyn SchemaExtension>],
    phase: Phase,
) -> SchemaBuilder {
    // The mutation slot is shared across iterations via reborrowing below;
    // hold a single owning Option<&mut Object> here.
    let mut mutation_slot = mutation;
    let mut current_builder = Some(builder);

    for ext in extensions {
        // Reborrow query and mutation each iteration so the shorter borrow
        // ends with the ExtensionContext drop. This lets the phase loop
        // continue past extension N to extension N+1 with fresh borrows.
        let (ctx_query, ctx_mutation) = match phase {
            Phase::RegisterTypes => (None, None),
            Phase::ExtendQuery => (Some(&mut *query), None),
            Phase::ExtendMutation => (None, mutation_slot.as_deref_mut()),
        };

        // The per-iteration builder is always Some at this point because the
        // previous iteration put it back at end-of-loop.
        let mut ctx = ExtensionContext {
            builder: current_builder.take(),
            query: ctx_query,
            mutation: ctx_mutation,
        };

        match phase {
            Phase::RegisterTypes => ext.register_types(&mut ctx),
            Phase::ExtendQuery => ext.extend_query(&mut ctx),
            Phase::ExtendMutation => ext.extend_mutation(&mut ctx),
        }

        // Reclaim the builder. Extensions cannot legitimately leave the
        // builder in `None` — only `register_type` takes/replaces, always
        // restoring under &mut self.
        current_builder = Some(ctx.builder.take().unwrap_or_else(|| {
            panic!(
                "extension '{}' left ExtensionContext.builder == None — \
                 this is a bug in fw-graph-build's extension wiring",
                ext.name()
            )
        }));
    }

    current_builder.expect(
        "extension phase completed without restoring builder — \
         internal invariant violated",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, TypeRef};

    struct TestExtension;
    impl SchemaExtension for TestExtension {
        fn name(&self) -> &str {
            "test-extension"
        }
    }

    fn fresh_ctx<'a>(query: &'a mut Object) -> ExtensionContext<'a> {
        let builder = async_graphql::dynamic::Schema::build("Query", None, None);
        ExtensionContext {
            builder: Some(builder),
            query: Some(query),
            mutation: None,
        }
    }

    #[test]
    fn test_extension_has_name() {
        assert_eq!(TestExtension.name(), "test-extension");
    }

    #[test]
    fn test_default_hooks_are_no_op() {
        let ext = TestExtension;
        let mut query = Object::new("Query");
        // RegisterTypes phase: query/mutation are None.
        let builder = async_graphql::dynamic::Schema::build("Query", None, None);
        let mut ctx = ExtensionContext {
            builder: Some(builder),
            query: None,
            mutation: None,
        };
        ext.register_types(&mut ctx);
        // ExtendQuery phase: query is Some.
        let mut ctx2 = fresh_ctx(&mut query);
        ext.extend_query(&mut ctx2);
        ext.extend_mutation(&mut ctx2);
    }

    #[test]
    fn test_register_type_works() {
        struct TypeAdder;
        impl SchemaExtension for TypeAdder {
            fn name(&self) -> &str { "type-adder" }
            fn register_types(&self, ctx: &mut ExtensionContext<'_>) {
                ctx.register_type(Object::new("AddedType"));
            }
        }
        let builder = async_graphql::dynamic::Schema::build("Query", None, None);
        let mut ctx = ExtensionContext {
            builder: Some(builder),
            query: None,
            mutation: None,
        };
        TypeAdder.register_types(&mut ctx);
        assert!(ctx.builder.is_some(), "builder should be restored");
    }

    #[test]
    fn test_query_field_chains_multiple_fields() {
        struct MultiFielder;
        impl SchemaExtension for MultiFielder {
            fn name(&self) -> &str { "multi" }
            fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
                for name in ["alpha", "beta", "gamma"] {
                    ctx.query_field(Field::new(
                        name,
                        TypeRef::named_nn(TypeRef::STRING),
                        |_| FieldFuture::from_value(Some(async_graphql::Value::from("x"))),
                    ));
                }
            }
        }
        let mut query = Object::new("Query");
        let mut ctx = fresh_ctx(&mut query);
        MultiFielder.extend_query(&mut ctx);
        // The placeholder type name must never leak into the result.
        assert_eq!(query.type_name(), "Query");
    }

    #[test]
    #[should_panic(expected = "outside extend_query phase")]
    fn test_query_field_panics_outside_extend_query() {
        let builder = async_graphql::dynamic::Schema::build("Query", None, None);
        let mut ctx = ExtensionContext {
            builder: Some(builder),
            query: None,
            mutation: None,
        };
        ctx.query_field(Field::new(
            "x",
            TypeRef::named_nn(TypeRef::STRING),
            |_| FieldFuture::from_value(None),
        ));
    }

    #[test]
    #[should_panic(expected = "outside extend_mutation phase")]
    fn test_mutation_field_panics_outside_extend_mutation() {
        let builder = async_graphql::dynamic::Schema::build("Query", None, None);
        let mut ctx = ExtensionContext {
            builder: Some(builder),
            query: None,
            mutation: None,
        };
        ctx.mutation_field(Field::new(
            "x",
            TypeRef::named_nn(TypeRef::STRING),
            |_| FieldFuture::from_value(None),
        ));
    }

    #[test]
    fn test_replace_with_preserves_type_name() {
        let mut obj = Object::new("MyObject");
        replace_with(&mut obj, |o| {
            o.field(Field::new(
                "f",
                TypeRef::named_nn(TypeRef::STRING),
                |_| FieldFuture::from_value(None),
            ))
        });
        assert_eq!(obj.type_name(), "MyObject");
    }
}
