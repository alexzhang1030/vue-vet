//! Nuxt data-fetch API bag contracts (`useAsyncData`, `useFetch`, …).

use vue_vet_core::ReactiveBindingKind;
use vue_vet_reactivity::{NamedApiBag, TracerPlugin};

/// Nuxt / Nitro-style async data helpers.
pub struct NuxtDataPlugin;

impl TracerPlugin for NuxtDataPlugin {
  fn id(&self) -> &'static str {
    "nuxt-data"
  }

  fn named_api_bags(&self) -> &'static [NamedApiBag] {
    BAGS
  }
}

fn async_data_field_kind(field: &str) -> Option<ReactiveBindingKind> {
  match field {
    // Nuxt `AsyncData` bag — reactive halves only (skip `refresh` / `execute` / `clear`).
    "data" | "pending" | "error" | "status" => Some(ReactiveBindingKind::Ref),
    _ => None,
  }
}

pub const ASYNC_DATA_BAG: NamedApiBag = NamedApiBag {
  callee: "useAsyncData",
  field_kind: async_data_field_kind,
  ambient_methods: &[],
  ambient_fields: &[],
};

pub const LAZY_ASYNC_DATA_BAG: NamedApiBag = NamedApiBag {
  callee: "useLazyAsyncData",
  field_kind: async_data_field_kind,
  ambient_methods: &[],
  ambient_fields: &[],
};

pub const FETCH_BAG: NamedApiBag = NamedApiBag {
  callee: "useFetch",
  field_kind: async_data_field_kind,
  ambient_methods: &[],
  ambient_fields: &[],
};

pub const LAZY_FETCH_BAG: NamedApiBag = NamedApiBag {
  callee: "useLazyFetch",
  field_kind: async_data_field_kind,
  ambient_methods: &[],
  ambient_fields: &[],
};

static BAGS: &[NamedApiBag] = &[ASYNC_DATA_BAG, FETCH_BAG, LAZY_ASYNC_DATA_BAG, LAZY_FETCH_BAG];
