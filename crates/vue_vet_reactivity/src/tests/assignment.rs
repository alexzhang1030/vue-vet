use super::helpers::*;

#[test]
fn nested_lvalue_reads_object_chain_not_plain_ref_assign() {
  let nested = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const draft = ref({ params: { scheduledAt: '' } });\n\
     const time = ref('09:00');\n\
     watchEffect(() => {\n\
       draft.value.params.scheduledAt = time.value;\n\
     });",
  );
  let scope = require_follow_scope(&nested, TrackingScopeKind::WatchEffect, "nested");
  assert!(
    scope
      .reads
      .iter()
      .any(|read| read.binding == "draft" && read.property.as_deref() == Some("value")),
    "lvalue must get draft.value: {:?}",
    scope.reads
  );
  assert!(
    scope
      .reads
      .iter()
      .any(|read| read.binding == "time" && read.property.as_deref() == Some("value")),
    "rhs must get time.value: {:?}",
    scope.reads
  );

  let plain = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const out = ref(0);\n\
     const source = ref(1);\n\
     watchEffect(() => {\n\
       out.value = source.value;\n\
     });",
  );
  let scope = require_follow_scope(&plain, TrackingScopeKind::WatchEffect, "plain");
  assert!(
    scope.reads.iter().all(|read| read.binding != "out"),
    "plain ref assign must not get out.value: {:?}",
    scope.reads
  );
  assert!(
    scope
      .reads
      .iter()
      .any(|read| read.binding == "source" && read.property.as_deref() == Some("value")),
    "plain assign still reads source: {:?}",
    scope.reads
  );
}

#[test]
fn computed_key_lvalue_reads_key_not_reactive_receiver() {
  let graph = graph(
    "import { reactive, ref, watchEffect } from 'vue';\n\
     const items = reactive({ a: 0 });\n\
     const key = ref('a');\n\
     const source = ref(1);\n\
     watchEffect(() => {\n\
       items[key.value] = source.value;\n\
     });",
  );
  let scope = require_follow_scope(&graph, TrackingScopeKind::WatchEffect, "watchEffect");
  assert!(
    scope.reads.iter().all(|read| read.binding != "items"),
    "Vue does not get the reactive receiver on `items[key] = source`; got {:?}",
    scope.reads
  );
  assert!(
    scope
      .reads
      .iter()
      .any(|read| read.binding == "key" && read.property.as_deref() == Some("value")),
    "indexed lvalue must read computed key: {:?}",
    scope.reads
  );
  assert!(
    scope
      .reads
      .iter()
      .any(|read| read.binding == "source" && read.property.as_deref() == Some("value")),
    "indexed lvalue must read source: {:?}",
    scope.reads
  );
}

#[test]
fn destructuring_assignment_is_write_only_for_assigned_member() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const target = ref(0);\n\
     const source = ref(1);\n\
     watchEffect(() => {\n\
       [target.value] = [source.value];\n\
     });",
  );
  let scope = require_follow_scope(&graph, TrackingScopeKind::WatchEffect, "watchEffect");
  assert!(
    scope.reads.iter().all(|read| read.binding != "target"),
    "destructuring write must not get target.value: {:?}",
    scope.reads
  );
  assert!(
    scope
      .reads
      .iter()
      .any(|read| read.binding == "source" && read.property.as_deref() == Some("value")),
    "destructuring rhs must get source.value: {:?}",
    scope.reads
  );
  assert!(
    scope
      .writes
      .iter()
      .any(|write| write.binding == "target" && write.property.as_deref() == Some("value")),
    "destructuring must record the target write: {:?}",
    scope.writes
  );
}

#[test]
fn object_pattern_assignment_is_write_only_for_assigned_member() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const target = ref(0);\n\
     const source = ref(1);\n\
     watchEffect(() => {\n\
       ({ field: target.value } = { field: source.value });\n\
     });",
  );
  let scope = require_follow_scope(&graph, TrackingScopeKind::WatchEffect, "watchEffect");
  assert!(
    scope.reads.iter().all(|read| read.binding != "target"),
    "object assignment pattern must not get target.value: {:?}",
    scope.reads
  );
}

#[test]
fn assignment_pattern_default_and_computed_key_are_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const source = ref({});\n\
     const key = ref('field');\n\
     const fallback = ref(1);\n\
     const target = ref(0);\n\
     watchEffect(() => { ({ [key.value]: target.value = fallback.value } = source.value) });",
  );
  let scope = require_follow_scope(&graph, TrackingScopeKind::WatchEffect, "watchEffect");
  let mut paths: Vec<_> = scope
    .reads
    .iter()
    .map(|read| format!("{}.{}", read.binding, read.property.as_deref().unwrap_or("")))
    .collect();
  paths.sort();
  assert_eq!(
    paths,
    vec!["fallback.value".to_string(), "key.value".to_string(), "source.value".to_string()],
    "computed key and default initializer must stay reads; got {:?}",
    scope.reads
  );
  assert!(
    scope
      .writes
      .iter()
      .any(|write| write.binding == "target" && write.property.as_deref() == Some("value")),
    "pattern binding must remain a write: {:?}",
    scope.writes
  );
}

#[test]
fn helper_form_matches_inline_lvalue_reads() {
  let inline = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const draft = ref({ params: { scheduledAt: '' } });\n\
     const time = ref('09:00');\n\
     watchEffect(() => {\n\
       draft.value.params.scheduledAt = time.value;\n\
     });",
  );
  let helper = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const draft = ref({ params: { scheduledAt: '' } });\n\
     const time = ref('09:00');\n\
     function assign() { draft.value.params.scheduledAt = time.value; }\n\
     watchEffect(() => { assign(); });",
  );
  let inline_scope = require_follow_scope(&inline, TrackingScopeKind::WatchEffect, "inline");
  let helper_scope = require_follow_scope(&helper, TrackingScopeKind::WatchEffect, "helper");
  let keys = |scope: &vue_vet_core::TrackingScopeFact| {
    let mut keys: Vec<(String, Option<String>)> =
      scope.reads.iter().map(|read| (read.binding.clone(), read.property.clone())).collect();
    keys.sort_unstable();
    keys
  };
  assert_eq!(keys(inline_scope), keys(helper_scope), "helper lvalue reads must match inline");
}
