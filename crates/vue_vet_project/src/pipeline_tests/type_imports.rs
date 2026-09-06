use std::sync::Arc;

use vue_vet_core::{ReactiveBindingKind, ScriptKind, TrackingScopeKind};
use vue_vet_oxc::analyze_module_source;
use vue_vet_reactivity::{ModuleReactivity, ModuleSource};

use super::helpers::*;

#[expect(clippy::panic, reason = "unexpected Oxc errors must fail the pipeline unit test")]
fn analyzed_ts(path: &str, source: &str) -> ProjectFile {
  let analysis = analyze_module_source(source, source, 0, "ts", ScriptKind::Script)
    .unwrap_or_else(|error| panic!("oxc analysis of {path} failed: {error}"));
  ProjectFile {
    path: path.into(),
    source_len: source.len(),
    facts: SfcFacts {
      template: analysis.template_facts,
      script: ScriptFacts { blocks: vec![analysis.script_facts] },
    }
    .into(),
    module_source: Some(Arc::new(
      ModuleSource::standalone(path, source, "ts", ScriptKind::Script)
        .with_module_summary(analysis.module_trace),
    )),
    ordinary_module_source: None,
  }
}

fn computed_reads_item_value(module: &Arc<ModuleReactivity>) -> bool {
  module
    .graph
    .bindings
    .iter()
    .any(|binding| binding.name == "item" && binding.kind == ReactiveBindingKind::Ref)
    && module.graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "item" && read.property.as_deref() == Some("value"))
        && !scope.uncertain_accesses.iter().any(|name| name == "item")
    })
}

fn write_split_state_kit(project: &TempProject) {
  project.write(
    "node_modules/state-kit/package.json",
    r#"{"name":"state-kit","version":"1.0.0","type":"module","types":"./public.d.ts","exports":{".":{"import":{"types":"./public.d.mts","default":"./runtime.mjs"}}}}"#,
  );
  project.write(
    "node_modules/state-kit/public.d.ts",
    "import type { Ref } from 'vue'\nexport interface Item { value: number }\nexport declare function make(): Ref<number>\n",
  );
  project.write(
    "node_modules/state-kit/public.d.mts",
    "import type { Ref } from 'vue'\nexport interface Item { value: number }\nexport declare function make(): Ref<number>\n",
  );
  project.write(
    "node_modules/state-kit/runtime.mjs",
    "export function make() { return { value: 1 } }\n",
  );
}

#[test]
fn split_package_type_and_runtime_exports_keep_complete_analysis() {
  let project = TempProject::new("split-package-exports");
  project.write("package.json", r#"{"private":true,"dependencies":{"state-kit":"1.0.0"}}"#);
  write_split_state_kit(&project);
  let source = "import type { Item } from 'state-kit'\n\
import { computed } from 'vue'\n\
import { make } from 'state-kit'\n\
export const item = make()\n\
export const doubled = computed(() => item.value * 2)\n";
  project.write("src/consumer.ts", source);
  let consumer = analyzed_ts("src/consumer.ts", source);
  let graph = build_project_graph(project.root(), &[consumer]);
  assert!(
    graph.reactivity_error.is_none(),
    "nested types/default package exports must not collide: {:?}",
    graph.reactivity_error
  );
  let module = graph.module_reactivity.iter().find(|module| module.id == "src/consumer.ts");
  assert!(
    module.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "item" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "item" && read.property.as_deref() == Some("value"))
        })
    }),
    "Ref factory result must seed computed dependency; got {:?}",
    module.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn local_type_declaration_and_runtime_js_share_specifier_without_collision() {
  let project = TempProject::new("local-type-runtime");
  let dts = "import type { Ref } from 'vue'\nexport type Item = { value: number }\nexport declare function make(): Ref<number>\n";
  let js = "import { ref } from 'vue'\nexport function make() { return ref(1) }\n";
  let source = "import type { Item } from './types'\n\
import { computed } from 'vue'\n\
import { make } from './types'\n\
export const item = make()\n\
export const doubled = computed(() => item.value * 2)\n";
  project.write("src/types.d.ts", dts);
  project.write("src/types.js", js);
  project.write("src/consumer.ts", source);
  let types_dts = analyzed_ts("src/types.d.ts", dts);
  let types_js = analyzed_ts("src/types.js", js);
  let consumer = analyzed_ts("src/consumer.ts", source);
  let graph = build_project_graph(project.root(), &[consumer, types_dts, types_js]);
  assert!(
    graph.reactivity_error.is_none(),
    "type-only .d.ts and runtime .js for one specifier must not collide: {:?}",
    graph.reactivity_error
  );
  assert!(
    graph.edges.iter().any(|edge| edge.kind == EdgeKind::Import && edge.to.contains("types.js")),
    "runtime import must keep the .js Import edge: {:?}",
    graph.edges
  );
  let module = graph.module_reactivity.iter().find(|module| module.id == "src/consumer.ts");
  assert!(
    module.is_some_and(computed_reads_item_value),
    "local `ref()` factory result `item` must be a proven computed `.value` read: {:?}",
    module.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn local_plain_object_factory_keeps_uncertain_access() {
  let project = TempProject::new("local-plain-object-factory");
  let dts = "export type Item = { value: number }\nexport declare function make(): Item\n";
  let js = "export function make() { return { value: 1 } }\n";
  let source = "import type { Item } from './types'\n\
import { computed } from 'vue'\n\
import { make } from './types'\n\
export const item = make()\n\
export const doubled = computed(() => item.value * 2)\n";
  project.write("src/types.d.ts", dts);
  project.write("src/types.js", js);
  project.write("src/consumer.ts", source);
  let types_dts = analyzed_ts("src/types.d.ts", dts);
  let types_js = analyzed_ts("src/types.js", js);
  let consumer = analyzed_ts("src/consumer.ts", source);
  let graph = build_project_graph(project.root(), &[consumer, types_dts, types_js]);
  assert!(graph.reactivity_error.is_none(), "{:?}", graph.reactivity_error);
  let module = graph.module_reactivity.iter().find(|module| module.id == "src/consumer.ts");
  assert!(
    module.is_some_and(|module| {
      !module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "item" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.uncertain_accesses.iter().any(|name| name == "item")
        })
    }),
    "plain-object factory must stay uncertain, not a proven Ref: {:?}",
    module.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn mixed_named_type_specifier_keeps_runtime_import_edge() {
  let project = TempProject::new("mixed-named-type");
  project
    .write("src/flags.js", "import { ref } from 'vue'\nexport function make() { return ref(1) }\n");
  project.write(
    "src/flags.d.ts",
    "import type { Ref } from 'vue'\nexport type Flag = boolean\nexport declare function make(): Ref<number>\n",
  );
  let source = "import { type Flag, make } from './flags'\n\
import { computed } from 'vue'\n\
export const ready: Flag = true\n\
export const item = make()\n\
export const doubled = computed(() => item.value * 2)\n";
  project.write("src/consumer.ts", source);
  let flags = analyzed_ts(
    "src/flags.js",
    "import { ref } from 'vue'\nexport function make() { return ref(1) }\n",
  );
  let consumer = analyzed_ts("src/consumer.ts", source);
  let graph = build_project_graph(project.root(), &[consumer, flags]);
  assert!(graph.reactivity_error.is_none(), "{:?}", graph.reactivity_error);
  assert!(
    graph.edges.iter().any(|edge| edge.kind == EdgeKind::Import && edge.to.contains("flags.js")),
    "mixed named import must retain the runtime Import edge: {:?}",
    graph.edges
  );
  let module = graph.module_reactivity.iter().find(|module| module.id == "src/consumer.ts");
  assert!(
    module.is_some_and(computed_reads_item_value),
    "mixed named type import must keep `item` as a proven Ref `.value` read: {:?}",
    module.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn type_only_component_alias_does_not_suppress_unused_component() {
  let project = TempProject::new("type-only-component-alias");
  let mut page = file("pages/index.vue", &[("../components/Widget.vue", "Widget")], &[], &[]);
  {
    let facts = std::sync::Arc::make_mut(&mut page.facts);
    if let Some(block) = facts.script.blocks.first_mut()
      && let Some(import) = block.imports.first_mut()
    {
      import.type_only = true;
    }
  }
  let component = file("components/Widget.vue", &[], &[], &[]);
  materialize(&project, &[page.clone(), component.clone()]);
  let graph = build_project_graph(project.root(), &[page, component]);
  assert!(
    graph.diagnostics.iter().any(|diagnostic| diagnostic.rule_id == PROJECT_RULE_IDS[1]),
    "type-only component alias must not count as usage: {:?}",
    graph.diagnostics
  );
  assert!(
    graph.edges.iter().all(|edge| edge.kind != EdgeKind::ComponentUsage),
    "type-only alias must not emit component-use evidence: {:?}",
    graph.edges
  );
}

#[test]
fn type_only_unresolved_imports_keep_declaration_grouping() {
  let project = TempProject::new("type-only-unresolved");
  let source = "import type { Item, Other } from './missing'\nimport type { Flag } from './gone'\n";
  project.write("src/consumer.ts", source);
  let consumer = analyzed_ts("src/consumer.ts", source);
  let graph = build_project_graph(project.root(), &[consumer]);
  let unresolved = graph
    .diagnostics
    .iter()
    .filter(|diagnostic| diagnostic.rule_id == PROJECT_RULE_IDS[0])
    .collect::<Vec<_>>();
  assert_eq!(unresolved.len(), 2, "one diagnostic per type-only declaration: {unresolved:?}");
  let Some(first) = unresolved.first() else {
    return;
  };
  let Some(second) = unresolved.get(1) else {
    return;
  };
  assert_ne!(first.span.offset, second.span.offset);
}
