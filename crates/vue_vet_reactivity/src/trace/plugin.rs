//! Compile-time tracer plugins — ecosystem API bag contracts.
//!
//! The reactivity **engine** does not hardcode Nuxt / vue-i18n surfaces. Plugins
//! (see the `vue_vet_plugins` crate) supply [`NamedApiBag`] rows; seed and read
//! paths stay generic.
//!
//! This is **not** a dynamic JS plugin host or AST Traverse. Plugins are Rust
//! static data registered at the analysis boundary (CLI / Oxc / session).

use vue_vet_core::ReactiveBindingKind;

/// One named API bag contract (e.g. `useI18n`, `useAsyncData`).
///
/// - [`Self::fields`] via [`Self::field_kind`]: object-destructure seeds
/// - [`Self::ambient_methods`]: locals that inject [`Self::ambient_fields`] on call
#[derive(Clone, Copy, Debug)]
pub struct NamedApiBag {
  /// Canonical callee name (`useI18n`, `useAsyncData`, …).
  pub callee: &'static str,
  /// Per-field reactive kind for object destructure (`data` → Ref, …).
  pub field_kind: fn(&str) -> Option<ReactiveBindingKind>,
  /// Destructured methods whose call tracks [`Self::ambient_fields`].
  pub ambient_methods: &'static [&'static str],
  /// Ambient field names tracked when an ambient method runs.
  pub ambient_fields: &'static [&'static str],
}

impl NamedApiBag {
  /// Look up a field kind from this bag's contract.
  #[must_use]
  pub fn field_kind_of(self, field: &str) -> Option<ReactiveBindingKind> {
    (self.field_kind)(field)
  }
}

/// Compile-time ecosystem extension for the reactivity tracer.
///
/// Implement in a plugin crate and register via [`TraceConfig::named_api_bags`]
/// (flattened) or [`flatten_named_api_bags`].
pub trait TracerPlugin: Send + Sync {
  /// Stable plugin id for diagnostics / cache identity (`vue-i18n`, `nuxt-data`).
  fn id(&self) -> &'static str;

  /// Named API bag contracts this plugin contributes.
  fn named_api_bags(&self) -> &'static [NamedApiBag];
}

/// Per-trace configuration. Ecosystem bags come from plugins — not the engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct TraceConfig<'a> {
  /// Flattened named API bag catalog from registered plugins.
  pub named_api_bags: &'a [NamedApiBag],
}

impl TraceConfig<'_> {
  /// Empty catalog: pure Vue primitives only (no Nuxt / vue-i18n bags).
  #[must_use]
  pub const fn empty() -> Self {
    Self { named_api_bags: &[] }
  }
}

/// Flatten plugin contributions into a single owned catalog (stable callee order).
#[must_use]
pub fn flatten_named_api_bags(plugins: &[&dyn TracerPlugin]) -> Vec<NamedApiBag> {
  let mut bags = Vec::new();
  for plugin in plugins {
    bags.extend_from_slice(plugin.named_api_bags());
  }
  bags.sort_by_key(|bag| bag.callee);
  bags
}

/// Look up a bag by callee name in a catalog.
#[must_use]
pub fn named_api_bag<'a>(catalog: &'a [NamedApiBag], callee: &str) -> Option<&'a NamedApiBag> {
  catalog.iter().find(|bag| bag.callee == callee)
}

/// Whether `name` is a known API-bag callee in the catalog (bare auto-import allowlist).
#[must_use]
pub fn is_named_api_bag_callee(catalog: &[NamedApiBag], name: &str) -> bool {
  catalog.iter().any(|bag| bag.callee == name)
}
