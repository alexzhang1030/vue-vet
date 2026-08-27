//! Compile-time ecosystem plugins for the Vue Vet reactivity tracer.
//!
//! Hardcoded Nuxt / vue-i18n (and future) API bag contracts live here — not in
//! `vue_vet_reactivity`. The engine only consumes [`NamedApiBag`] rows.
//!
//! Not a dynamic JS plugin host. The Vue Vet analysis boundary (Oxc / project /
//! session) installs [`default_named_api_bags`] automatically.

pub(crate) mod nuxt;
pub(crate) mod vue_i18n;

use vue_vet_reactivity::{NamedApiBag, TraceConfig, TraceModulesOptions, TracerPlugin};

pub use nuxt::NuxtDataPlugin;
pub use vue_i18n::VueI18nPlugin;

/// Built-in ecosystem plugins shipped with Vue Vet.
#[must_use]
pub fn default_plugins() -> &'static [&'static dyn TracerPlugin] {
  &[&VueI18nPlugin, &NuxtDataPlugin]
}

/// Flattened named API bag catalog for the default plugin set (callee-sorted).
///
/// Prefer this (or [`default_trace_config`] / [`default_trace_modules_options`])
/// at the analysis boundary so every scan sees the same bags.
#[must_use]
pub fn default_named_api_bags() -> &'static [NamedApiBag] {
  // Callee-sorted: useAsyncData, useFetch, useI18n, useLazyAsyncData, useLazyFetch.
  static BAGS: &[NamedApiBag] = &[
    nuxt::ASYNC_DATA_BAG,
    nuxt::FETCH_BAG,
    vue_i18n::USE_I18N_BAG,
    nuxt::LAZY_ASYNC_DATA_BAG,
    nuxt::LAZY_FETCH_BAG,
  ];
  BAGS
}

/// [`TraceConfig`] with the default ecosystem plugin catalog installed.
#[must_use]
pub fn default_trace_config() -> TraceConfig<'static> {
  TraceConfig { named_api_bags: default_named_api_bags() }
}

/// [`TraceModulesOptions`] with the default ecosystem plugin catalog installed.
///
/// Other fields use [`TraceModulesOptions::default`] (worker count, pool reuse, …).
#[must_use]
pub fn default_trace_modules_options() -> TraceModulesOptions {
  TraceModulesOptions { named_api_bags: default_named_api_bags().to_vec(), ..Default::default() }
}

/// Fill an empty `named_api_bags` catalog in place.
pub fn ensure_default_plugins_mut(options: &mut TraceModulesOptions) {
  if options.named_api_bags.is_empty() {
    options.named_api_bags = default_named_api_bags().to_vec();
  }
}

/// Ensure `options` carries the default plugin catalog when none were set.
///
/// Used by outer analysis entry points so callers get Nuxt / vue-i18n modeling
/// without constructing the catalog by hand.
#[must_use]
pub fn ensure_default_plugins(mut options: TraceModulesOptions) -> TraceModulesOptions {
  ensure_default_plugins_mut(&mut options);
  options
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_reactivity::flatten_named_api_bags;

  #[test]
  fn default_catalog_matches_flatten() {
    let flattened = flatten_named_api_bags(default_plugins());
    let defaults = default_named_api_bags();
    assert_eq!(flattened.len(), defaults.len());
    for (left, right) in flattened.iter().zip(defaults.iter()) {
      assert_eq!(left.callee, right.callee);
    }
  }

  #[test]
  fn plugin_ids_are_stable() {
    let ids: Vec<_> = default_plugins().iter().map(|p| p.id()).collect();
    assert!(ids.contains(&"vue-i18n"));
    assert!(ids.contains(&"nuxt-data"));
  }

  #[test]
  fn ensure_default_plugins_fills_empty() {
    let options = ensure_default_plugins(TraceModulesOptions::default());
    assert!(!options.named_api_bags.is_empty());
    assert_eq!(options.named_api_bags.len(), default_named_api_bags().len());
  }

  #[test]
  fn ensure_default_plugins_preserves_custom() {
    let options = ensure_default_plugins(TraceModulesOptions {
      named_api_bags: vec![vue_i18n::USE_I18N_BAG],
      ..Default::default()
    });
    assert_eq!(options.named_api_bags.len(), 1);
    assert_eq!(options.named_api_bags.first().map(|bag| bag.callee), Some("useI18n"));
  }
}
