//! One `define*` call per `<script setup>`. The five macros share the extra-call walk.

use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::extra_setup_calls;

macro_rules! duplicate_define_rule {
  ($meta:ident, $ty:ident, $rule:ident, $id:literal, $doc:literal, $name:literal) => {
    static $meta: RuleMeta = RuleMeta {
      id: $id,
      category: "correctness",
      default_severity: Severity::Error,
      confidence: Confidence::High,
      documentation: $doc,
      group: None,
    };

    pub(super) struct $ty;

    pub(super) static $rule: $ty = $ty;

    impl Rule for $ty {
      fn meta(&self) -> &'static RuleMeta {
        &$meta
      }

      fn run_once(&self, context: &mut RuleContext<'_>) {
        for call in extra_setup_calls(context.script(), $name) {
          context.report(
            self.meta(),
            call.span,
            concat!("`", $name, "` may only be called once in `<script setup>`").into(),
            Some(concat!("Merge the declarations into a single `", $name, "` call.").into()),
          );
        }
      }
    }
  };
}

duplicate_define_rule!(
  EMITS_META,
  NoDuplicateDefineEmits,
  EMITS,
  "vue-vet/correctness/no-duplicate-define-emits",
  "rules/correctness/no-duplicate-define-emits",
  "defineEmits"
);
duplicate_define_rule!(
  EXPOSE_META,
  NoDuplicateDefineExpose,
  EXPOSE,
  "vue-vet/correctness/no-duplicate-define-expose",
  "rules/correctness/no-duplicate-define-expose",
  "defineExpose"
);
duplicate_define_rule!(
  OPTIONS_META,
  NoDuplicateDefineOptions,
  OPTIONS,
  "vue-vet/correctness/no-duplicate-define-options",
  "rules/correctness/no-duplicate-define-options",
  "defineOptions"
);
duplicate_define_rule!(
  PROPS_META,
  NoDuplicateDefineProps,
  PROPS,
  "vue-vet/correctness/no-duplicate-define-props",
  "rules/correctness/no-duplicate-define-props",
  "defineProps"
);
duplicate_define_rule!(
  SLOTS_META,
  NoDuplicateDefineSlots,
  SLOTS,
  "vue-vet/correctness/no-duplicate-define-slots",
  "rules/correctness/no-duplicate-define-slots",
  "defineSlots"
);
