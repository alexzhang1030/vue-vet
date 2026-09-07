#![expect(
  clippy::expect_used,
  clippy::format_push_string,
  clippy::needless_raw_string_hashes,
  reason = "scaling fixtures and bypass assertions"
)]

use super::helpers::*;
use vue_vet_core::{NotificationBypassKind, ReactiveViewKind};

#[test]
fn shallow_ref_nested_write_emits_bypass() {
  let (graph, work) = graph_work(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ count: 1 })
watchSyncEffect(() => { void state.value.count })
state.value.count = 2
"#,
  );
  assert_eq!(graph.notification_bypasses.len(), 1);
  let bypass = graph.notification_bypasses.first().expect("bypass");
  assert_eq!(bypass.kind, NotificationBypassKind::ShallowNested);
  assert_eq!(bypass.path, vec!["value".to_string(), "count".to_string()]);
  assert!(work.node_visits > 0);
  assert_eq!(work.emissions, 1);
}

#[test]
fn shallow_ref_slot_replacement_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ count: 1 })
watchSyncEffect(() => { void state.value.count })
state.value = { count: 2 }
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn shallow_reactive_nested_write_emits_bypass() {
  let graph = graph(
    r#"
import { shallowReactive, watchSyncEffect } from 'vue'
const state = shallowReactive({ details: { count: 1 } })
watchSyncEffect(() => { void state.details.count })
state.details.count = 2
"#,
  );
  assert_eq!(graph.notification_bypasses.len(), 1);
  assert_eq!(
    graph.notification_bypasses.first().expect("bypass").kind,
    NotificationBypassKind::ShallowNested
  );
}

#[test]
fn nested_reactive_child_is_quiet() {
  let graph = graph(
    r#"
import { shallowReactive, reactive, watchSyncEffect } from 'vue'
const state = shallowReactive({ details: reactive({ count: 1 }) })
watchSyncEffect(() => { void state.details.count })
state.details.count = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn toraw_direct_write_emits_bypass() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ count: 1 })
watchSyncEffect(() => { void state.count })
toRaw(state).count = 2
"#,
  );
  assert_eq!(graph.notification_bypasses.len(), 1);
  assert_eq!(
    graph.notification_bypasses.first().expect("bypass").kind,
    NotificationBypassKind::ToRawWrite
  );
}

#[test]
fn toraw_write_emits_bypass() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ count: 1 })
watchSyncEffect(() => { void state.count })
const raw = toRaw(state)
raw.count = 2
"#,
  );
  assert_eq!(graph.notification_bypasses.len(), 1);
  assert_eq!(
    graph.notification_bypasses.first().expect("bypass").kind,
    NotificationBypassKind::ToRawWrite
  );
  assert!(graph.source_views.iter().any(|view| view.view == ReactiveViewKind::Raw));
}

#[test]
fn proxy_write_is_quiet() {
  let graph = graph(
    r#"
import { reactive, watchSyncEffect } from 'vue'
const state = reactive({ count: 1 })
watchSyncEffect(() => { void state.count })
state.count = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn trigger_ref_keeps_shallow_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect, triggerRef } from 'vue'
const state = shallowRef({ count: 1 })
watchSyncEffect(() => { void state.value.count })
state.value.count = 2
triggerRef(state)
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn unrelated_property_is_quiet() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ count: 1, other: 0 })
watchSyncEffect(() => { void state.other })
const raw = toRaw(state)
raw.count = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn inactive_if_false_watcher_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const inactive = shallowRef({ n: 1 })
if (false) watchSyncEffect(() => { void inactive.value.n })
inactive.value.n = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn async_await_consumer_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const deferred = shallowRef({ n: 1 })
watchSyncEffect(async () => {
  await Promise.resolve()
  void deferred.value.n
})
deferred.value.n = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn stopped_handle_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const stopped = shallowRef({ n: 1 })
const handle = watchSyncEffect(() => { void stopped.value.n })
handle.stop()
stopped.value.n = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn write_only_consumer_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const writeOnly = shallowRef({ n: 1 })
watchSyncEffect(() => { writeOnly.value.n = 2 })
writeOnly.value.n = 3
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn duplicate_flush_post_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchEffect } from 'vue'
const duplicateFlush = shallowRef({ n: 1 })
watchEffect(() => { void duplicateFlush.value.n }, { flush: 'pre', flush: 'post' })
duplicateFlush.value.n = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn spread_flush_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchEffect } from 'vue'
const spreadFlush = shallowRef({ n: 1 })
const postOptions = { flush: 'post' }
watchEffect(() => { void spreadFlush.value.n }, { flush: 'pre', ...postOptions })
spreadFlush.value.n = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn spread_args_flush_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchEffect } from 'vue'
const spreadArgs = shallowRef({ n: 1 })
watchEffect(() => { void spreadArgs.value.n }, ...[{ flush: 'post' }])
spreadArgs.value.n = 2
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn mark_raw_reactive_is_quiet() {
  let graph = graph(
    r#"
import { reactive, markRaw, toRaw, watchSyncEffect } from 'vue'
const state = reactive(markRaw({ n: 0 }))
watchSyncEffect(() => { void state.n })
toRaw(state).n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn assignment_pattern_payload_replace_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, reactive, watchSyncEffect } from 'vue'
const replaced = shallowRef({ n: 0 })
;({ x: replaced.value } = { x: reactive({ n: 0 }) })
watchSyncEffect(() => { void replaced.value.n })
replaced.value.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn mutable_alias_replace_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, reactive, watchSyncEffect } from 'vue'
const aliased = shallowRef({ n: 0 })
let mutableAlias = aliased
mutableAlias.value = reactive({ n: 0 })
watchSyncEffect(() => { void aliased.value.n })
aliased.value.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn spread_payload_override_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, reactive, watchSyncEffect } from 'vue'
const spread = shallowRef({ item: { n: 0 }, ...{ item: reactive({ n: 0 }) } })
watchSyncEffect(() => { void spread.value.item.n })
spread.value.item.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn skip_flag_reactive_is_quiet() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const marked = reactive({ __v_skip: true, n: 0 })
watchSyncEffect(() => { void marked.n })
toRaw(marked).n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn nested_payload_member_replace_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, reactive, watchSyncEffect } from 'vue'
const replacedBeforeWatch = shallowRef({ item: { n: 0 } })
replacedBeforeWatch.value.item = reactive({ n: 0 })
watchSyncEffect(() => { void replacedBeforeWatch.value.item.n })
replacedBeforeWatch.value.item.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn toraw_through_nested_proxy_is_quiet() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const storedProxy = reactive({ item: reactive({ n: 0 }) })
const outerRaw = toRaw(storedProxy)
watchSyncEffect(() => { void storedProxy.item.n })
outerRaw.item.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn accessor_flush_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchEffect } from 'vue'
const accessor = shallowRef({ n: 0 })
watchEffect(() => { void accessor.value.n }, { get flush() { return 'post' } })
accessor.value.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn duplicate_key_payload_override_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, reactive, watchSyncEffect } from 'vue'
const duplicate = shallowRef({ item: { deep: { n: 0 } }, item: reactive({ deep: { n: 0 } }) })
watchSyncEffect(() => { void duplicate.value.item.deep.n })
duplicate.value.item.deep.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

fn independent_source(n: usize) -> String {
  let mut source = String::from("import { shallowRef, watchSyncEffect } from 'vue'\n");
  for index in 0..n {
    source.push_str(&format!(
      "function f{index}() {{\nconst s{index} = shallowRef({{ n: 1 }})\nwatchSyncEffect(() => {{ void s{index}.value.n }})\ns{index}.value.n = 2\n}}\nf{index}()\n"
    ));
  }
  source
}

#[test]
fn independent_local_scopes_scale_linearly() {
  let (g4, w4) = graph_work(&independent_source(4));
  let (g8, w8) = graph_work(&independent_source(8));
  let (g16, w16) = graph_work(&independent_source(16));
  assert_eq!(g4.notification_bypasses.len(), 4);
  assert_eq!(g8.notification_bypasses.len(), 8);
  assert_eq!(g16.notification_bypasses.len(), 16);
  assert_eq!(w4.emissions, 4);
  assert_eq!(w8.emissions, 8);
  assert_eq!(w16.emissions, 16);
  assert!(w16.node_visits <= w8.node_visits.saturating_mul(3));
  assert!(w16.reference_visits <= w8.reference_visits.saturating_mul(3));
  assert!(w16.queries <= w8.queries.saturating_mul(3));
  assert!(w16.events <= w8.events.saturating_mul(3));
  assert!(w16.copies <= w8.copies.saturating_mul(3).saturating_add(16));
}

fn one_source_many(n: usize) -> String {
  let mut source = String::from(
    "import { shallowRef, watchSyncEffect } from 'vue'\nconst state = shallowRef({ n: 1 })\n",
  );
  for index in 0..n {
    source.push_str(&format!(
      "if (false) watchSyncEffect(() => {{ void state.value.n; void {index} }})\n"
    ));
    source.push_str(&format!("watchSyncEffect(() => {{ void state.value.n; void {index} }})\n"));
    source.push_str(&format!("state.value.n = {index}\n"));
  }
  source
}

#[test]
fn one_source_many_consumers_and_writes_stays_indexed() {
  let (g4, _w4) = graph_work(&one_source_many(4));
  let (g8, w8) = graph_work(&one_source_many(8));
  let (g16, w16) = graph_work(&one_source_many(16));
  assert_eq!(g4.notification_bypasses.len(), 4);
  assert_eq!(g8.notification_bypasses.len(), 8);
  assert_eq!(g16.notification_bypasses.len(), 16);
  assert_eq!(w16.emissions, 16);
  assert!(w16.node_visits <= w8.node_visits.saturating_mul(3));
  assert!(w16.queries <= w8.queries.saturating_mul(3));
  assert!(w16.events <= w8.events.saturating_mul(3));
}

fn assert_indexed_growth(previous: usize, next: usize) {
  assert!(
    next.saturating_mul(10) < previous.saturating_mul(30),
    "indexed work {next} vs previous {previous} failed next*10 < previous*30"
  );
}

#[test]
fn readonly_raw_and_inherited_markers_are_quiet() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const readonlyMarker = reactive({ __v_isReadonly: true, n: 0 })
watchSyncEffect(() => { void readonlyMarker.n })
toRaw(readonlyMarker).n = 1
const rawMarker = reactive({ __v_raw: { n: 0 }, n: 0 })
watchSyncEffect(() => { void rawMarker.n })
toRaw(rawMarker).n = 1
const inheritedMarker = reactive({ __proto__: { __v_skip: true }, n: 0 })
watchSyncEffect(() => { void inheritedMarker.n })
toRaw(inheritedMarker).n = 1
const nestedMarker = reactive({ item: { __v_skip: true, n: 0 } })
watchSyncEffect(() => { void nestedMarker.item.n })
const rawNested = toRaw(nestedMarker)
rawNested.item.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn ordinary_nested_raw_write_stays_positive() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ item: { n: 0 } })
watchSyncEffect(() => { void state.item.n })
const raw = toRaw(state)
raw.item.n = 1
"#,
  );
  assert_eq!(graph.notification_bypasses.len(), 1);
}

#[test]
fn numeric_skip_and_string_readonly_are_quiet() {
  let graph = graph(
    r#"
import { reactive, toRaw, watchSyncEffect } from 'vue'
const skipped = reactive({ __v_skip: 1, n: 0 })
watchSyncEffect(() => { void skipped.n })
const skippedRaw = toRaw(skipped)
skippedRaw.n++
const readonlyState = reactive({ __v_isReadonly: 'yes', n: 0 })
watchSyncEffect(() => { void readonlyState.n })
const readonlyRaw = toRaw(readonlyState)
readonlyRaw.n++
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn existing_ref_identity_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ __v_isRef: true, value: { n: 0 } })
watchSyncEffect(() => { void state.value.n })
state.value.n++
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn unknown_and_accessor_markers_are_quiet() {
  let graph = graph(
    r#"
import { reactive, shallowRef, toRaw, watchSyncEffect } from 'vue'
const flag = 1
const skipped = reactive({ __v_skip: flag, n: 0 })
watchSyncEffect(() => { void skipped.n })
toRaw(skipped).n++
const accessor = reactive({ get __v_isReadonly() { return 'yes' }, n: 0 })
watchSyncEffect(() => { void accessor.n })
toRaw(accessor).n++
const maybeRef = shallowRef({ get __v_isRef() { return true }, value: { n: 0 } })
watchSyncEffect(() => { void maybeRef.value.n })
maybeRef.value.n++
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn unresolved_computed_marker_keys_are_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const flag = '__v_isRef'
const state = shallowRef({ [flag]: true, value: { n: 0 } })
watchSyncEffect(() => { void state.value.value.n })
state.value.value.n = 1
const joined = '__v_' + 'isRef'
const concat = shallowRef({ [joined]: true, value: { n: 0 } })
watchSyncEffect(() => { void concat.value.value.n })
concat.value.value.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn literal_computed_is_ref_key_is_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ ['__v_isRef']: true, value: { n: 0 } })
watchSyncEffect(() => { void state.value.value.n })
state.value.value.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn spreads_that_may_supply_marker_flags_are_quiet() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'
const extra = { __v_isRef: true }
const state = shallowRef({ ...extra, value: { n: 0 } })
watchSyncEffect(() => { void state.value.value.n })
state.value.value.n = 1
"#,
  );
  assert!(graph.notification_bypasses.is_empty());
}

#[test]
fn computed_key_and_spread_probe_only_ordinary_positive() {
  let graph = graph(
    r#"
import { shallowRef, watchSyncEffect } from 'vue'

function unresolvedFlag() {
  const flag = '__v_isRef'
  const state = shallowRef({ [flag]: true, value: { n: 0 } })
  watchSyncEffect(() => { void state.value.value.n })
  state.value.value.n = 1
}

function literalComputed() {
  const state = shallowRef({ ['__v_isRef']: true, value: { n: 0 } })
  watchSyncEffect(() => { void state.value.value.n })
  state.value.value.n = 1
}

function spreadFlags() {
  const extra = { __v_isRef: true }
  const state = shallowRef({ ...extra, value: { n: 0 } })
  watchSyncEffect(() => { void state.value.value.n })
  state.value.value.n = 1
}

unresolvedFlag()
literalComputed()
spreadFlags()

function positiveControl() {
  const state = shallowRef({ n: 0 })
  watchSyncEffect(() => { void state.value.n })
  state.value.n = 1
}
positiveControl()
"#,
  );
  assert_eq!(graph.notification_bypasses.len(), 1);
  let bypass = graph.notification_bypasses.first().expect("ordinary positive");
  assert_eq!(bypass.kind, NotificationBypassKind::ShallowNested);
  assert_eq!(bypass.path, vec!["value".to_string(), "n".to_string()]);
}

#[test]
fn truthy_markers_probe_only_ordinary_positive() {
  let graph = graph(
    r#"
import { reactive, shallowRef, toRaw, watchSyncEffect } from 'vue'

function numericSkip() {
  const state = reactive({ __v_skip: 1, n: 0 })
  watchSyncEffect(() => { void state.n })
  const raw = toRaw(state)
  raw.n++
}

function stringReadonly() {
  const state = reactive({ __v_isReadonly: 'yes', n: 0 })
  watchSyncEffect(() => { void state.n })
  const raw = toRaw(state)
  raw.n++
}

function normalizedRefMarker() {
  const state = shallowRef({ __v_isRef: true, value: { n: 0 } })
  watchSyncEffect(() => { void state.value.n })
  state.value.n++
}

numericSkip()
stringReadonly()
normalizedRefMarker()

function positiveControl() {
  const state = reactive({ n: 0 })
  watchSyncEffect(() => { void state.n })
  const raw = toRaw(state)
  raw.n = 1
}
positiveControl()
"#,
  );
  assert_eq!(graph.notification_bypasses.len(), 1);
  let bypass = graph.notification_bypasses.first().expect("ordinary positive");
  assert_eq!(bypass.kind, NotificationBypassKind::ToRawWrite);
  assert_eq!(bypass.path, vec!["n".to_string()]);
}

fn independent_source_n(n: usize) -> String {
  independent_source(n)
}

fn unrelated_consts(n: usize) -> String {
  let mut source = String::from("import { shallowRef, watchSyncEffect } from 'vue'\n");
  for index in 0..n {
    source.push_str(&format!(
      "const s{index} = shallowRef({{ n: 1 }})\nwatchSyncEffect(() => {{ void s{index}.value.n }})\ns{index}.value.n = 2\nconst u{index} = {index}\n"
    ));
  }
  source
}

fn alias_fanout(n: usize) -> String {
  let mut source = String::from(
    "import { reactive, toRaw, watchSyncEffect } from 'vue'\nconst state = reactive({",
  );
  for index in 0..n {
    source.push_str(&format!(" k{index}: {index},"));
  }
  source.push_str(" n: 0 })\n");
  for index in 0..n {
    source.push_str(&format!("const a{index} = state\n"));
  }
  source.push_str("watchSyncEffect(() => { void state.n })\nconst raw = toRaw(a0)\nraw.n = 1\n");
  source
}

fn escaped_aliases(n: usize) -> String {
  let mut source = String::from(
    "import { reactive, toRaw, watchSyncEffect } from 'vue'\nconst state = reactive({ n: 0 })\nwatchSyncEffect(() => { void state.n })\n",
  );
  for index in 0..n {
    source.push_str(&format!("export const escaped{index} = state\n"));
  }
  source.push_str("const raw = toRaw(state)\nraw.n = 1\n");
  source
}

fn wide_payload(n: usize) -> String {
  let mut source =
    String::from("import { shallowRef, watchSyncEffect } from 'vue'\nconst state = shallowRef({");
  for index in 0..n {
    source.push_str(&format!(" k{index}: {index},"));
  }
  source.push_str(" n: 1 })\nwatchSyncEffect(() => { void state.value.n })\nstate.value.n = 2\n");
  source
}

fn payload_replacements(n: usize) -> String {
  let mut source = String::from(
    "import { shallowRef, watchSyncEffect } from 'vue'\nconst state = shallowRef({ keep: { n: 1 }",
  );
  for index in 0..n {
    source.push_str(&format!(", k{index}: {{ n: {index} }}"));
  }
  source.push_str(" })\n");
  for index in 0..n {
    source.push_str(&format!("state.value.k{index} = {{ n: {index} }}\n"));
  }
  source.push_str("watchSyncEffect(() => { void state.value.keep.n })\nstate.value.keep.n = 2\n");
  source
}

fn many_reads(n: usize) -> String {
  let mut source = String::from(
    "import { shallowRef, watchSyncEffect } from 'vue'\nconst state = shallowRef({ n: 1",
  );
  for index in 0..n {
    source.push_str(&format!(", k{index}: {index}"));
  }
  source.push_str(" })\nwatchSyncEffect(() => {\n");
  for index in 0..n {
    source.push_str(&format!("  void state.value.k{index}\n"));
  }
  source.push_str("  void state.value.n\n})\nstate.value.n = 2\n");
  source
}

fn many_stops(n: usize) -> String {
  let mut source = String::from("import { shallowRef, watchSyncEffect } from 'vue'\n");
  for index in 0..n {
    source.push_str(&format!(
      "const s{index} = shallowRef({{ n: 1 }})\nconst h{index} = watchSyncEffect(() => {{ void s{index}.value.n }})\n"
    ));
  }
  for index in 0..n {
    source.push_str(&format!("h{index}.stop()\n"));
  }
  for index in 0..n {
    source.push_str(&format!("h{index}.stop()\n"));
  }
  for index in 0..n {
    source.push_str(&format!("s{index}.value.n = 2\n"));
  }
  source
}

fn named_exports(n: usize) -> String {
  let mut source = String::from("import { shallowRef, watchSyncEffect } from 'vue'\n");
  for index in 0..n {
    source.push_str(&format!(
      "const s{index} = shallowRef({{ n: 1 }})\nwatchSyncEffect(() => {{ void s{index}.value.n }})\ns{index}.value.n = 2\nexport {{ s{index} }}\n"
    ));
  }
  source
}

#[test]
fn independent_sources_index_work() {
  let (g32, w32) = graph_work(&independent_source_n(32));
  let (g64, w64) = graph_work(&independent_source_n(64));
  let (g128, w128) = graph_work(&independent_source_n(128));
  assert_eq!(g32.notification_bypasses.len(), 32);
  assert_eq!(g64.notification_bypasses.len(), 64);
  assert_eq!(g128.notification_bypasses.len(), 128);
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn unrelated_declarations_index_work() {
  let (g32, w32) = graph_work(&unrelated_consts(32));
  let (g64, w64) = graph_work(&unrelated_consts(64));
  let (g128, w128) = graph_work(&unrelated_consts(128));
  assert_eq!(g32.notification_bypasses.len(), 32);
  assert_eq!(g64.notification_bypasses.len(), 64);
  assert_eq!(g128.notification_bypasses.len(), 128);
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn alias_fanout_direct_from_source() {
  let (g32, w32) = graph_work(&alias_fanout(32));
  let (g64, w64) = graph_work(&alias_fanout(64));
  let (g128, w128) = graph_work(&alias_fanout(128));
  assert_eq!(g32.notification_bypasses.len(), 1);
  assert_eq!(g64.notification_bypasses.len(), 1);
  assert_eq!(g128.notification_bypasses.len(), 1);
  assert_eq!(w32.aliases, 33);
  assert_eq!(w64.aliases, 65);
  assert_eq!(w128.aliases, 129);
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn invalidated_exports_stay_quiet() {
  let (g32, w32) = graph_work(&escaped_aliases(32));
  let (g64, w64) = graph_work(&escaped_aliases(64));
  let (g128, w128) = graph_work(&escaped_aliases(128));
  assert!(g32.notification_bypasses.is_empty());
  assert!(g64.notification_bypasses.is_empty());
  assert!(g128.notification_bypasses.is_empty());
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn wide_payload_prefix_range() {
  let (g32, w32) = graph_work(&wide_payload(32));
  let (g64, w64) = graph_work(&wide_payload(64));
  let (g128, w128) = graph_work(&wide_payload(128));
  assert_eq!(g32.notification_bypasses.len(), 1);
  assert_eq!(g64.notification_bypasses.len(), 1);
  assert_eq!(g128.notification_bypasses.len(), 1);
  assert!(w128.payload_prefix_visits <= w64.payload_prefix_visits.saturating_mul(3));
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn payload_replacements_keep_sibling() {
  let (g32, w32) = graph_work(&payload_replacements(32));
  let (g64, w64) = graph_work(&payload_replacements(64));
  let (g128, w128) = graph_work(&payload_replacements(128));
  assert_eq!(g32.notification_bypasses.len(), 1);
  assert_eq!(g64.notification_bypasses.len(), 1);
  assert_eq!(g128.notification_bypasses.len(), 1);
  assert!(w32.payload_prefix_removals > 0);
  assert!(w64.payload_prefix_removals > w32.payload_prefix_removals);
  assert!(w128.payload_prefix_removals > w64.payload_prefix_removals);
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn many_reads_one_watcher_indexed() {
  let (g32, w32) = graph_work(&many_reads(32));
  let (g64, w64) = graph_work(&many_reads(64));
  let (g128, w128) = graph_work(&many_reads(128));
  assert_eq!(g32.notification_bypasses.len(), 1);
  assert_eq!(g64.notification_bypasses.len(), 1);
  assert_eq!(g128.notification_bypasses.len(), 1);
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn stop_buckets_bound_visits() {
  let (g32, w32) = graph_work(&many_stops(32));
  let (g64, w64) = graph_work(&many_stops(64));
  let (g128, w128) = graph_work(&many_stops(128));
  assert!(g32.notification_bypasses.is_empty());
  assert!(g64.notification_bypasses.is_empty());
  assert!(g128.notification_bypasses.is_empty());
  assert_eq!(w32.stop_bucket_visits, 32);
  assert_eq!(w64.stop_bucket_visits, 64);
  assert_eq!(w128.stop_bucket_visits, 128);
  assert!(w128.stop_bucket_visits <= w64.stop_bucket_visits.saturating_mul(3));
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}

#[test]
fn named_exports_module_scope_lookup() {
  let (g32, w32) = graph_work(&named_exports(32));
  let (g64, w64) = graph_work(&named_exports(64));
  let (g128, w128) = graph_work(&named_exports(128));
  assert!(g32.notification_bypasses.is_empty());
  assert!(g64.notification_bypasses.is_empty());
  assert!(g128.notification_bypasses.is_empty());
  assert_indexed_growth(w32.indexed_work(), w64.indexed_work());
  assert_indexed_growth(w64.indexed_work(), w128.indexed_work());
}
