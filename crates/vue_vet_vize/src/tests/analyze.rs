use std::{path::Path, sync::Arc};

use super::super::*;
use vue_vet_core::{Diagnostic, RuleEnvironment, RuleRegistry};
use vue_vet_practice::practice_rules;
use vue_vet_rules::builtin_rules;

#[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
fn analyze_for_test(path: &Path, source: &str) -> Vec<Diagnostic> {
  match analyze_sfc_with_facts(path, source) {
    Ok(analysis) => {
      let mut rules = builtin_rules();
      rules.extend(practice_rules());
      RuleRegistry::new(rules).run_with_environment(
        path,
        source,
        &analysis.facts.template,
        &analysis.facts.script,
        RuleEnvironment::default(),
      )
    }
    Err(error) => panic!("analysis unexpectedly failed: {error}"),
  }
}

fn facts_for_test(path: &Path, source: &str) -> SfcFacts {
  analysis_for_test(path, source).facts
}

#[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
fn analysis_for_test(path: &Path, source: &str) -> AnalyzedSfc {
  match analyze_sfc_with_facts(path, source) {
    Ok(analysis) => analysis,
    Err(error) => panic!("analysis unexpectedly failed: {error}"),
  }
}

#[test]
fn style_only_edit_reuses_template_and_script_blocks() {
  let path = Path::new("Reuse.vue");
  let base = concat!(
    "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
    "<template><p>{{ count }}</p></template>\n",
    "<style>.a { color: red; }</style>\n",
  );
  let first = analysis_for_test(path, base);
  let style_only = concat!(
    "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
    "<template><p>{{ count }}</p></template>\n",
    "<style>.a { color: blue; }</style>\n",
  );
  let second = analysis_reusing_for_test(path, style_only, &first);
  assert_eq!(first.revisions, second.revisions);
  assert_eq!(first.facts, second.facts);
  assert!(
    second.module_source.as_ref().and_then(ModuleSource::module_summary).is_some(),
    "reused analysis must keep module summary"
  );
}

#[test]
fn style_v_bind_ident_joins_computed_binding() {
  let source = concat!(
    "<script setup lang=\"ts\">\n",
    "import { computed, ref } from 'vue'\n",
    "const count = ref(0)\n",
    "const color = computed(() => (count.value > 0 ? 'red' : 'blue'))\n",
    "</script>\n",
    "<template><p>{{ count }}</p></template>\n",
    "<style>.text { color: v-bind(color); background: v-bind('color'); }</style>\n",
  );
  let facts = facts_for_test(Path::new("StyleBind.vue"), source);
  let style_exprs: Vec<_> =
    facts.template.expressions.iter().filter(|expression| expression.surface == "style").collect();
  assert_eq!(style_exprs.len(), 2, "quoted and unquoted v-bind(color) must both extract");
  assert!(
    style_exprs.iter().all(|expression| {
      expression.identifiers.as_ref().is_some_and(|idents| idents == &["color".to_string()])
    }),
    "style v-bind must resolve the ident; got {style_exprs:?}"
  );
  assert!(
    facts.script.blocks.first().is_some_and(|block| {
      block
        .reactivity_graph
        .template_reads
        .iter()
        .any(|read| read.binding == "color" && read.surface == "style")
    }),
    "CSS v-bind(color) must join the computed; blocks={:?}",
    facts.script.blocks
  );
  let diagnostics = analyze_for_test(Path::new("StyleBind.vue"), source);
  assert!(
    diagnostics
      .iter()
      .all(|diagnostic| { diagnostic.rule_id != "vue-vet/reactivity/no-unused-computed-binding" }),
    "computed used only in CSS v-bind must not be unused; {diagnostics:?}"
  );
}

#[test]
fn style_v_bind_skips_complex_expressions() {
  let source = concat!(
    "<script setup lang=\"ts\">\n",
    "import { computed, ref } from 'vue'\n",
    "const height = computed(() => 10)\n",
    "const theme = computed(() => ({ color: 'red' }))\n",
    "</script>\n",
    "<template><p></p></template>\n",
    "<style>.box { height: v-bind(\"height + 'px'\"); color: v-bind(theme.color); }</style>\n",
  );
  let facts = facts_for_test(Path::new("StyleComplex.vue"), source);
  assert!(
    facts.template.expressions.iter().all(|expression| expression.surface != "style"),
    "complex CSS v-bind must stay quiet; got {:?}",
    facts.template.expressions
  );
}

#[test]
fn style_only_v_bind_edit_refreshes_template_reads() {
  let path = Path::new("StyleReuse.vue");
  let base = concat!(
    "<script setup lang=\"ts\">\nimport { computed, ref } from 'vue'\n",
    "const count = ref(0)\nconst color = computed(() => 'red')\n",
    "const size = computed(() => '1rem')\n</script>\n",
    "<template><p>{{ count }}</p></template>\n",
    "<style>.a { color: v-bind(color); }</style>\n",
  );
  let first = analysis_for_test(path, base);
  let swapped = concat!(
    "<script setup lang=\"ts\">\nimport { computed, ref } from 'vue'\n",
    "const count = ref(0)\nconst color = computed(() => 'red')\n",
    "const size = computed(() => '1rem')\n</script>\n",
    "<template><p>{{ count }}</p></template>\n",
    "<style>.a { font-size: v-bind(size); }</style>\n",
  );
  let second = analysis_reusing_for_test(path, swapped, &first);
  assert_eq!(first.revisions, second.revisions);
  let first_graph = first.facts.script.blocks.first().map(|block| &block.reactivity_graph);
  let second_graph = second.facts.script.blocks.first().map(|block| &block.reactivity_graph);
  assert!(
    first_graph.is_some_and(|graph| {
      graph.template_reads.iter().any(|read| read.binding == "color" && read.surface == "style")
    }),
    "first analysis must join color"
  );
  assert!(
    second_graph.is_some_and(|graph| {
      graph.template_reads.iter().any(|read| read.binding == "size" && read.surface == "style")
        && graph
          .template_reads
          .iter()
          .all(|read| read.binding != "color" || read.surface != "style")
    }),
    "style-only v-bind swap must re-join size and drop color; second={second_graph:?}"
  );
}

#[test]
fn template_only_edit_keeps_script_fingerprint() {
  let path = Path::new("Tpl.vue");
  let base = concat!(
    "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
    "<template><p>{{ count }}</p></template>\n",
  );
  let first = analysis_for_test(path, base);
  let template_only = concat!(
    "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
    "<template><span>{{ count }}</span></template>\n",
  );
  let second = analysis_reusing_for_test(path, template_only, &first);
  assert_eq!(first.revisions.script_setup, second.revisions.script_setup);
  assert_ne!(first.revisions.template, second.revisions.template);
  assert_ne!(first.facts.template, second.facts.template);
}

#[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
fn analysis_reusing_for_test(path: &Path, source: &str, previous: &AnalyzedSfc) -> AnalyzedSfc {
  match analyze_sfc_facts_reusing(path, source, Some(previous)) {
    Ok(analysis) => analysis,
    Err(error) => panic!("analysis unexpectedly failed: {error}"),
  }
}

#[test]
fn label_facts_mark_nested_labelable_controls() {
  let source = "<template>\n  <label>\n    <input type=\"text\">\n  </label>\n  <label>Name</label>\n</template>";
  let facts = facts_for_test(Path::new("Label.vue"), source);
  let labels = facts.template.elements.iter().filter(|el| el.tag == "label").collect::<Vec<_>>();
  assert_eq!(labels.len(), 2, "expected two label elements");
  if let [nested, text_only] = labels.as_slice() {
    assert!(nested.has_labelable_descendant, "nested input must set has_labelable_descendant");
    assert!(!text_only.has_labelable_descendant, "text-only label must stay clear");
  }
  let input = facts.template.elements.iter().find(|el| el.tag == "input");
  assert!(
    input.is_some_and(|el| el.has_label_ancestor),
    "nested input must set has_label_ancestor"
  );
}

#[test]
fn reports_v_html_at_the_source_location() {
  let source = "<template>\n  <div v-html=\"html\" />\n</template>";
  let diagnostics = analyze_for_test(Path::new("Unsafe.vue"), source);

  assert_eq!(diagnostics.len(), 1, "expected exactly one v-html diagnostic");
  assert_eq!(
    diagnostics.first().map(|diagnostic| diagnostic.rule_id.as_str()),
    Some("vue-vet/security/no-v-html"),
    "expected the stable no-v-html rule ID"
  );
  assert_eq!(diagnostics.first().map(|diagnostic| diagnostic.span.line), Some(2));
  assert_eq!(diagnostics.first().map(|diagnostic| diagnostic.span.column), Some(8));
}

#[test]
fn ignores_the_same_text_outside_the_template() {
  let source = "<script setup>\nconst note = 'v-html'\n</script>\n<template><div /></template>";
  let diagnostics = analyze_for_test(Path::new("Safe.vue"), source);

  assert!(diagnostics.is_empty(), "script text must not be treated as a template directive");
}

#[test]
fn ignores_comments_text_and_similar_attribute_names() {
  let source = r#"<template>
<!-- <div v-html="html" /> -->
<p>write v-html only when content is trusted</p>
<div data-v-html="html" />
</template>"#;
  let diagnostics = analyze_for_test(Path::new("Safe.vue"), source);

  assert!(diagnostics.is_empty(), "non-directive text and attributes must not produce findings");
}

#[test]
fn joins_template_interpolation_and_directives_onto_script_bindings() {
  let source = r#"<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const label = ref('x')
const user = ref({ name: 'a' })
const items = ref([1])
const name = ref('shadow')
const item = ref('script-item')
</script>
<template>
<div v-if="count > 0" :title="user.name">{{ label }}</div>
<li v-for="item in items" :key="item">{{ item }}</li>
<p>{{ item }}</p>
<template #default="{ value }">
  <span>{{ value }} · {{ label }}</span>
</template>
</template>"#;
  let facts = facts_for_test(Path::new("Join.vue"), source);
  let Some(graph) = facts.script.blocks.first().map(|block| &block.reactivity_graph) else {
    assert!(!facts.script.blocks.is_empty(), "script setup block must be analyzed");
    return;
  };

  assert!(
    facts.template.expressions.iter().any(|expression| expression.surface == "interpolation"),
    "Vize interpolations must be extracted as expression surfaces"
  );
  assert!(
    facts.template.expressions.iter().any(|expression| {
      expression.surface == "title"
        && expression
          .identifiers
          .as_ref()
          .is_some_and(|identifiers| identifiers.iter().any(|identifier| identifier == "user"))
        && expression
          .identifiers
          .as_ref()
          .is_some_and(|identifiers| !identifiers.iter().any(|identifier| identifier == "name"))
    }),
    "Oxc AST extraction must keep member objects and drop static property names"
  );
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "count" && read.surface == "if"),
    "v-if expression must join onto the count binding"
  );
  assert!(
    graph
      .template_reads
      .iter()
      .any(|read| read.binding == "label" && read.surface == "interpolation"),
    "mustache interpolation must join onto the label binding"
  );
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "user" && read.surface == "title"),
    "v-bind member expression must join the object binding"
  );
  assert!(
    !graph.template_reads.iter().any(|read| read.binding == "name"),
    "static property `name` must not join a same-named reactive binding"
  );
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "items" && read.surface == "for"),
    "v-for iterable source must join onto items"
  );
  // Inside v-for, `item` is a template-local alias even when script also has `item`.
  assert!(
    !graph
      .template_reads
      .iter()
      .any(|read| read.binding == "item" && matches!(read.surface.as_str(), "key" | "for")),
    "v-for alias uses must not join the script item binding"
  );
  assert!(
    facts.template.expressions.iter().any(|expression| {
      expression.surface == "key"
        && expression.identifiers.as_ref().is_some_and(std::vec::Vec::is_empty)
    }),
    "`:key=\"item\"` free reads must resolve empty under the v-for alias scope"
  );
  let item_interpolation_joins = graph
    .template_reads
    .iter()
    .filter(|read| read.binding == "item" && read.surface == "interpolation")
    .count();
  assert_eq!(
    item_interpolation_joins, 1,
    "only the outer `{{{{ item }}}}` outside v-for should join the script item binding"
  );
  assert!(
    !graph.template_reads.iter().any(|read| read.binding == "value"),
    "slot prop aliases must not join script bindings"
  );
  assert!(
    facts.template.expressions.iter().any(|expression| {
      expression.surface == "interpolation"
        && expression.identifiers.as_ref().is_some_and(|identifiers| {
          identifiers.iter().any(|identifier| identifier == "label")
            && !identifiers.iter().any(|identifier| identifier == "value")
        })
    }),
    "slot body may read script bindings while dropping slot prop aliases"
  );
  // Expression spans must be absolute SFC offsets (not template-relative zeros).
  assert!(
    facts.template.expressions.iter().all(|expression| expression.span.offset > 0),
    "expression spans must use original SFC offsets via template.loc.start + expr.loc"
  );
}

#[test]
fn define_props_computed_and_template_join_end_to_end() {
  let source = r#"<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ count: number; label: string }>()
const doubled = computed(() => props.count * 2)
</script>
<template>
<p v-if="props.count > 0">{{ props.label }} · {{ doubled }}</p>
</template>"#;
  let facts = facts_for_test(Path::new("PropsCard.vue"), source);
  let Some(graph) = facts.script.blocks.first().map(|block| &block.reactivity_graph) else {
    assert!(!facts.script.blocks.is_empty(), "script setup must be analyzed");
    return;
  };
  assert!(
    graph.bindings.iter().any(|binding| {
      binding.name == "props" && binding.kind == vue_vet_core::ReactiveBindingKind::Reactive
    }),
    "defineProps must seed a reactive props binding"
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == vue_vet_core::TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("count"))
    }),
    "computed must track props.count"
  );
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "props" && read.surface == "if"),
    "template v-if must join props"
  );
  assert!(
    graph
      .template_reads
      .iter()
      .any(|read| read.binding == "props" && read.surface == "interpolation"),
    "template must join props member reads onto the props binding"
  );
  assert!(
    graph
      .template_reads
      .iter()
      .any(|read| read.binding == "doubled" && read.surface == "interpolation"),
    "template must join the computed binding"
  );
  assert!(
    graph.edges.iter().any(|edge| {
      edge.kind == vue_vet_core::ReactiveDependencyKind::Template && edge.to == "props"
    }),
    "template edges must target props"
  );
}

#[test]
fn dual_script_emits_setup_and_ordinary_module_sources() {
  let source = r#"<script lang="ts">
import { ref } from 'vue'
export function useShared() {
const shared = ref(0)
return { shared }
}
</script>
<script setup lang="ts">
import { watchEffect } from 'vue'
import { useShared } from './unused'
const bag = useShared()
watchEffect(() => { void bag.shared.value })
</script>
<template><p>{{ bag.shared }}</p></template>
"#;
  let analysis = analysis_for_test(Path::new("Dual.vue"), source);
  assert!(
    analysis.module_source.as_ref().is_some_and(|module| {
      module.kind == ScriptKind::Setup && module.id == "Dual.vue" && module.source.contains("bag")
    }),
    "primary module source must be script setup"
  );
  assert!(
    analysis.ordinary_module_source.as_ref().is_some_and(|module| {
      module.kind == ScriptKind::Script
        && module.id == "Dual.vue#script"
        && module.source.contains("useShared")
    }),
    "dual ordinary companion must use #script id; got {:?}",
    analysis.ordinary_module_source.as_ref().map(|module| (
      &module.id,
      module.kind,
      &module.source
    ))
  );
  assert_eq!(
    analysis.facts.script.blocks.len(),
    2,
    "both script blocks must be analyzed for rules"
  );
}

#[test]
#[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
fn same_file_composable_instance_tracks_and_joins_template_in_sfc() {
  use std::path::PathBuf;
  use vue_vet_project::{ProjectFile, build_project_graph};

  let sfc = r#"<script setup lang="ts">
import { ref, watchEffect } from 'vue'
function useSignal() {
const signal = ref(0)
return { signal }
}
const bag = useSignal()
watchEffect(() => { void bag.signal.value })
</script>
<template>
<p>{{ bag.signal }}</p>
</template>
"#;
  let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("../../target")
    .join(format!("vize-same-file-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&temp);
  if let Err(error) = std::fs::create_dir_all(&temp) {
    panic!("failed to create temp project: {error}");
  }
  if let Err(error) = std::fs::write(temp.join("LocalBag.vue"), sfc) {
    panic!("failed to write LocalBag.vue: {error}");
  }
  // Shipped Vize path: analyze_sfc_with_facts → per-block graph + template join.
  let analysis = analysis_for_test(Path::new("LocalBag.vue"), sfc);
  let vize_ok = analysis.facts.script.blocks.first().is_some_and(|block| {
    let graph = &block.reactivity_graph;
    graph.composable_instances.contains_key("bag")
      && graph.effects.iter().any(|effect| {
        effect.reads.iter().any(|read| {
          read.binding == "signal" && read.kind == vue_vet_core::ReactiveReadKind::Unconditional
        })
      })
      && graph
        .template_reads
        .iter()
        .any(|read| read.binding == "signal" && read.surface == "interpolation")
  });
  assert!(
    vize_ok,
    "Vize SFC path must track same-file bag.signal and join template; blocks={:?}",
    analysis
      .facts
      .script
      .blocks
      .iter()
      .map(|block| {
        (
          block.reactivity_graph.composable_instances.clone(),
          block
            .reactivity_graph
            .effects
            .iter()
            .flat_map(|effect| effect.reads.iter().map(|read| read.binding.clone()))
            .collect::<Vec<_>>(),
          block.reactivity_graph.template_reads.clone(),
        )
      })
      .collect::<Vec<_>>()
  );

  // Project re-trace path (single module, no cross-file seed).
  assert!(analysis.module_source.is_some(), "script setup must produce a project module source");
  if let Some(mut module) = analysis.module_source {
    module.id = "LocalBag.vue".into();
    let files = [ProjectFile {
      path: PathBuf::from("LocalBag.vue").into(),
      source_len: sfc.len(),
      facts: analysis.facts.into(),
      module_source: Some(Arc::new(module)),
      ordinary_module_source: None,
    }];
    let project = build_project_graph(&temp, &files);
    let _ignored = std::fs::remove_dir_all(&temp);
    let page = project.module_reactivity.iter().find(|module| module.id == "LocalBag.vue");
    assert!(
      page.is_some_and(|module| {
        module.graph.composable_instances.contains_key("bag")
          && module
            .graph
            .effects
            .iter()
            .any(|effect| effect.reads.iter().any(|read| read.binding == "signal"))
          && module
            .graph
            .template_reads
            .iter()
            .any(|read| read.binding == "signal" && read.surface == "interpolation")
      }),
      "project re-trace must keep same-file instance + template join; got {:?}",
      page.map(|module| {
        (module.graph.composable_instances.clone(), module.graph.template_reads.clone())
      })
    );
  }
}

#[test]
#[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
fn composable_instance_member_joins_template_after_module_seeds() {
  use std::path::PathBuf;
  use vue_vet_project::{ProjectFile, build_project_graph};
  use vue_vet_reactivity::ModuleSource;

  let producer = "import { ref } from 'vue'; export function useSignal() { const signal = ref(0); return { signal }; }";
  let sfc = r#"<script setup lang="ts">
import { watchEffect } from 'vue'
import { useSignal } from './useSignal'
const bag = useSignal()
watchEffect(() => { void bag.signal.value })
</script>
<template>
<p>{{ bag.signal }}</p>
<p>{{ bag?.signal }}</p>
</template>
"#;
  let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("../../target")
    .join(format!("vize-composable-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&temp);
  if let Err(error) = std::fs::create_dir_all(&temp) {
    panic!("failed to create temp project: {error}");
  }
  if let Err(error) = std::fs::write(temp.join("useSignal.ts"), producer) {
    panic!("failed to write useSignal.ts: {error}");
  }
  if let Err(error) = std::fs::write(temp.join("App.vue"), sfc) {
    panic!("failed to write App.vue: {error}");
  }
  let analysis = analysis_for_test(Path::new("App.vue"), sfc);
  let files = [
    ProjectFile {
      path: PathBuf::from("useSignal.ts").into(),
      source_len: producer.len(),
      facts: SfcFacts::default().into(),
      module_source: Some(Arc::new(ModuleSource::standalone(
        "useSignal.ts",
        producer,
        "ts",
        ScriptKind::Script,
      ))),
      ordinary_module_source: None,
    },
    ProjectFile {
      path: PathBuf::from("App.vue").into(),
      source_len: sfc.len(),
      facts: analysis.facts.into(),
      module_source: analysis.module_source.map(Arc::new),
      ordinary_module_source: None,
    },
  ];
  let graph = build_project_graph(&temp, &files);
  let _ignored = std::fs::remove_dir_all(&temp);
  assert!(
    graph.reactivity_error.is_none(),
    "module tracing must succeed: {:?}",
    graph.reactivity_error
  );
  let app = graph.module_reactivity.iter().find(|module| module.id == "App.vue");
  assert!(
    app.is_some_and(|module| {
      module.graph.composable_instances.contains_key("bag")
        && module.graph.effects.iter().any(|effect| {
          effect.reads.iter().any(|read| {
            read.binding == "signal" && read.kind == vue_vet_core::ReactiveReadKind::Unconditional
          })
        })
        && module
          .graph
          .template_reads
          .iter()
          .any(|read| read.binding == "signal" && read.surface == "interpolation")
        && module.graph.edges.iter().any(|edge| {
          edge.kind == vue_vet_core::ReactiveDependencyKind::Template && edge.to == "signal"
        })
    }),
    "seeded bag.signal must track in effects and join template {{ bag.signal }}; got {:?}",
    app.map(|module| {
      (
        module.graph.composable_instances.clone(),
        module
          .graph
          .effects
          .iter()
          .flat_map(|effect| effect.reads.iter().map(|read| read.binding.clone()))
          .collect::<Vec<_>>(),
        module.graph.template_reads.clone(),
      )
    })
  );
}

#[test]
#[expect(clippy::panic, reason = "missing module source must fail the extraction test")]
fn exposes_script_setup_module_source_with_sfc_span_mapping() {
  let source = r#"<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
</script>
<template>
<div>{{ count }}</div>
</template>"#;
  let analysis = analysis_for_test(Path::new("pages/Counter.vue"), source);
  let Some(module) = analysis.module_source.as_ref() else {
    panic!("script setup must produce a project module source");
  };
  assert_eq!(module.id, "pages/Counter.vue");
  assert_eq!(module.kind, ScriptKind::Setup);
  assert_eq!(module.language, "ts");
  assert!(module.source.contains("const count = ref(0)"));
  assert!(module.source_offset > 0, "script body offset must be absolute in the SFC");
  assert_eq!(module.span_source.as_ref(), source);
  let body = module
    .span_source
    .get(module.source_offset..module.source_offset.saturating_add(module.source.len()));
  assert_eq!(
    body,
    Some(module.source.as_ref()),
    "extracted script body must be an exact slice of the original SFC at source_offset"
  );
}

#[test]
#[expect(
  clippy::expect_used,
  clippy::panic,
  reason = "fixture IO and analysis failures must fail the integration test"
)]
fn project_graph_uses_vize_module_source_for_seeds() {
  use std::path::PathBuf;
  use vue_vet_project::{ProjectFile, build_project_graph};
  use vue_vet_reactivity::ModuleSource;

  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/module-seeds");
  let app = std::fs::read_to_string(root.join("App.vue")).expect("app fixture");
  let producer =
    std::fs::read_to_string(root.join("composables/useField.ts")).expect("producer fixture");
  let analysis = analysis_for_test(Path::new("App.vue"), &app);
  let Some(module) = analysis.module_source.clone() else {
    panic!("module source missing");
  };
  let files = [
    ProjectFile {
      path: PathBuf::from("App.vue").into(),
      source_len: app.len(),
      facts: analysis.facts.into(),
      module_source: Some(Arc::new({
        let mut module = module;
        module.id = "App.vue".into();
        module
      })),
      ordinary_module_source: None,
    },
    ProjectFile {
      path: PathBuf::from("composables/useField.ts").into(),
      source_len: producer.len(),
      facts: SfcFacts::default().into(),
      module_source: Some(Arc::new(ModuleSource::standalone(
        "composables/useField.ts",
        producer,
        "ts",
        ScriptKind::Script,
      ))),
      ordinary_module_source: None,
    },
  ];
  let graph = build_project_graph(&root, &files);
  let app_mod = graph.module_reactivity.iter().find(|module| module.id == "App.vue");
  assert!(
    app_mod.is_some_and(|module| {
      module.graph.effects.iter().any(|effect| {
        effect.reads.iter().any(|read| {
          read.binding == "title" && read.kind == vue_vet_core::ReactiveReadKind::AfterAwait
        })
      })
    }),
    "Vize module_source through project graph must seed after-await title reads"
  );
}

#[test]
#[expect(
  clippy::expect_used,
  clippy::panic,
  reason = "fixture IO and analysis failures must fail the integration test"
)]
fn prop_flow_fixture_joins_parent_binding_onto_child_props() {
  use std::path::PathBuf;
  use vue_vet_core::ReactiveDependencyKind;
  use vue_vet_project::{ProjectFile, build_project_graph};

  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/prop-flow");
  let mut files = Vec::new();
  for name in ["Parent.vue", "Child.vue", "MultiHop.vue"] {
    let source = std::fs::read_to_string(root.join(name)).expect("fixture");
    let analysis = analysis_for_test(Path::new(name), &source);
    let Some(module) = analysis.module_source.clone() else {
      panic!("module source missing for {name}");
    };
    files.push(ProjectFile {
      path: PathBuf::from(name).into(),
      source_len: source.len(),
      facts: analysis.facts.into(),
      module_source: Some(Arc::new({
        let mut module = module;
        module.id = name.into();
        module
      })),
      ordinary_module_source: None,
    });
  }
  let graph = build_project_graph(&root, &files);
  let child = graph.module_reactivity.iter().find(|module| module.id == "Child.vue");
  let props = child.map(|module| {
    module
      .graph
      .edges
      .iter()
      .filter(|edge| edge.kind == ReactiveDependencyKind::Prop)
      .map(|edge| (edge.property.as_deref(), edge.to.as_str()))
      .collect::<Vec<_>>()
  });
  assert!(
    props.as_ref().is_some_and(|props| {
      props.contains(&(Some("title"), "label"))
        && props.contains(&(Some("modelValue"), "msg"))
        && props.contains(&(Some("subtitle"), "bag"))
        && props.contains(&(Some("count"), "msg"))
    }),
    "prop-flow fixture must emit title/v-model/member/.value Prop edges; got {props:?}"
  );
  assert!(
    child.is_some_and(|module| {
      module.graph.edges.iter().any(|edge| {
        edge.kind == ReactiveDependencyKind::Prop
          && edge.property.as_deref() == Some("subtitle")
          && edge.to == "bag"
          && edge.to_id.as_deref().is_some_and(|id| id.starts_with("MultiHop.vue:bag@"))
      })
    }),
    "MultiHop.vue multi-hop chain must join root binding onto Child props"
  );
  assert!(
    child.is_some_and(|module| {
      module.graph.edges.iter().any(|edge| {
        edge.kind == ReactiveDependencyKind::Prop
          && edge.property.as_deref() == Some("title")
          && edge.to == "bag"
          && edge.to_id.as_deref().is_some_and(|id| id.starts_with("MultiHop.vue:bag@"))
      })
    }),
    "MultiHop.vue optional chain must join root binding onto Child props"
  );
}
