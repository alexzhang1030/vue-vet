use super::helpers::*;

#[test]
fn seeds_use_i18n_locale_destructure() {
  let graph = graph(
    "const { locale, t } = useI18n();
     const label = computed(() => locale.value + t('x'));",
  );
  assert!(
    graph.bindings.iter().any(|b| b.name == "locale" && b.kind == ReactiveBindingKind::Computed),
    "useI18n locale must seed Computed; got {:?}",
    graph.bindings
  );
  assert!(
    !graph.bindings.iter().any(|b| b.name == "t"),
    "useI18n t function must not seed a binding; got {:?}",
    graph.bindings
  );
  assert!(
    graph.edges.iter().any(|e| e.from == "label" && e.to == "locale"),
    "computed must track locale; edges={:?}",
    graph.edges
  );
}

#[test]
fn use_i18n_translator_only_tracks_ambient_composer_deps() {
  // PublishWidget-style: `const { t } = useI18n(); computed(() => t('…'))`.
  let graph = graph(
    "const { t } = useI18n();\n\
     const expiresInOptions = computed(() => [t('time_ago_options.hour_future', 1)]);\n\
     void expiresInOptions.value;",
  );
  assert!(
    !graph.bindings.iter().any(|b| b.name == "t"),
    "t itself must not become a reactive binding; got {:?}",
    graph.bindings
  );
  let computed = graph.scopes.iter().find(|s| s.kind == TrackingScopeKind::Computed);
  assert!(
    computed.is_some_and(|scope| {
      !scope.reads.is_empty()
        && scope.reads.iter().all(|read| read.kind == ReactiveReadKind::Unconditional)
        && scope.reads.iter().any(|read| {
          matches!(read.property.as_deref(), Some("locale" | "fallbackLocale" | "messages"))
        })
    }),
    "t() must inject ambient composer reads; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn use_i18n_translator_prefers_co_destructured_locale() {
  let graph = graph(
    "const { locale, t } = useI18n();\n\
     const label = computed(() => t('hello'));\n\
     void label.value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| {
          read.binding == "locale"
            && read.property.as_deref() == Some("value")
            && read.kind != ReactiveReadKind::OutsideTracking
        })
    }),
    "t() with co-destructured locale must track locale.value; scopes={:?}",
    graph.scopes
  );
  // Under-approx: co-destructured ambient fields only — no extra site bag.
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().all(|read| read.binding == "locale")
    }),
    "co-destructured path should not invent extra ambient bindings; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn use_i18n_renamed_translator_tracks() {
  let graph = graph(
    "const { t: translate } = useI18n();\n\
     const label = computed(() => translate('x'));\n\
     void label.value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| {
          read.property.as_deref() == Some("locale")
            && read.kind != ReactiveReadKind::OutsideTracking
        })
    }),
    "renamed translator must still inject ambient deps; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn local_function_named_t_is_not_i18n_ambient() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function t() { return 'x'; }\n\
     const label = computed(() => t());\n\
     void label.value;",
  );
  assert!(
    graph
      .scopes
      .iter()
      .all(|scope| { scope.kind != TrackingScopeKind::Computed || scope.reads.is_empty() }),
    "local t() must not invent API ambient deps; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn use_i18n_translator_inside_then_is_outside_tracking() {
  let graph = graph(
    "import { watchEffect } from 'vue';\n\
     const { t } = useI18n();\n\
     watchEffect(() => { Promise.resolve().then(() => t('x')); });",
  );
  let effect = graph.scopes.iter().find(|s| s.kind == TrackingScopeKind::WatchEffect);
  assert!(effect.is_some(), "watchEffect scope missing; scopes={:?}", graph.scopes);
  let ambient: Vec<_> =
    effect.map(|scope| scope.reads.iter().map(|read| read.kind).collect()).unwrap_or_default();
  assert!(
    !ambient.is_empty(),
    "expected ambient reads from t() inside then(); scopes={:?}",
    graph.scopes
  );
  assert!(
    ambient.iter().all(|kind| *kind == ReactiveReadKind::OutsideTracking),
    "t() only from then() must stay outside tracking; reads={ambient:?} scopes={:?}",
    graph.scopes
  );
}
