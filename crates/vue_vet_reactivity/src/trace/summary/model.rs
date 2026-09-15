//! Module semantic IR data model: [`ModuleSummary`] and the A6 [`ExportState`] lattice values.
//!
//! Pure data — no Oxc walks. Lattice rules live in [`super::export_lattice`].

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use oxc_span::Span;
use vue_vet_core::{ModuleId, ReactiveBindingKind, ReactivityGraph};

use super::{export_lattice, options_callback, typed_callback};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ImportSummary {
  pub(super) local: String,
  pub(super) imported: String,
  pub(super) source: String,
  pub(super) span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExportSummary {
  Local { local: String, exported: String },
  Reexport { source: String, imported: String, exported: String },
  Star { source: String },
}

/// Synthetic [`ModuleLink`] specifier prefix for bare Nuxt auto-import calls.
///
/// Kept in sync with `vue_vet_project::conventions::NUXT_IMPORTS_SPECIFIER_PREFIX`.
pub(super) const NUXT_IMPORTS_SPECIFIER_PREFIX: &str = "#nuxt-imports:";
/// Exclusive end for [`BTreeMap::range`] over `#nuxt-imports:…` keys (`';` follows `:`).
pub(super) const NUXT_IMPORTS_RANGE_END: &str = "#nuxt-imports;";

/// `return { isLoading }` where `isLoading` came from a call destructure.
///
/// - Non-empty [`Self::path`]: `const { field } = root.a.b()` → link-time
///   [`ExportState::ValueBag`] walk.
/// - Empty path: `const { field } = useX()` → link-time
///   [`ExportState::Composable`] field lookup on `root` (bare or imported).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingValueBagField {
  pub root: String,
  pub path: Vec<String>,
  pub field: String,
}

/// Object-bag return shape for a composable (explicit fields + optional open spread).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComposableShape {
  pub fields: BTreeMap<String, ReactiveBindingKind>,
  /// `return { …, ...bag }` where `bag` is a proven reactive object surface
  /// (`bag.field.value` reads in the same function). Unknown destructured keys
  /// seed as [`ReactiveBindingKind::Ref`] (under-approx).
  pub open_reactive_spread: bool,
  /// Re-exported fields from value-bag member destructures (resolved at link).
  pub(crate) pending_value_bag_fields: BTreeMap<String, PendingValueBagField>,
}

impl ComposableShape {
  #[must_use]
  pub const fn from_fields(fields: BTreeMap<String, ReactiveBindingKind>) -> Self {
    Self { fields, open_reactive_spread: false, pending_value_bag_fields: BTreeMap::new() }
  }

  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.fields.is_empty() && !self.open_reactive_spread && self.pending_value_bag_fields.is_empty()
  }

  /// Kind for a destructured property; open spreads default unknown keys to Ref.
  #[must_use]
  pub fn kind_for_destructure(&self, key: &str) -> Option<ReactiveBindingKind> {
    self
      .fields
      .get(key)
      .copied()
      .or_else(|| self.open_reactive_spread.then_some(ReactiveBindingKind::Ref))
  }

  #[must_use]
  pub(crate) fn has_pending_value_bag_fields(&self) -> bool {
    !self.pending_value_bag_fields.is_empty()
  }
}

/// Nested object of callables / sub-bags (`createApi()` → `{ maps: { useX } }`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ValueBag {
  pub entries: BTreeMap<String, ValueBagEntry>,
}

impl ValueBag {
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  /// Walk a static member path to a leaf method shape / factory.
  #[must_use]
  pub fn resolve_path(&self, path: &[String]) -> Option<&ValueBagEntry> {
    let mut current = self;
    for (index, segment) in path.iter().enumerate() {
      let entry = current.entries.get(segment)?;
      if index + 1 == path.len() {
        return Some(entry);
      }
      match entry {
        ValueBagEntry::Nested(nested) => current = nested,
        ValueBagEntry::Method(_)
        | ValueBagEntry::MethodFactory(_)
        | ValueBagEntry::MethodForward(_)
        | ValueBagEntry::MethodGeneric(_) => return None,
      }
    }
    None
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueBagEntry {
  Nested(ValueBag),
  /// Property is a function that returns a composable object bag.
  Method(ComposableShape),
  /// Property is a function that returns a scalar reactive value.
  MethodFactory(ReactiveBindingKind),
  /// Property forwards to another local/import name — resolve at link time.
  MethodForward(String),
  /// Property returns `… as T` where `T` is the owning factory's type parameter
  /// at this index — instantiate at a typed call site.
  MethodGeneric(u8),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExportState {
  /// Imported local is itself a reactive binding (`import { count } from './x'`).
  Known(ReactiveBindingKind),
  /// Calling the export returns a statically keyed object bag.
  Composable(ComposableShape),
  /// Calling the export returns a scalar reactive value (`return ref(0)` / `(): Ref<T>`).
  Factory(ReactiveBindingKind),
  /// Calling the export returns a nested value bag of methods / sub-bags.
  ValueFactory(ValueBag),
  /// Binding is already a nested value bag (`const api = createApi()`).
  ValueBag(ValueBag),
  /// `const api = createApi()` where `createApi` is not yet a local
  /// [`ValueFactory`](ExportState::ValueFactory) (typically an import).
  /// Link-time refine → [`ExportState::ValueBag`].
  ValueFactoryCall(String),
  /// `const { useInject: useX } = createContext<Ctx>(…)` — property is
  /// [`ValueBagEntry::MethodGeneric`]; link-time checks the callee bag then
  /// publishes [`ExportState::Composable`] from the matching type argument.
  GenericMethodInstantiate {
    callee: String,
    property: String,
    type_arg_shapes: Vec<ComposableShape>,
  },
  /// Body is `return callee(...)` — resolve callee export at link time.
  ForwardReturn(String),
  /// Calling the export wraps `defineComponent` with the first argument as setup
  /// (cross-module typed helpers). Consumers seed the setup `props` parameter.
  ComponentFactory,
  /// Declared `() => PlainObject` (no Ref fields) — needs body evidence for Reactive factory.
  DeclaredPlainObjectFactory,
  /// Body unwraps a state ref (e.g. `return useState(...).value`) — needs plain-object declaration.
  BodyUnwrappedState,
  Ambiguous,
}

/// Under-approx classification of a composable/factory function return.
#[derive(Debug, Eq, PartialEq)]
pub enum ComposableReturn {
  Object(ComposableShape),
  ValueBag(ValueBag),
  Factory(ReactiveBindingKind),
  /// Body unwraps a state ref (`return useState(...).value`, unresolved / `#imports`).
  UnwrappedState,
  /// Sole return is `return callee(...)` to an unresolved local/import name.
  Forward(String),
  /// Sole return is `expr as T` where `T` is an enclosing type parameter (index).
  GenericParam(u8),
}

/// Declared TypeScript return surface for factory/composable exports.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum DeclaredReturn {
  Factory(ReactiveBindingKind),
  Composable(ComposableShape),
  /// Object-shaped type with ≥1 property and no Ref-like fields.
  PlainObject,
}

/// Export-resolution payload only — no source body, no owned reactivity graph.
/// Shares [`ModuleSummary`] across the seed barrier instead of cloning its vectors.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct ModuleExportFacts {
  pub(super) id: ModuleId,
  pub(super) summary: Arc<ModuleSummary>,
}

/// Stable module semantic IR extracted from an existing Oxc semantic.
///
/// Cross-file linking consumes this summary instead of parser ASTs. It is
/// intentionally not disk-serializable: callers retain it only for the current
/// analysis lifecycle, and Oxc nodes never cross the adapter boundary.
#[derive(Debug, Eq, PartialEq)]
pub struct ModuleSummary {
  pub(super) imports: Vec<ImportSummary>,
  pub(super) exports: Vec<ExportSummary>,
  pub(super) locals: BTreeMap<String, ExportState>,
  /// Named local/export → options-object callback bag shapes (from declared types).
  pub(super) options_callback_slots: BTreeMap<String, options_callback::OptionsCallbackSlots>,
  /// Named local/export → typed function-callback Ref formals (from declared types).
  pub(super) typed_callback_param_slots: BTreeMap<String, typed_callback::TypedCallbackParamSlots>,
  pub(super) provides: Vec<super::super::ProvideSite>,
  pub(super) injects: Vec<super::super::InjectSite>,
  /// Identifier callees seen during phase one (`foo()` / `foo<T>()`).
  ///
  /// Phase two uses this to skip a reparse when the seed plan is call-site-only
  /// (`Factory` / `Composable` / `ValueFactory` / callback slots) and none of
  /// those names are called. Not a linking-surface field — body edits that only
  /// change call sites must not miss the linking cache.
  pub(super) called_locals: BTreeSet<String>,
  pub(super) local_graph: std::sync::Arc<ReactivityGraph>,
}

impl ModuleSummary {
  /// Specifiers this module imports or re-exports (for external follow).
  #[must_use]
  pub fn follow_specifiers(&self) -> Vec<String> {
    let mut specifiers = BTreeSet::new();
    for import in &self.imports {
      specifiers.insert(import.source.clone());
    }
    for export in &self.exports {
      match export {
        ExportSummary::Reexport { source, .. } | ExportSummary::Star { source } => {
          specifiers.insert(source.clone());
        }
        ExportSummary::Local { .. } => {}
      }
    }
    specifiers.into_iter().collect()
  }

  /// Bare package sources this module re-exports (`export * from 'pkg'` /
  /// `export { x } from 'pkg'`). External summary follow uses these so a barrel
  /// entry like `@vueuse/core` can load `@vueuse/shared` and publish star exports.
  #[must_use]
  pub fn reexport_bare_package_sources(&self) -> BTreeSet<String> {
    let mut sources = BTreeSet::new();
    for export in &self.exports {
      let source = match export {
        ExportSummary::Reexport { source, .. } | ExportSummary::Star { source } => source.as_str(),
        ExportSummary::Local { .. } => continue,
      };
      if source.starts_with("./") || source.starts_with("../") || source.starts_with('#') {
        continue;
      }
      sources.insert(source.to_owned());
    }
    sources
  }

  /// Bare import sources that `typeof` forwards need (external follow may load them).
  ///
  /// Relative follows already cover same-package barrels; `typeof useY` aliases often
  /// point at another package (`import { useY } from 'pkg'`), which stays quiet unless
  /// listed here.
  #[must_use]
  pub fn typeof_forward_sources(&self) -> BTreeSet<String> {
    let mut sources = BTreeSet::new();
    for state in self.locals.values() {
      let ExportState::ForwardReturn(callee) = state else {
        continue;
      };
      for import in &self.imports {
        if import.local == *callee
          && !import.source.starts_with("./")
          && !import.source.starts_with("../")
        {
          sources.insert(import.source.clone());
        }
      }
    }
    sources
  }

  /// Whether a companion implementation file may still complete provisional seeds.
  ///
  /// Only provisional declaration/body halves need a merge. Do **not** treat "no
  /// finished Factory/Composable seeds" as incomplete — that would parse every
  /// unrelated package's companion `.js` (e.g. multi‑MB `typescript.js`).
  #[must_use]
  pub fn needs_implementation_merge(&self) -> bool {
    self.locals.values().any(|state| {
      matches!(state, ExportState::DeclaredPlainObjectFactory | ExportState::BodyUnwrappedState)
    })
  }

  /// Whether a size-capped companion body may still publish `ComponentFactory`.
  ///
  /// True when an exported local has no seedable state yet (typical `export declare
  /// function wrap(...)` in package `.d.ts`). Still requires a real body that
  /// forwards to `defineComponent` — never invent from the declaration alone.
  #[must_use]
  pub fn may_gain_component_factory_from_impl(&self) -> bool {
    self.exports.iter().any(|export| match export {
      ExportSummary::Local { local, .. } => !matches!(
        self.locals.get(local),
        Some(
          ExportState::ComponentFactory
            | ExportState::Factory(_)
            | ExportState::Composable(_)
            | ExportState::Known(_)
            | ExportState::ValueFactory(_)
            | ExportState::ValueBag(_)
        )
      ),
      ExportSummary::Reexport { .. } | ExportSummary::Star { .. } => false,
    })
  }

  /// Whether any local is a `ComponentFactory` setup-forward wrapper.
  #[must_use]
  pub fn has_component_factory_local(&self) -> bool {
    self.locals.values().any(|state| matches!(state, ExportState::ComponentFactory))
  }
}

/// Merge `.d.ts` declaration locals with companion implementation locals.
///
/// `DeclaredPlainObjectFactory` + `BodyUnwrappedState` → `Factory(Reactive)`.
/// Shares [`ModuleSummary::local_graph`] by `Arc` — does not deep-clone the graph.
#[must_use]
pub fn merge_declaration_implementation_summary(
  declaration: &ModuleSummary,
  implementation: &ModuleSummary,
) -> ModuleSummary {
  let mut merged = declaration.locals.clone();
  for (name, impl_state) in &implementation.locals {
    if let Some(next) =
      export_lattice::merge_declaration_implementation_local(merged.get(name), impl_state)
    {
      merged.insert(name.clone(), next);
    }
  }
  let mut options_callback_slots = declaration.options_callback_slots.clone();
  for (name, slots) in &implementation.options_callback_slots {
    options_callback_slots.insert(name.clone(), slots.clone());
  }
  let mut typed_callback_param_slots = declaration.typed_callback_param_slots.clone();
  for (name, slots) in &implementation.typed_callback_param_slots {
    typed_callback_param_slots.insert(name.clone(), slots.clone());
  }
  ModuleSummary {
    imports: declaration.imports.clone(),
    exports: declaration.exports.clone(),
    locals: merged,
    options_callback_slots,
    typed_callback_param_slots,
    provides: declaration.provides.clone(),
    injects: declaration.injects.clone(),
    called_locals: declaration
      .called_locals
      .union(&implementation.called_locals)
      .cloned()
      .collect(),
    local_graph: Arc::clone(&declaration.local_graph),
  }
}
