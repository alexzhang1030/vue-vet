use super::helpers::*;

#[test]
fn builds_computed_dependency_edges() {
  let graph = graph(
    "import { computed, ref } from 'vue';\n\
     const source = ref(1);\n\
     const doubled = computed(() => source.value * 2);",
  );
  assert!(
    graph.edges.iter().any(|edge| {
      edge.kind == ReactiveDependencyKind::Computed && edge.from == "doubled" && edge.to == "source"
    }),
    "computed scopes must invert into depends-on edges"
  );
}

#[test]
fn joins_template_reads_onto_script_bindings() {
  let mut graph = graph("import { ref } from 'vue'; const count = ref(0);");
  let Some(binding_span) = graph.bindings.first().map(|binding| binding.span.clone()) else {
    assert!(!graph.bindings.is_empty(), "count binding missing");
    return;
  };
  let template = TemplateFacts {
    elements: vec![TemplateElementFact {
      tag: "div".into(),
      span: binding_span.clone(),
      attributes: Vec::new(),
      directives: vec![TemplateDirectiveFact {
        name: "if".into(),
        raw_name: "v-if".into(),
        argument: None,
        expression: Some("count > 0".into()),
        modifiers: Vec::new(),
        span: binding_span.clone(),
      }],
      has_children: false,
      has_accessible_content: false,
      has_labelable_descendant: false,
      has_label_ancestor: false,
      has_accessible_name_ancestor: false,
    }],
    expressions: vec![vue_vet_core::TemplateExpressionFact {
      surface: "if".into(),
      expression: "count > 0".into(),
      span: binding_span,
      identifiers: Some(vec!["count".into()]),
    }],
  };
  graph.join_template_reads(&template);
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "count" && read.surface == "if"),
    "template v-if expressions must join onto reactive bindings"
  );
  assert!(
    graph
      .edges
      .iter()
      .any(|edge| edge.kind == ReactiveDependencyKind::Template && edge.to == "count"),
    "template joins must appear in the inverted edge list"
  );
}

#[test]
fn dependency_edges_include_span_qualified_to_id() {
  let graph = graph(
    "import { ref, computed } from 'vue'; const source = ref(1); const doubled = computed(() => source.value * 2);",
  );
  let edge = graph.edges.iter().find(|edge| {
    edge.kind == ReactiveDependencyKind::Computed && edge.from == "doubled" && edge.to == "source"
  });
  assert!(
    edge.is_some_and(|edge| {
      edge.to_id.as_deref().is_some_and(|id| id.starts_with("source@"))
        && edge.to_identity().split('@').next() == Some("source")
    }),
    "anonymous traces keep name@offset to_id; got {:?}",
    edge.map(|edge| &edge.to_id)
  );
}

#[test]
fn module_traces_qualify_to_id_with_module_prefix() {
  let modules = [ModuleSource::standalone(
    "producer.ts",
    "import { ref, computed } from 'vue'; export const source = ref(1); export const doubled = computed(() => source.value * 2);",
    "ts",
    ScriptKind::Script,
  )];
  let traced = traced_modules(&modules, &[]);
  let producer = traced.iter().find(|module| module.id == "producer.ts");
  let edge = producer.and_then(|module| {
    module.graph.edges.iter().find(|edge| {
      edge.kind == ReactiveDependencyKind::Computed && edge.from == "doubled" && edge.to == "source"
    })
  });
  assert!(
    edge.is_some_and(|edge| {
      edge.to_id.as_deref().is_some_and(|id| id.starts_with("producer.ts:source@"))
    }),
    "v8 module traces must prefix to_id with module id; got {:?}",
    edge.map(|edge| &edge.to_id)
  );
}

#[test]
fn dependency_edges_carry_member_property_for_props_bag() {
  let graph = graph(
    "import { computed } from 'vue'; const props = defineProps<{ count: number; mode: string }>(); const label = computed(() => props.count + props.mode);",
  );
  assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
  let count = graph.edges.iter().find(|edge| {
    edge.from == "label" && edge.to == "props" && edge.property.as_deref() == Some("count")
  });
  let mode = graph.edges.iter().find(|edge| {
    edge.from == "label" && edge.to == "props" && edge.property.as_deref() == Some("mode")
  });
  assert!(
    count.is_some_and(|edge| edge.to_path() == "props.count"),
    "v7 edges must carry property for props.count; got {:?}",
    graph.edges
  );
  assert!(
    mode.is_some_and(|edge| edge.to_path() == "props.mode"),
    "v7 edges must carry property for props.mode; got {:?}",
    graph.edges
  );
}
