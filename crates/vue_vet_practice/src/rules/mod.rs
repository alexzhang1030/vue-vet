use vue_vet_core::Rule;

mod prefer_attached_effect_scope;
mod prefer_conditional_watch_source;
mod prefer_define_model;
mod prefer_keyed_map_dependency;
mod prefer_lazy_computed_async;
mod prefer_queued_watch_flush;
mod prefer_stable_computed_identity;
mod prefer_sync_ref_one_way;
mod prefer_to_value;
mod prefer_use_slots_attrs;
mod prefer_use_template_ref;
mod vueuse_observers;
mod vueuse_use_debounce_fn;
mod vueuse_use_event_listener;
mod vueuse_use_interval_fn;
mod vueuse_use_raf_fn;
mod vueuse_use_timeout_fn;
mod vueuse_use_window_size;

pub fn all() -> Vec<&'static dyn Rule> {
  vec![
    &prefer_attached_effect_scope::RULE,
    &prefer_conditional_watch_source::RULE,
    &prefer_define_model::RULE,
    &prefer_keyed_map_dependency::RULE,
    &prefer_lazy_computed_async::RULE,
    &prefer_queued_watch_flush::RULE,
    &prefer_stable_computed_identity::RULE,
    &prefer_sync_ref_one_way::RULE,
    &prefer_to_value::RULE,
    &prefer_use_slots_attrs::RULE,
    &prefer_use_template_ref::RULE,
    &vueuse_use_debounce_fn::RULE,
    &vueuse_use_event_listener::RULE,
    &vueuse_observers::INTERSECTION,
    &vueuse_use_interval_fn::RULE,
    &vueuse_observers::MUTATION,
    &vueuse_use_raf_fn::RULE,
    &vueuse_observers::RESIZE,
    &vueuse_use_timeout_fn::RULE,
    &vueuse_use_window_size::RULE,
  ]
}
