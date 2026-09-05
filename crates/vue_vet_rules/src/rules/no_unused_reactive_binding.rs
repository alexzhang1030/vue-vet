use std::collections::BTreeSet;

use vue_vet_core::{Confidence, ReactiveBindingKind, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::{script_binding_at, static_template_ref_names, used_reactive_names};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-unused-reactive-binding",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-unused-reactive-binding",
};

pub(super) struct NoUnusedReactiveBinding;

pub(super) static RULE: NoUnusedReactiveBinding = NoUnusedReactiveBinding;

impl Rule for NoUnusedReactiveBinding {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    // Cross-fact aggregation across template + scopes + edges + script reads.
    let template_ref_names: BTreeSet<&str> =
      static_template_ref_names(&context.template().elements)
        .filter(|value| !value.is_empty())
        .collect();
    for block in &context.script().blocks {
      let graph = &block.reactivity_graph;
      let mut used = used_reactive_names(graph);
      used.extend(template_ref_names.iter().copied());
      for binding in &graph.bindings {
        if !is_local_value_binding(binding.kind) || used.contains(binding.name.as_str()) {
          continue;
        }
        // Span-match so an exported outer `count` cannot hide an inner unused
        // `count`. Graph/template uses stay name-based (`used_reactive_names`).
        let Some(local) = script_binding_at(block, &binding.name, binding.span) else {
          continue;
        };
        if local.exported {
          continue;
        }
        if local.reads != 0 {
          continue;
        }
        let kind_label = binding_kind_label(binding.kind);
        context.report(
          self.meta(),
          binding.span,
          format!(
            "reactive binding `{}` ({kind_label}) is never read in script or template",
            binding.name
          ),
          Some(
            "Remove the unused binding, or read it from a tracking scope, template expression, or other script use."
              .into(),
          ),
        );
      }
    }
  }
}

const fn is_local_value_binding(kind: ReactiveBindingKind) -> bool {
  matches!(
    kind,
    ReactiveBindingKind::Ref
      | ReactiveBindingKind::ShallowRef
      | ReactiveBindingKind::Computed
      | ReactiveBindingKind::Reactive
      | ReactiveBindingKind::ShallowReactive
      | ReactiveBindingKind::Readonly
      | ReactiveBindingKind::ShallowReadonly
      | ReactiveBindingKind::CustomRef
  )
}

const fn binding_kind_label(kind: ReactiveBindingKind) -> &'static str {
  match kind {
    ReactiveBindingKind::Ref => "ref",
    ReactiveBindingKind::ShallowRef => "shallowRef",
    ReactiveBindingKind::Computed => "computed",
    ReactiveBindingKind::Reactive => "reactive",
    ReactiveBindingKind::ShallowReactive => "shallowReactive",
    ReactiveBindingKind::Readonly => "readonly",
    ReactiveBindingKind::ShallowReadonly => "shallowReadonly",
    ReactiveBindingKind::CustomRef => "customRef",
    ReactiveBindingKind::ToRef => "toRef",
    ReactiveBindingKind::TemplateRef => "useTemplateRef",
    ReactiveBindingKind::ModelRef => "defineModel",
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;
  use std::sync::Arc;

  use vue_vet_core::{
    ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, RuleRegistry, ScriptBindingFact,
    ScriptBlockFacts, ScriptFacts, ScriptKind, SourceSpan, TemplateFacts, TemplateReactiveReadFact,
  };

  use super::RULE;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 5, line: 1, column: offset.saturating_add(1) }
  }

  fn run(script: &ScriptFacts) -> Vec<vue_vet_core::Diagnostic> {
    RuleRegistry::new(vec![&RULE]).run(
      Path::new("shadow.ts"),
      "",
      &TemplateFacts::default(),
      script,
    )
  }

  fn block(bindings: Vec<ScriptBindingFact>, graph: ReactivityGraph) -> ScriptBlockFacts {
    ScriptBlockFacts {
      kind: ScriptKind::Script,
      language: "ts".into(),
      imports: Vec::new(),
      bindings,
      calls: Vec::new(),
      member_writes: Vec::new(),
      destructures: Vec::new(),
      top_level_await_ends: Vec::new(),
      operands: Vec::new(),
      reactivity_graph: Arc::new(graph),
    }
  }

  #[test]
  fn exported_outer_does_not_hide_inner_unused_at_later_span() {
    let mut graph = ReactivityGraph::default();
    graph.bindings.push(ReactiveBindingFact {
      name: "count".into(),
      kind: ReactiveBindingKind::Ref,
      initialized_with_null: false,
      alias_of: None,
      span: span(1),
    });
    graph.bindings.push(ReactiveBindingFact {
      name: "count".into(),
      kind: ReactiveBindingKind::Ref,
      initialized_with_null: false,
      alias_of: None,
      span: span(9),
    });
    let script = ScriptFacts {
      blocks: vec![block(
        vec![
          ScriptBindingFact {
            name: "count".into(),
            reads: 0,
            writes: 0,
            span: span(1),
            exported: true,
          },
          ScriptBindingFact {
            name: "count".into(),
            reads: 0,
            writes: 0,
            span: span(9),
            exported: false,
          },
        ],
        graph,
      )],
    };
    let diagnostics = run(&script);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics.first().map(|diagnostic| diagnostic.span.offset), Some(9));
  }

  #[test]
  fn template_use_keeps_later_top_level_count_quiet() {
    let mut graph = ReactivityGraph::default();
    graph.bindings.push(ReactiveBindingFact {
      name: "count".into(),
      kind: ReactiveBindingKind::Ref,
      initialized_with_null: false,
      alias_of: None,
      span: span(9),
    });
    graph.template_reads.push(TemplateReactiveReadFact {
      binding: "count".into(),
      span: span(20),
      surface: "text".into(),
    });
    let script = ScriptFacts {
      blocks: vec![block(
        vec![
          ScriptBindingFact {
            name: "count".into(),
            reads: 0,
            writes: 0,
            span: span(1),
            exported: false,
          },
          ScriptBindingFact {
            name: "count".into(),
            reads: 0,
            writes: 0,
            span: span(9),
            exported: false,
          },
        ],
        graph,
      )],
    };
    let diagnostics = run(&script);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
  }
}
