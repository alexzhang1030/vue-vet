//! vue-i18n composition API bag contract (`useI18n`).

use vue_vet_core::ReactiveBindingKind;
use vue_vet_reactivity::{NamedApiBag, TracerPlugin};

/// vue-i18n `useI18n()` composer surface.
pub struct VueI18nPlugin;

impl TracerPlugin for VueI18nPlugin {
  fn id(&self) -> &'static str {
    "vue-i18n"
  }

  fn named_api_bags(&self) -> &'static [NamedApiBag] {
    BAGS
  }
}

fn i18n_field_kind(field: &str) -> Option<ReactiveBindingKind> {
  match field {
    // vue-i18n composition API — locale/locales/messages are computed/ref-like.
    "locale" | "fallbackLocale" | "locales" | "messages" | "availableLocales" => {
      Some(ReactiveBindingKind::Computed)
    }
    _ => None,
  }
}

pub const USE_I18N_BAG: NamedApiBag = NamedApiBag {
  callee: "useI18n",
  field_kind: i18n_field_kind,
  // vue-i18n Composer: wrapWithDeps on these methods.
  ambient_methods: &["t", "d", "n", "rt", "te"],
  ambient_fields: &["locale", "fallbackLocale", "messages"],
};

static BAGS: &[NamedApiBag] = &[USE_I18N_BAG];
