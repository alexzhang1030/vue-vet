//! Shared fact queries for Vue Vet built-in and practice rules.
//!
//! Rules consume Vue Vet-owned facts only. This crate does not depend on Vize
//! or Oxc types. It is a workspace-internal query layer, not a new rule pack
//! and not part of the published `vue_vet_core` contract.

mod blocks;
mod graph;
mod reads;

pub use blocks::{
  block_calls, extra_setup_calls, first_top_level_await_end, is_setup_block, script_block,
  script_has_call, setup_blocks, setup_calls_after_first_top_level_await,
};
pub use graph::{reactive_binding, script_binding, used_reactive_names};
pub use reads::{
  binding_path, effect_family, guard_path, has_prior_unconditional_read, is_readonly_kind,
  member_path, same_target, unconditional_self_triggers, unguarded_conditional_reads, write_path,
};

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use vue_vet_core::{
    ReactiveBindingFact, ReactiveBindingKind, ReactiveDependencyEdge, ReactiveDependencyKind,
    ReactiveReadFact, ReactiveReadKind, ReactiveWriteFact, ReactivityGraph, ScriptBindingFact,
    ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind, SourceSpan,
    TemplateReactiveReadFact, TrackingScopeFact, TrackingScopeKind,
  };

  use super::*;

  fn span_at(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 1, line: 1, column: offset.saturating_add(1) }
  }

  fn call(callee: &str, offset: usize) -> ScriptCallFact {
    ScriptCallFact {
      callee: callee.into(),
      assigned_to: None,
      resolved_import: None,
      argument_identifiers: Vec::new(),
      span: span_at(offset),
    }
  }

  fn setup_block(
    calls: Vec<ScriptCallFact>,
    await_ends: Vec<usize>,
    graph: ReactivityGraph,
  ) -> ScriptBlockFacts {
    ScriptBlockFacts {
      kind: ScriptKind::Setup,
      language: "ts".into(),
      imports: Vec::new(),
      bindings: Vec::new(),
      calls,
      member_writes: Vec::new(),
      destructures: Vec::new(),
      top_level_await_ends: await_ends,
      operands: Vec::new(),
      reactivity_graph: Arc::new(graph),
    }
  }

  fn ordinary_block(calls: Vec<ScriptCallFact>) -> ScriptBlockFacts {
    let mut block = setup_block(calls, Vec::new(), ReactivityGraph::default());
    block.kind = ScriptKind::Script;
    block
  }

  fn read(
    binding: &str,
    property: Option<&str>,
    kind: ReactiveReadKind,
    offset: usize,
  ) -> ReactiveReadFact {
    ReactiveReadFact {
      binding: binding.into(),
      property: property.map(str::to_owned),
      kind,
      guards: Vec::new(),
      guarded_by: None,
      span: span_at(offset),
    }
  }

  #[test]
  fn setup_blocks_skip_ordinary_script() {
    let script = ScriptFacts {
      blocks: vec![
        ordinary_block(vec![call("defineProps", 1)]),
        setup_block(vec![call("defineProps", 10)], Vec::new(), ReactivityGraph::default()),
      ],
    };
    let kinds: Vec<ScriptKind> = setup_blocks(&script).map(|block| block.kind).collect();
    assert_eq!(kinds, vec![ScriptKind::Setup]);
    assert!(script_block(&script, ScriptKind::Setup).is_some_and(is_setup_block));
    assert!(script_block(&script, ScriptKind::Script).is_some_and(|block| !is_setup_block(block)));
  }

  #[test]
  fn after_await_uses_first_end_and_includes_boundary() {
    let script = ScriptFacts {
      blocks: vec![
        setup_block(
          vec![call("onMounted", 4), call("onMounted", 10), call("watch", 12)],
          vec![10, 40],
          ReactivityGraph::default(),
        ),
        ordinary_block(vec![call("onMounted", 80)]),
        setup_block(vec![call("onMounted", 3)], Vec::new(), ReactivityGraph::default()),
      ],
    };
    let hits: Vec<usize> = setup_calls_after_first_top_level_await(&script, "onMounted")
      .map(|call| call.span.offset)
      .collect();
    assert_eq!(hits, vec![10], "only setup onMounted at or after the first await end");
  }

  #[test]
  fn extra_setup_calls_skip_first_match_per_setup_block() {
    let script = ScriptFacts {
      blocks: vec![
        setup_block(
          vec![call("defineProps", 1), call("defineEmits", 2), call("defineProps", 3)],
          Vec::new(),
          ReactivityGraph::default(),
        ),
        ordinary_block(vec![call("defineProps", 4), call("defineProps", 5)]),
      ],
    };
    let extras: Vec<usize> =
      extra_setup_calls(&script, "defineProps").map(|call| call.span.offset).collect();
    assert_eq!(extras, vec![3]);
  }

  #[test]
  fn prior_unconditional_matches_earlier_same_target_only() {
    let same_later = read("count", Some("value"), ReactiveReadKind::Conditional, 8);
    let other = read("other", Some("value"), ReactiveReadKind::Conditional, 9);
    let earlier_conditional = read("count", Some("value"), ReactiveReadKind::Conditional, 1);
    let reads = vec![
      read("count", Some("value"), ReactiveReadKind::Unconditional, 2),
      same_later.clone(),
      other.clone(),
      earlier_conditional.clone(),
    ];
    assert!(has_prior_unconditional_read(&reads, &same_later));
    assert!(!has_prior_unconditional_read(&reads, &other));
    assert!(!has_prior_unconditional_read(&reads, &earlier_conditional));
    let unguarded: Vec<usize> =
      unguarded_conditional_reads(&reads).map(|item| item.span.offset).collect();
    assert_eq!(unguarded, vec![9, 1]);
  }

  #[test]
  fn used_reactive_names_collect_template_scope_and_edge_targets() {
    let mut graph = ReactivityGraph::default();
    graph.template_reads.push(TemplateReactiveReadFact {
      binding: "from_template".into(),
      span: span_at(1),
      surface: "text".into(),
    });
    graph.scopes.push(TrackingScopeFact {
      kind: TrackingScopeKind::WatchEffect,
      callee: "watchEffect".into(),
      span: span_at(2),
      reads: vec![read("from_read", None, ReactiveReadKind::Unconditional, 3)],
      writes: vec![ReactiveWriteFact {
        binding: "from_write".into(),
        property: Some("value".into()),
        span: span_at(4),
      }],
      assignment_only: false,
      binding: None,
      uncertain_accesses: Vec::new(),
    });
    graph.edges.push(ReactiveDependencyEdge {
      from: "computed".into(),
      to: "from_edge".into(),
      to_id: None,
      property: None,
      kind: ReactiveDependencyKind::Computed,
      span: span_at(5),
    });
    let names = used_reactive_names(&graph);
    assert!(names.contains("from_template"));
    assert!(names.contains("from_read"));
    assert!(names.contains("from_write"));
    assert!(names.contains("from_edge"));
    assert!(!names.contains("computed"));
  }

  #[test]
  fn binding_lookups_and_paths_match_historical_formatting() {
    let mut graph = ReactivityGraph::default();
    graph.bindings.push(ReactiveBindingFact {
      name: "state".into(),
      kind: ReactiveBindingKind::Readonly,
      initialized_with_null: false,
      span: span_at(1),
    });
    let mut block = setup_block(Vec::new(), Vec::new(), graph);
    block.bindings.push(ScriptBindingFact {
      name: "state".into(),
      reads: 0,
      writes: 0,
      span: span_at(1),
    });
    assert!(
      reactive_binding(&block, "state").is_some_and(|binding| is_readonly_kind(binding.kind))
    );
    assert!(script_binding(&block, "state").is_some());
    assert!(reactive_binding(&block, "missing").is_none());
    let read = read("count", Some("value"), ReactiveReadKind::Unconditional, 0);
    assert_eq!(binding_path(&read), "count.value");
    assert_eq!(member_path("count", None), "count");
  }

  #[test]
  fn script_has_call_sees_any_block() {
    let script = ScriptFacts { blocks: vec![ordinary_block(vec![call("pauseTracking", 1)])] };
    assert!(script_has_call(&script, "pauseTracking"));
    assert!(!script_has_call(&script, "enableTracking"));
  }
}
