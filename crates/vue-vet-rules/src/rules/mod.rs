use vue_vet_core::Rule;

mod anchor_has_content;
mod button_has_content;
mod click_events_have_key_events;
mod iframe_has_title;
mod img_has_alt;
mod no_aria_hidden_on_focusable;
mod no_autofocus;
mod no_conditional_watch_effect_dependency;
mod no_deprecated_slot_scope;
mod no_deprecated_v_on_native_modifier;
mod no_distracting_elements;
mod no_duplicate_define_emits;
mod no_duplicate_define_expose;
mod no_duplicate_define_options;
mod no_duplicate_define_props;
mod no_duplicate_define_slots;
mod no_mutating_props;
mod no_nonreactive_props_destructure;
mod no_positive_tabindex;
mod no_redundant_role;
mod no_v_html;
mod no_v_if_with_v_for;
mod prefer_use_template_ref;
mod require_component_is;
mod require_v_for_key;
mod valid_aria_role;
mod valid_v_html;
mod valid_v_text;

pub(super) fn builtins() -> Vec<&'static dyn Rule> {
  vec![
    &anchor_has_content::RULE,
    &button_has_content::RULE,
    &click_events_have_key_events::RULE,
    &iframe_has_title::RULE,
    &img_has_alt::RULE,
    &no_aria_hidden_on_focusable::RULE,
    &no_autofocus::RULE,
    &no_conditional_watch_effect_dependency::RULE,
    &no_deprecated_slot_scope::RULE,
    &no_deprecated_v_on_native_modifier::RULE,
    &no_distracting_elements::RULE,
    &no_duplicate_define_emits::RULE,
    &no_duplicate_define_expose::RULE,
    &no_duplicate_define_options::RULE,
    &no_duplicate_define_props::RULE,
    &no_duplicate_define_slots::RULE,
    &no_mutating_props::RULE,
    &no_nonreactive_props_destructure::RULE,
    &no_positive_tabindex::RULE,
    &no_redundant_role::RULE,
    &no_v_html::RULE,
    &no_v_if_with_v_for::RULE,
    &prefer_use_template_ref::RULE,
    &require_component_is::RULE,
    &require_v_for_key::RULE,
    &valid_aria_role::RULE,
    &valid_v_html::RULE,
    &valid_v_text::RULE,
  ]
}
