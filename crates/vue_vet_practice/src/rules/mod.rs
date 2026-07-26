use vue_vet_core::Rule;

mod vueuse_use_debounce_fn;
mod vueuse_use_event_listener;

pub fn all() -> Vec<&'static dyn Rule> {
  vec![&vueuse_use_debounce_fn::RULE, &vueuse_use_event_listener::RULE]
}
