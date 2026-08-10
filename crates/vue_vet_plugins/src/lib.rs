//! Compile-time ecosystem plugins for the Vue Vet reactivity tracer.
//!
//! Hardcoded Nuxt / vue-i18n (and future) API bag contracts live here — not in
//! `vue_vet_reactivity`. The engine only consumes [`NamedApiBag`] rows.
//!
//! Not a dynamic JS plugin host. Register via [`default_named_api_bags`] at the
//! analysis boundary (Oxc adapter, session, CLI).

pub(crate) mod nuxt;
pub(crate) mod vue_i18n;

use vue_vet_reactivity::{NamedApiBag, TracerPlugin};

pub use nuxt::NuxtDataPlugin;
pub use vue_i18n::VueI18nPlugin;

/// Built-in ecosystem plugins shipped with Vue Vet.
#[must_use]
pub fn default_plugins() -> &'static [&'static dyn TracerPlugin] {
  &[&VueI18nPlugin, &NuxtDataPlugin]
}

/// Flattened named API bag catalog for the default plugin set (callee-sorted).
///
/// Prefer this at the Oxc / session boundary so every scan sees the same bags.
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
}
