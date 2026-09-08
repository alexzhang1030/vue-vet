//! Neutral Vue source-contract facts from Oxc semantics (issue #224).
//!
//! Proven Vue identity (`info.api`) is a named or namespace import from `vue`,
//! `vue-demi`, `@vue/runtime-core`, `@vue/runtime-dom`, or `@vue/reactivity`,
//! plus a **named** `#imports` specifier whose imported name is a known Vue
//! export. Namespace `#imports`, type-only specifiers, and unknown auto-import
//! names stay unproven. Compiler macros `defineProps` / `defineModel` are
//! setup-only. Actual-Proxy proof for native `structuredClone` is a separate
//! origin discriminator: only `vue` / `@vue/runtime-core` / `@vue/runtime-dom`
//! / `@vue/reactivity`. Named `#imports` and `vue-demi` stay unproved here.
//! Origin uses the indexed import source of resolved proxy constructors only;
//! local and unknown calls skip that lookup. Native `structuredClone` *calls*
//! require a definite static key; unresolved `globalThis` *writes* with a
//! non-literal key poison identity.
//!
//! Replacement findings require a simple `=` of a fresh object/array/`new`
//! built-in collection in the same straight-line block after `watch`.
//! Named `watchEffect` / `watchPostEffect` / `watchSyncEffect` imports keep
//! source indexes empty when every resolved reference is a proven call with
//! fewer than two arguments and no spread. Ordinary sinks and namespace
//! imports keep full indexing.

mod clone_boundary;
mod demand;
mod index;
mod normalization;
mod proof;
mod shape;
mod stats;
mod watch_api;
mod watch_callbacks;

use std::collections::HashMap;

use oxc_ast::{
  AstKind,
  ast::{ArrayExpressionElement, CallExpression, Expression, IdentifierReference},
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  ScriptKind, SourceContractFacts, SourceContractSiteFact, SourceSpan,
  WatchReplacedObjectSourceFact,
};

use crate::facts::source_span;

use index::{CallInfo, Indexes, ObjectProp};
pub use shape::{ContractSink, contract_sink};
use shape::{Shape, ShapeHint, classify_vue_result, collect_vue_imports, is_ref_api, span_key};
use stats::WorkCounter;

pub use stats::SourceContractStats;

const MAX_DEPTH: u8 = 8;

pub(in crate::source_contracts) struct Collector<'a> {
  pub(in crate::source_contracts) semantic: &'a oxc_semantic::Semantic<'a>,
  pub(in crate::source_contracts) line_index: &'a vue_vet_core::LineIndex,
  pub(in crate::source_contracts) sfc_source: &'a str,
  pub(in crate::source_contracts) script_offset: usize,
  pub(in crate::source_contracts) indexes: Indexes,
  pub(in crate::source_contracts) shape_cache: HashMap<SymbolId, Shape>,
  pub(in crate::source_contracts) property_shape: HashMap<(SymbolId, String), Shape>,
  pub(in crate::source_contracts) proxy_proof: HashMap<SymbolId, bool>,
  pub(in crate::source_contracts) facts: SourceContractFacts,
}

pub fn collect_source_contract_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  kind: ScriptKind,
) -> SourceContractFacts {
  collect_source_contract_facts_with_stats(semantic, line_index, sfc_source, script_offset, kind).0
}

pub fn collect_source_contract_facts_with_stats(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  kind: ScriptKind,
) -> (SourceContractFacts, SourceContractStats) {
  collect_prepared(semantic, line_index, sfc_source, script_offset, kind, false)
}

#[cfg(test)]
pub fn collect_source_contract_facts_forced_full(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  kind: ScriptKind,
) -> (SourceContractFacts, SourceContractStats) {
  collect_prepared(semantic, line_index, sfc_source, script_offset, kind, true)
}

fn collect_prepared(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  kind: ScriptKind,
  force_full: bool,
) -> (SourceContractFacts, SourceContractStats) {
  let work = WorkCounter::default();
  let (vue_imports, needs_index) = collect_vue_imports(semantic, &work);
  if !needs_index && !force_full {
    return (SourceContractFacts::default(), work.snapshot());
  }
  let mut collector = Collector {
    semantic,
    line_index,
    sfc_source,
    script_offset,
    indexes: Indexes::build(
      semantic,
      line_index,
      sfc_source,
      script_offset,
      kind,
      vue_imports,
      work,
    ),
    shape_cache: HashMap::new(),
    property_shape: HashMap::new(),
    proxy_proof: HashMap::new(),
    facts: SourceContractFacts::default(),
  };
  collector.walk();
  collector.finish()
}

impl Collector<'_> {
  fn walk(&mut self) {
    for (node_id, node) in self.semantic.nodes().iter_enumerated() {
      self.indexes.note_node();
      let AstKind::CallExpression(call) = node.kind() else {
        continue;
      };
      let Some(info) = self.indexes.calls.get(&span_key(call.span)).copied() else {
        continue;
      };
      // Native `structuredClone` is not a Vue API. Classify it before the Vue-only
      // sink return so a proven Proxy constructor in the same module can report.
      // Native-only modules (no Vue imports) stay quiet: there is no actual-Proxy
      // proof. A future native-only sink must not depend on Vue `info.api`.
      if info.native_structured_clone {
        self.collect_structured_clone(info);
      }
      self.collect_inactive_scope_run(node_id, call);
      let Some(api) = info.api else {
        continue;
      };
      if info.has_spread {
        continue;
      }
      match contract_sink(api) {
        Some(ContractSink::TriggerRef) => self.collect_trigger_ref(info),
        Some(ContractSink::ToRefs) => {
          self.collect_torefs(info);
          self.collect_missing_torefs_key(node_id, call, info);
        }
        Some(ContractSink::ProxyConstructor) => self.collect_primitive_reactive(info, api),
        Some(ContractSink::Watch) => {
          self.collect_watch(node_id, call, info);
          self.collect_watch_api(call, info);
          self.collect_watch_callback_contracts(call, info);
        }
        Some(ContractSink::WatchEffectFamily) => self.collect_watch_api(call, info),
        Some(ContractSink::ToRef) => self.collect_toref(call, info),
        Some(ContractSink::EffectScope) => self.collect_effect_scope(info),
        Some(ContractSink::CustomRef) => self.collect_custom_ref(node_id, call, info),
        None => {}
      }
    }
  }

  fn finish(mut self) -> (SourceContractFacts, SourceContractStats) {
    self.facts.trigger_ref_non_ref.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.torefs_non_proxy.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.primitive_reactive_target.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.watch_unwrapped_source.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.watch_replaced_object_source.sort_by(|left, right| {
      self.indexes.note_query();
      (left.source_span.offset, left.replacement_span.offset)
        .cmp(&(right.source_span.offset, right.replacement_span.offset))
    });
    self.facts.watch_ignored_option.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.watch_signature_mismatch.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.watch_callback_contracts.sort_by(|left, right| {
      self.indexes.note_query();
      (left.watch_span.offset, left.guard_span.offset, left.reason as u8).cmp(&(
        right.watch_span.offset,
        right.guard_span.offset,
        right.reason as u8,
      ))
    });
    self.facts.toref_ignored_key.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.effect_scope_callback.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.uncloneable_proxy_data.sort_by(|left, right| {
      self.indexes.note_query();
      left.span.offset.cmp(&right.span.offset)
    });
    self.facts.invalid_custom_ref_interface.sort_by(|left, right| {
      self.indexes.note_query();
      (left.demand_span.offset, left.interface_span.offset)
        .cmp(&(right.demand_span.offset, right.interface_span.offset))
    });
    self.facts.inactive_scope_result.sort_by(|left, right| {
      self.indexes.note_query();
      (left.consumer_span.offset, left.run_span.offset)
        .cmp(&(right.consumer_span.offset, right.run_span.offset))
    });
    self.facts.missing_torefs_key.sort_by(|left, right| {
      self.indexes.note_query();
      (left.demand_span.offset, left.torefs_span.offset)
        .cmp(&(right.demand_span.offset, right.torefs_span.offset))
    });
    (self.facts, self.indexes.stats())
  }

  fn collect_trigger_ref(&mut self, info: CallInfo) {
    let Some(argument) = info.first_arg else {
      return;
    };
    if self.classify_span(argument, MAX_DEPTH).is_non_ref_trigger_target() {
      self
        .facts
        .trigger_ref_non_ref
        .push(SourceContractSiteFact { span: self.span(argument), api: "triggerRef".into() });
    }
  }

  fn collect_torefs(&mut self, info: CallInfo) {
    let Some(argument) = info.first_arg else {
      return;
    };
    if self.classify_span(argument, MAX_DEPTH).is_plain_torefs_target() {
      self
        .facts
        .torefs_non_proxy
        .push(SourceContractSiteFact { span: self.span(argument), api: "toRefs".into() });
    }
  }

  fn collect_primitive_reactive(&mut self, info: CallInfo, api: &str) {
    let Some(argument) = info.first_arg else {
      return;
    };
    if self.classify_span(argument, MAX_DEPTH).is_primitive_reactive_target() {
      self
        .facts
        .primitive_reactive_target
        .push(SourceContractSiteFact { span: self.span(argument), api: api.into() });
    }
  }

  fn collect_watch(&mut self, node_id: NodeId, call: &CallExpression<'_>, info: CallInfo) {
    let Some(source) = info.first_arg.and_then(|span| positional_source(call, span)) else {
      return;
    };
    match source.get_inner_expression() {
      Expression::ArrayExpression(array) => {
        if array.elements.iter().any(|element| {
          element.is_elision() || matches!(element, ArrayExpressionElement::SpreadElement(_))
        }) {
          return;
        }
        for element in &array.elements {
          let Some(expression) = element.as_expression() else {
            continue;
          };
          self.collect_watch_source(node_id, expression);
        }
      }
      inner => self.collect_watch_source(node_id, inner),
    }
  }

  fn collect_watch_source(&mut self, watch_id: NodeId, source: &Expression<'_>) {
    let inner = source.get_inner_expression();
    if matches!(inner, Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)) {
      return;
    }
    if self.is_unwrapped_primitive_source(watch_id, inner) {
      self
        .facts
        .watch_unwrapped_source
        .push(SourceContractSiteFact { span: self.span(source.span()), api: "watch".into() });
      return;
    }
    self.collect_replaced_object_source(watch_id, source, inner);
  }

  fn is_unwrapped_primitive_source(&mut self, watch_id: NodeId, source: &Expression<'_>) -> bool {
    match source.get_inner_expression() {
      Expression::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
        let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
          return false;
        };
        let Some(symbol_id) = self.reference_symbol(object) else {
          return false;
        };
        self.classify_symbol(symbol_id, MAX_DEPTH) == Shape::RefLike
          && !self.indexes.payload_uncertain(symbol_id)
          && self.ref_payload_at(
            self.indexes.root_of(symbol_id),
            watch_id,
            self.span(source.span()).offset,
          ) == Shape::Primitive
      }
      Expression::StaticMemberExpression(member) => {
        let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
          return false;
        };
        let Some(symbol_id) = self.reference_symbol(object) else {
          return false;
        };
        let shape = self.classify_symbol(symbol_id, MAX_DEPTH);
        if !shape.is_deep_mutable_proxy()
          && shape != Shape::ShallowProxy
          && shape != Shape::ReadonlyProxy
        {
          return false;
        }
        !self.indexes.payload_uncertain(symbol_id)
          && self.member_shape_at(
            self.indexes.root_of(symbol_id),
            member.property.name.as_str(),
            watch_id,
            self.span(source.span()).offset,
          ) == Shape::Primitive
      }
      Expression::Identifier(identifier) => {
        matches!(self.classify_identifier(identifier, MAX_DEPTH), Shape::Primitive | Shape::Nullish)
      }
      other => self.classify_span(other.span(), MAX_DEPTH) == Shape::Primitive,
    }
  }

  fn collect_replaced_object_source(
    &mut self,
    watch_id: NodeId,
    source: &Expression<'_>,
    inner: &Expression<'_>,
  ) {
    if self.call_is_bound(watch_id) {
      return;
    }
    let Some(watch_stmt) = self.indexes.stmt_site.get(&watch_id).copied() else {
      return;
    };
    let Some((object, property)) = static_member(inner) else {
      return;
    };
    let Some(object_symbol) = self.reference_symbol(object) else {
      return;
    };
    if !self.classify_symbol(object_symbol, MAX_DEPTH).is_deep_mutable_proxy() {
      return;
    }
    let root = self.indexes.root_of(object_symbol);
    if self.indexes.payload_uncertain(root) {
      return;
    }
    let watch_offset = self.span(source.span()).offset;
    let shape = self.member_shape_at(root, property, watch_id, watch_offset);
    if !matches!(shape, Shape::PlainRecord | Shape::Collection) {
      return;
    }
    let Some(replacement) = self.indexes.first_member_write_after(
      root,
      property,
      watch_stmt.block,
      watch_stmt.expr_offset,
    ) else {
      return;
    };
    let watch_end = self.span(source.span()).offset.saturating_add(self.span(source.span()).length);
    if self.indexes.has_event_between(watch_stmt.block, watch_end, replacement.offset) {
      return;
    }
    self.facts.watch_replaced_object_source.push(WatchReplacedObjectSourceFact {
      source_span: self.span(source.span()),
      replacement_span: self.span(replacement.rhs),
      object: object.name.to_string(),
      property: property.to_string(),
    });
  }

  fn call_is_bound(&self, node_id: NodeId) -> bool {
    matches!(
      self.semantic.nodes().parent_kind(node_id),
      AstKind::VariableDeclarator(_) | AstKind::AssignmentExpression(_)
    )
  }

  fn ref_payload_at(&mut self, root: SymbolId, watch_id: NodeId, watch_offset: usize) -> Shape {
    let init = self.ref_init_payload(root);
    if init != Shape::Primitive && init != Shape::Function {
      return Shape::Unknown;
    }
    if self.indexes.value_writes_mixed(root) {
      return Shape::Unknown;
    }
    let Some(watch_stmt) = self.indexes.stmt_site.get(&watch_id).copied() else {
      return Shape::Unknown;
    };
    if self.indexes.value_write_owner_mismatch(root, watch_stmt.callable, watch_stmt.block) {
      return Shape::Unknown;
    }
    if self
      .indexes
      .last_value_write_before(root, watch_stmt.callable, watch_stmt.block, watch_offset)
      .is_some()
    {
      return Shape::Unknown;
    }
    init
  }

  fn member_shape_at(
    &mut self,
    root: SymbolId,
    property: &str,
    watch_id: NodeId,
    watch_offset: usize,
  ) -> Shape {
    if self.indexes.member_writes_mixed(root, property) {
      return Shape::Unknown;
    }
    let shape = self.initial_member_shape(root, property);
    let Some(watch_stmt) = self.indexes.stmt_site.get(&watch_id).copied() else {
      return Shape::Unknown;
    };
    if self.indexes.member_write_owner_mismatch(
      root,
      property,
      watch_stmt.callable,
      watch_stmt.block,
    ) {
      return Shape::Unknown;
    }
    let Some(prior) = self.indexes.last_member_write_before(
      root,
      property,
      watch_stmt.callable,
      watch_stmt.block,
      watch_offset,
    ) else {
      return shape;
    };
    if !prior.simple_assign {
      return Shape::Unknown;
    }
    if prior.fresh_alloc { Shape::PlainRecord } else { Shape::Unknown }
  }

  fn initial_member_shape(&mut self, root: SymbolId, property: &str) -> Shape {
    let key = (root, property.to_string());
    if let Some(cached) = self.property_shape.get(&key) {
      return *cached;
    }
    let Some(init_span) = self.indexes.init_span.get(&root).copied() else {
      self.property_shape.insert(key, Shape::Unknown);
      return Shape::Unknown;
    };
    let Some(object_span) = self.proxy_object_span(init_span) else {
      self.property_shape.insert(key, Shape::Unknown);
      return Shape::Unknown;
    };
    let shape = match self.indexes.object_prop(object_span, property) {
      None | Some(ObjectProp::Unknown) => Shape::Unknown,
      Some(ObjectProp::Value(span)) => {
        self.classify_maybe(span, MAX_DEPTH).unwrap_or(Shape::Unknown)
      }
    };
    self.property_shape.insert(key, shape);
    shape
  }

  pub(in crate::source_contracts) fn proxy_object_span(&self, init_span: Span) -> Option<Span> {
    let call = self.indexes.calls.get(&span_key(init_span)).copied();
    if let Some(info) = call
      && matches!(info.api, Some("reactive" | "readonly" | "shallowReactive" | "shallowReadonly"))
    {
      return info.first_arg;
    }
    if self.indexes.objects.contains_key(&span_key(init_span)) {
      return Some(init_span);
    }
    if let ShapeHint::Call(span) = self.indexes.hints.get(&span_key(init_span)).copied()? {
      let info = self.indexes.calls.get(&span_key(span)).copied()?;
      if matches!(info.api, Some("reactive" | "readonly" | "shallowReactive" | "shallowReadonly")) {
        return info.first_arg;
      }
    }
    None
  }

  fn ref_init_payload(&mut self, root: SymbolId) -> Shape {
    let Some(init_span) = self.indexes.init_span.get(&root).copied() else {
      return Shape::Unknown;
    };
    let Some(hint) = self.indexes.hints.get(&span_key(init_span)).copied() else {
      return Shape::Unknown;
    };
    let ShapeHint::Call(span) = hint else {
      return Shape::Unknown;
    };
    let Some(info) = self.indexes.calls.get(&span_key(span)).copied() else {
      return Shape::Unknown;
    };
    if !matches!(info.api, Some("ref" | "shallowRef")) || info.has_spread {
      return Shape::Unknown;
    }
    info
      .first_arg
      .and_then(|argument| self.classify_maybe(argument, MAX_DEPTH))
      .unwrap_or(Shape::Unknown)
  }

  pub(in crate::source_contracts) fn classify_span(&mut self, span: Span, remaining: u8) -> Shape {
    self.classify_maybe(span, remaining).unwrap_or(Shape::Unknown)
  }

  pub(in crate::source_contracts) fn classify_maybe(
    &mut self,
    span: Span,
    remaining: u8,
  ) -> Option<Shape> {
    self.indexes.note_query();
    if remaining == 0 {
      return None;
    }
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    Some(match hint {
      ShapeHint::Unknown | ShapeHint::Identifier(None, _) => Shape::Unknown,
      ShapeHint::Primitive => Shape::Primitive,
      ShapeHint::Nullish | ShapeHint::Identifier(_, true) => Shape::Nullish,
      ShapeHint::PlainRecord => Shape::PlainRecord,
      ShapeHint::Function => Shape::Function,
      ShapeHint::Identifier(Some(symbol_id), false) => {
        self.classify_symbol_maybe(symbol_id, remaining.saturating_sub(1))?
      }
      ShapeHint::Call(call_span) => {
        let info = self.indexes.calls.get(&span_key(call_span)).copied()?;
        if info.has_spread {
          Shape::Unknown
        } else if let Some(api) = info.api {
          if is_ref_api(api) {
            Shape::RefLike
          } else {
            let inner = match info.first_arg {
              None => Shape::Unknown,
              Some(argument) => self.classify_maybe(argument, remaining.saturating_sub(1))?,
            };
            classify_vue_result(api, inner, info.has_spread)
          }
        } else {
          Shape::Unknown
        }
      }
      ShapeHint::New(span) => {
        if self.indexes.collections.contains(&span_key(span)) {
          Shape::Collection
        } else {
          Shape::Unknown
        }
      }
    })
  }

  fn classify_identifier(&mut self, identifier: &IdentifierReference<'_>, remaining: u8) -> Shape {
    let Some(symbol_id) = self.reference_symbol(identifier) else {
      return Shape::Unknown;
    };
    self.classify_symbol(symbol_id, remaining)
  }

  pub(in crate::source_contracts) fn classify_symbol(
    &mut self,
    symbol_id: SymbolId,
    remaining: u8,
  ) -> Shape {
    self.classify_symbol_maybe(symbol_id, remaining).unwrap_or(Shape::Unknown)
  }

  fn classify_symbol_maybe(&mut self, symbol_id: SymbolId, remaining: u8) -> Option<Shape> {
    let root = self.indexes.root_of(symbol_id);
    if let Some(cached) = self.shape_cache.get(&root) {
      return Some(*cached);
    }
    if remaining == 0 {
      return None;
    }
    if self.indexes.vue_imports.contains_key(&root) || self.indexes.reassigned.contains(&root) {
      self.shape_cache.insert(root, Shape::Unknown);
      return Some(Shape::Unknown);
    }
    let Some(init_span) = self.indexes.init_span.get(&root).copied() else {
      self.shape_cache.insert(root, Shape::Unknown);
      return Some(Shape::Unknown);
    };
    let shape = self.classify_maybe(init_span, remaining.saturating_sub(1))?;
    self.shape_cache.insert(root, shape);
    Some(shape)
  }

  pub(in crate::source_contracts) fn reference_symbol(
    &self,
    identifier: &IdentifierReference<'_>,
  ) -> Option<SymbolId> {
    let reference_id = identifier.reference_id.get()?;
    self.semantic.scoping().get_reference(reference_id).symbol_id()
  }

  pub(in crate::source_contracts) fn span(&self, span: Span) -> SourceSpan {
    source_span(self.line_index, self.sfc_source, self.script_offset, span)
  }
}

fn positional_source<'a>(call: &'a CallExpression<'a>, span: Span) -> Option<&'a Expression<'a>> {
  call.arguments.first().and_then(oxc_ast::ast::Argument::as_expression).filter(|expression| {
    expression.span() == span || expression.get_inner_expression().span() == span
  })
}

fn static_member<'a>(
  expression: &'a Expression<'a>,
) -> Option<(&'a IdentifierReference<'a>, &'a str)> {
  let Expression::StaticMemberExpression(member) = expression.get_inner_expression() else {
    return None;
  };
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  Some((object, member.property.name.as_str()))
}
