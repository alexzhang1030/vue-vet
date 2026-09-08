use super::helpers::*;

fn ref_only_module(index: usize) -> String {
  format!("import {{ ref }} from 'vue'; export const value{index} = ref({index});")
}

fn assert_one_summary_import_index(source: &str, scan: crate::SummaryScanWork) {
  assert_eq!(
    scan.import_index_builds, 1,
    "summary must build the canonical import index once for {source}: {scan:?}"
  );
  assert!(
    scan.import_index_node_visits > 0,
    "the one import-index scan must visit semantic nodes for {source}: {scan:?}"
  );
}

fn assert_empty_usage_walk(source: &str, usage: crate::ComposableUsageWork) {
  assert_eq!(
    usage.definition_count, 0,
    "ref-only / empty modules have no local composable defs: {source} {usage:?}"
  );
  assert_eq!(
    usage.usage_node_visits, 0,
    "empty definition maps must not walk call-use nodes: {source} {usage:?}"
  );
}

fn assert_populated_usage_walk(source: &str, usage: crate::ComposableUsageWork) {
  assert!(
    usage.definition_count > 0,
    "populated composable defs must be recorded: {source} {usage:?}"
  );
  assert!(
    usage.usage_node_visits > 0,
    "populated definition maps must walk call-use nodes: {source} {usage:?}"
  );
}

#[test]
fn ref_only_5k_shape_reuses_one_import_index_and_skips_empty_usage_walk() {
  for index in [0usize, 1, 4999] {
    let source = ref_only_module(index);
    let (graph, summary, scan, usage) = graph_and_summary(&source, ScriptKind::Script);
    assert_one_summary_import_index(&source, scan);
    assert_empty_usage_walk(&source, usage);
    assert_eq!(graph.bindings.len(), 1, "one exported ref for {source}: {:?}", graph.bindings);
    let expected_name = format!("value{index}");
    assert_eq!(
      graph.bindings.first().map(|binding| binding.name.as_str()),
      Some(expected_name.as_str())
    );
    assert_eq!(graph.bindings.first().map(|binding| binding.kind), Some(ReactiveBindingKind::Ref));
    assert_eq!(
      graph.bindings.first().map(|binding| binding.span.offset),
      source.find(&expected_name),
      "binding span stays on the identifier for {source}"
    );
    assert!(graph.scopes.is_empty(), "ref-only modules have no tracking scopes");
    assert!(graph.source_views.is_empty());
    assert!(graph.notification_bypasses.is_empty());
    assert!(summary.has_reactivity_export_seeds(), "exported ref is a summary seed for {source}");
  }
}

#[test]
fn empty_module_builds_one_import_index_and_skips_usage_walk() {
  let source = "";
  let (graph, summary, scan, usage) = graph_and_summary(source, ScriptKind::Script);
  assert_one_summary_import_index(source, scan);
  assert_empty_usage_walk(source, usage);
  assert!(graph.bindings.is_empty());
  assert!(graph.scopes.is_empty());
  assert!(graph.source_views.is_empty());
  assert!(graph.notification_bypasses.is_empty());
  assert!(!summary.has_reactivity_export_seeds());
  assert!(!summary.has_component_factory_local());
}

#[test]
fn named_namespace_string_type_only_bare_and_shadow_apis_keep_summary_and_graph() {
  let named = "import { ref as signal } from 'vue'; export const value = signal(0);";
  let (named_graph, named_summary, named_scan, named_usage) =
    graph_and_summary(named, ScriptKind::Script);
  assert_one_summary_import_index(named, named_scan);
  assert_empty_usage_walk(named, named_usage);
  assert_eq!(
    named_graph.bindings.first().map(|binding| binding.kind),
    Some(ReactiveBindingKind::Ref)
  );
  assert!(named_summary.has_reactivity_export_seeds());

  let string_name = "import { 'ref' as box } from 'vue'; export const value = box(0);";
  let (string_graph, string_summary, string_scan, string_usage) =
    graph_and_summary(string_name, ScriptKind::Script);
  assert_one_summary_import_index(string_name, string_scan);
  assert_empty_usage_walk(string_name, string_usage);
  assert_eq!(
    string_graph.bindings.first().map(|binding| binding.kind),
    Some(ReactiveBindingKind::Ref)
  );
  assert_eq!(named_graph.bindings.len(), string_graph.bindings.len());
  assert!(string_summary.has_reactivity_export_seeds());

  let namespace = "import * as Vue from 'vue'; export const value = Vue.ref(0);";
  let (namespace_graph, _, namespace_scan, namespace_usage) =
    graph_and_summary(namespace, ScriptKind::Script);
  assert_one_summary_import_index(namespace, namespace_scan);
  assert_empty_usage_walk(namespace, namespace_usage);
  assert_eq!(namespace_graph.bindings.len(), 1);

  let type_only = "import { type ref } from 'vue'; export const value = ref(0);";
  let (type_graph, _, type_scan, type_usage) = graph_and_summary(type_only, ScriptKind::Script);
  assert_one_summary_import_index(type_only, type_scan);
  assert_empty_usage_walk(type_only, type_usage);
  assert_eq!(
    type_graph.bindings.first().map(|binding| binding.kind),
    Some(ReactiveBindingKind::Ref),
    "type-only named specifiers stay in the canonical import index"
  );

  let bare = "export const value = ref(0);";
  let (bare_graph, _, bare_scan, bare_usage) = graph_and_summary(bare, ScriptKind::Script);
  assert_one_summary_import_index(bare, bare_scan);
  assert_empty_usage_walk(bare, bare_usage);
  assert_eq!(
    bare_graph.bindings.first().map(|binding| binding.kind),
    Some(ReactiveBindingKind::Ref),
    "unresolved auto-import ref() still binds"
  );

  let shadow = "function ref(value: number) { return { value }; }\nexport const value = ref(0);";
  let (shadow_graph, _, shadow_scan, shadow_usage) = graph_and_summary(shadow, ScriptKind::Script);
  assert_one_summary_import_index(shadow, shadow_scan);
  assert!(
    shadow_graph.bindings.is_empty(),
    "local ref lookalikes must not create nodes: {:?}",
    shadow_graph.bindings
  );
  assert_populated_usage_walk(shadow, shadow_usage);
}

#[test]
fn populated_local_composable_bags_factories_and_forwarders_keep_usage_walk() {
  let bag = "import { ref, computed } from 'vue';\n\
     function useInner() { return { data: ref(0), ready: ref(false) }; }\n\
     function useOuter() { return useInner(); }\n\
     const { data, ready } = useOuter();\n\
     const rows = computed(() => data.value);\n\
     const pending = computed(() => ready.value);";
  let (bag_graph, bag_summary, bag_scan, bag_usage) = graph_and_summary(bag, ScriptKind::Setup);
  assert_one_summary_import_index(bag, bag_scan);
  assert_populated_usage_walk(bag, bag_usage);
  assert!(
    bag_graph
      .bindings
      .iter()
      .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
      && bag_graph
        .bindings
        .iter()
        .any(|binding| binding.name == "ready" && binding.kind == ReactiveBindingKind::Ref),
    "forwarded bag destructure must seed: {:?}",
    bag_graph.bindings
  );
  assert!(bag_summary.has_reactivity_export_seeds());

  let factory = "import { ref, computed } from 'vue';\n\
     function useFlag() { const flag = ref(false); return flag; }\n\
     const enabled = useFlag();\n\
     const shown = computed(() => enabled.value);";
  let (factory_graph, _, factory_scan, factory_usage) =
    graph_and_summary(factory, ScriptKind::Setup);
  assert_one_summary_import_index(factory, factory_scan);
  assert_populated_usage_walk(factory, factory_usage);
  assert!(
    factory_graph
      .bindings
      .iter()
      .any(|binding| binding.name == "enabled" && binding.kind == ReactiveBindingKind::Ref),
    "scalar factory call must seed: {:?}",
    factory_graph.bindings
  );

  let value_bag = "import { ref, computed } from 'vue';\n\
     function useMapsGet() { return { data: ref(0), isLoading: ref(false) }; }\n\
     function createApi() { return { maps: { useMapsGet } }; }\n\
     const api = createApi();\n\
     const { data, isLoading } = api.maps.useMapsGet();\n\
     const rows = computed(() => data.value);\n\
     const pending = computed(() => isLoading.value);";
  let (value_graph, _, value_scan, value_usage) = graph_and_summary(value_bag, ScriptKind::Setup);
  assert_one_summary_import_index(value_bag, value_scan);
  assert_populated_usage_walk(value_bag, value_usage);
  assert!(
    value_graph
      .bindings
      .iter()
      .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
      && value_graph
        .bindings
        .iter()
        .any(|binding| binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref),
    "value-bag member destructure must seed: {:?}",
    value_graph.bindings
  );
}

#[test]
fn options_typed_define_component_and_provide_inject_reuse_one_import_index() {
  let options = "import { computed, type Ref } from 'vue';\n\
     interface FormCtx { values: Ref<{ name: string }> }\n\
     type FormSetup = (ctx: FormCtx) => void;\n\
     function defineFormProps(props: { setup?: FormSetup }) {\n\
       props.setup?.({ values: null as unknown as Ref<{ name: string }> });\n\
     }\n\
     defineFormProps({\n\
       setup({ values }) {\n\
         return computed(() => values.value.name);\n\
       },\n\
     });";
  let (options_graph, _, options_scan, _) = graph_and_summary(options, ScriptKind::Setup);
  assert_one_summary_import_index(options, options_scan);
  assert!(
    options_graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "values")
        && scope.uncertain_accesses.is_empty()
    }),
    "options callback slots must still seed: {:?}",
    options_graph.scopes
  );

  let typed = "import type { ComputedRef } from 'vue';\n\
     import { computed } from 'vue';\n\
     function usePagedQuery<T>(\n\
       _init: T,\n\
       run: (state: ComputedRef<T & { page: number }>) => unknown,\n\
     ) {\n\
       void run;\n\
     }\n\
     usePagedQuery({ q: '' }, (state) => {\n\
       const page = computed(() => state.value.page);\n\
       void page.value;\n\
     });";
  let (typed_graph, _, typed_scan, _) = graph_and_summary(typed, ScriptKind::Setup);
  assert_one_summary_import_index(typed, typed_scan);
  assert!(
    typed_graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "state" && read.property.as_deref() == Some("value"))
        && !scope.uncertain_accesses.iter().any(|name| name == "state")
    }),
    "typed callback param slots must still seed: {:?}",
    typed_graph.scopes
  );

  let component = "import { computed, defineComponent } from 'vue';\n\
     export default defineComponent({\n\
       props: { displayMode: String },\n\
       setup(props) {\n\
         const mode = computed(() => props.displayMode || 'whiteboard');\n\
         return () => mode.value;\n\
       },\n\
     });";
  let (component_graph, component_summary, component_scan, _) =
    graph_and_summary(component, ScriptKind::Setup);
  assert_one_summary_import_index(component, component_scan);
  assert!(
    component_graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("displayMode"))
    }),
    "defineComponent setup props must track: {:?}",
    component_graph.scopes
  );
  assert!(
    component_summary.has_component_factory_local()
      || component_summary.has_reactivity_export_seeds()
  );

  let inject = "import { provide, inject, ref, computed } from 'vue';\n\
     const count = ref(1);\n\
     provide('count', count);\n\
     const injected = inject('count');\n\
     const doubled = computed(() => injected.value * 2);\n\
     void doubled.value;";
  let (inject_graph, _, inject_scan, inject_usage) = graph_and_summary(inject, ScriptKind::Setup);
  assert_one_summary_import_index(inject, inject_scan);
  assert_empty_usage_walk(inject, inject_usage);
  assert!(
    inject_graph
      .bindings
      .iter()
      .any(|binding| binding.name == "injected" && binding.kind == ReactiveBindingKind::Ref),
    "same-file provide/inject must seed: {:?}",
    inject_graph.bindings
  );
}

#[test]
fn seeded_local_and_seed_only_graphs_keep_complete_parity() {
  let seeds = crate::TraceSeeds::with_bindings(vec![super::notification::seeded_count_fact()]);
  let local = r"
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ count: 1 })
watchSyncEffect(() => { void state.value.count; void seededCount.value })
state.value.count = 2
";
  let (gated, summary, scan, usage) =
    graph_and_summary_with_seeds(local, ScriptKind::Setup, &seeds);
  assert_one_summary_import_index(local, scan);
  assert_empty_usage_walk(local, usage);
  assert!(summary.has_reactivity_export_seeds());
  assert_eq!(gated.source_views.len(), 1);
  assert_eq!(gated.notification_bypasses.len(), 1);
  assert!(
    gated
      .bindings
      .iter()
      .any(|binding| binding.name == "seededCount" && binding.kind == ReactiveBindingKind::Ref),
    "seeded binding must remain on the local graph: {:?}",
    gated.bindings
  );

  let seed_only = r"
import { watchEffect } from 'vue'
watchEffect(() => { void seededCount.value })
";
  let (seed_gated, seed_work) = graph_seeded_work(seed_only, &seeds);
  let (seed_graph, seed_summary, seed_scan, seed_usage) =
    graph_and_summary_with_seeds(seed_only, ScriptKind::Setup, &seeds);
  assert_eq!(seed_gated, seed_graph, "seed-only helper graph must match gated work helper");
  assert_one_summary_import_index(seed_only, seed_scan);
  assert_empty_usage_walk(seed_only, seed_usage);
  assert!(seed_graph.source_views.is_empty());
  assert!(seed_graph.notification_bypasses.is_empty());
  assert!(
    seed_graph
      .bindings
      .iter()
      .any(|binding| binding.name == "seededCount" && binding.kind == ReactiveBindingKind::Ref)
  );
  assert!(seed_work.is_import_preflight_only(), "seed-only stays import-preflight: {seed_work:?}");
  assert!(
    seed_summary.has_reactivity_export_seeds(),
    "seed-only summary keeps the seeded binding on the local graph"
  );
}
