//! Closed-local customRef track/trigger reachability (issue round-4 contract).

use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, ArrayExpressionElement, AssignmentOperator, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, BinaryOperator, BindingPattern,
    CallExpression, Expression, FormalParameters, FunctionBody, ObjectExpression,
    ObjectPropertyKind, PropertyKind, SimpleAssignmentTarget, Statement, VariableDeclarationKind,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};

use super::Collector;
use super::custom_ref_proof::{
  TakenBranch, TakenLogical, callable_body, computed_key_expression, is_nested_callable,
  object_value_is_eager, preceding_statement_blocks_read, taken_conditional_branch,
  taken_logical_side,
};
use super::index::{ArgUse, CallInfo, WriteLiteral};
use super::proof::{classify_reach, enclosing_call, skip_ts_parent};
use super::shape::{Shape, span_key};
use super::timeline;
use vue_vet_core::{CustomRefLostNotificationFact, CustomRefLostNotificationReason};

const MAX_BODY_NODES: u32 = 256;
const FACTORY_BIND_BUDGET: u32 = 2048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Three {
  Present,
  Missing,
  Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum KnownPrim {
  Number(u64),
  Bool(bool),
  Str(String),
  BigInt(String),
  Undefined,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ConsumerSite {
  span: Span,
  offset: usize,
  block: NodeId,
  handle: Option<SymbolId>,
}

#[derive(Clone, Debug)]
pub(in crate::source_contracts) struct FactorySummary {
  valid_interface: bool,
  backing_bridge: bool,
  primitive_slot: bool,
  get_tracking: Three,
  set_notification: Three,
  get_span: Option<Span>,
  set_span: Option<Span>,
  storage_init: Option<KnownPrim>,
}

#[derive(Clone, Copy)]
struct SlotFn<'a> {
  span: Span,
  body: &'a FunctionBody<'a>,
  params: &'a FormalParameters<'a>,
  expression: bool,
  first_param: Option<SymbolId>,
  /// False for generator (body dormant on call) and async accessors.
  eager: bool,
}

#[derive(Default)]
#[expect(clippy::struct_excessive_bools, reason = "capability flags are independent proofs")]
struct BodyProof {
  track_sync: bool,
  trigger_sync: bool,
  track_deferred: bool,
  trigger_deferred: bool,
  track_passed: bool,
  trigger_passed: bool,
  helper_call: bool,
  param_member: bool,
  storage_read: bool,
  storage_write: bool,
  storage_from_param: bool,
  storage_other_write: bool,
  param_mutated: bool,
  storage_return: bool,
  getter_projection: bool,
  backing_read: bool,
  backing_write: bool,
  unknown_flow: bool,
  nodes: u32,
}

struct WalkCtx<'a> {
  track: Option<SymbolId>,
  trigger: Option<SymbolId>,
  storage: Option<SymbolId>,
  aliases: &'a HashMap<SymbolId, SymbolId>,
  holders: &'a HashSet<SymbolId>,
  setter_param: Option<SymbolId>,
  is_get: bool,
}

impl Collector<'_> {
  pub(super) fn collect_custom_ref_lost_notification(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread {
      return;
    }
    let Some(factory) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let summary = self.factory_summary(factory);
    if !summary.valid_interface || summary.backing_bridge || !summary.primitive_slot {
      return;
    }
    let Some(result) = self.result_symbol(node_id) else {
      return;
    };
    let root = self.indexes.root_of(result);
    if self.indexes.reassigned.contains(&root) {
      return;
    }
    if self.unknown_custom_ref_borrow(root, call.span) {
      return;
    }
    let Some(consumer) = self.first_active_consumer(root) else {
      return;
    };
    let Some(write) = self.first_changed_write(root, &summary, consumer) else {
      return;
    };
    let fallback = factory.span();
    if summary.get_tracking == Three::Missing {
      self.push_lost(
        summary.get_span.unwrap_or(fallback),
        call.span,
        consumer.span,
        write.span,
        CustomRefLostNotificationReason::GetTracking,
      );
    }
    if summary.set_notification == Three::Missing && !self.has_trigger_ref_after(root, write.offset)
    {
      self.push_lost(
        summary.set_span.unwrap_or(fallback),
        call.span,
        consumer.span,
        write.span,
        CustomRefLostNotificationReason::SetNotification,
      );
    }
  }

  fn factory_summary(&mut self, factory: &Expression<'_>) -> FactorySummary {
    let inner = factory.get_inner_expression();
    let key = span_key(inner.span());
    if let Some(cached) = self.factory_summaries.get(&key) {
      self.indexes.note_query();
      return cached.clone();
    }
    let summary = self.analyze_factory(inner);
    self.factory_summaries.insert(key, summary.clone());
    summary
  }

  fn analyze_factory(&mut self, factory: &Expression<'_>) -> FactorySummary {
    let quiet = FactorySummary {
      valid_interface: false,
      backing_bridge: false,
      primitive_slot: false,
      get_tracking: Three::Unknown,
      set_notification: Three::Unknown,
      get_span: None,
      set_span: None,
      storage_init: None,
    };
    let (fn_span, body, expression, params_ok, track, trigger) = match factory {
      Expression::ArrowFunctionExpression(arrow) => (
        arrow.span,
        &*arrow.body,
        arrow.expression,
        factory_params_ok(arrow.params.items.len(), arrow.params.rest.is_some()),
        param_symbol(self.indexes.function(arrow.span), 0),
        param_symbol(self.indexes.function(arrow.span), 1),
      ),
      Expression::FunctionExpression(function) => {
        let Some(body) = function.body.as_deref() else {
          return quiet;
        };
        (
          function.span,
          body,
          false,
          factory_params_ok(function.params.items.len(), function.params.rest.is_some()),
          param_symbol(self.indexes.function(function.span), 0),
          param_symbol(self.indexes.function(function.span), 1),
        )
      }
      _ => return quiet,
    };
    if !params_ok {
      return quiet;
    }
    if self.indexes.function(fn_span).is_some_and(|info| info.has_rest) {
      return quiet;
    }
    self.indexes.note_node();
    let Some(object) = return_object_from_body(body, expression) else {
      return quiet;
    };
    let Some((get, set)) = slots_of_object(object) else {
      return quiet;
    };
    let Some(get) = get else {
      return quiet;
    };
    let Some(set) = set else {
      return quiet;
    };
    let (storage, storage_init) = self.factory_storage(body, fn_span);
    let storage_init =
      self.factory_executed_storage_value(body, storage, storage_init, get.span, set.span);
    let aliases = self.local_aliases(body, track, trigger);
    let holders = self.capability_holders(body, &aliases);
    let extra_capability_escape = return_object_escapes_capabilities(object, &aliases, self);
    let get_proof = self.analyze_slot(&get, track, trigger, storage, &aliases, &holders, true);
    let set_proof = self.analyze_slot(&set, track, trigger, storage, &aliases, &holders, false);
    let backing_bridge = get_proof.backing_read && set_proof.backing_write;
    let primitive_slot = storage.is_some()
      && get_proof.storage_return
      && !get_proof.getter_projection
      && !get_proof.storage_write
      && !get_proof.unknown_flow
      && set_proof.storage_from_param
      && !set_proof.storage_other_write
      && !set_proof.unknown_flow
      && storage_init.is_some();
    let mut get_tracking = three_get(&get_proof);
    let mut set_notification = three_set(&set_proof);
    if extra_capability_escape {
      if get_tracking == Three::Missing {
        get_tracking = Three::Unknown;
      }
      if set_notification == Three::Missing {
        set_notification = Three::Unknown;
      }
    }
    FactorySummary {
      valid_interface: true,
      backing_bridge,
      primitive_slot,
      get_tracking,
      set_notification,
      get_span: Some(get.span),
      set_span: Some(set.span),
      storage_init,
    }
  }

  fn factory_storage(
    &mut self,
    body: &FunctionBody<'_>,
    factory_span: Span,
  ) -> (Option<SymbolId>, Option<KnownPrim>) {
    let mut chosen = None;
    for statement in &body.statements {
      self.indexes.note_node();
      let Statement::VariableDeclaration(declaration) = statement else {
        continue;
      };
      if !matches!(declaration.kind, VariableDeclarationKind::Let | VariableDeclarationKind::Var) {
        continue;
      }
      for declarator in &declaration.declarations {
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          return (None, None);
        };
        let Some(symbol_id) = binding.symbol_id.get() else {
          continue;
        };
        let Some(init) = &declarator.init else {
          return (None, None);
        };
        let Some(prim) = known_prim(init, |ident| self.reference_symbol(ident)) else {
          if self.looks_like_ref_init(init) {
            continue;
          }
          return (None, None);
        };
        if chosen.is_some() {
          return (None, None);
        }
        let _ = factory_span;
        chosen = Some((symbol_id, prim));
      }
    }
    match chosen {
      Some((symbol_id, prim)) => (Some(symbol_id), Some(prim)),
      None => (None, None),
    }
  }

  fn factory_executed_storage_value(
    &mut self,
    body: &FunctionBody<'_>,
    storage: Option<SymbolId>,
    mut current: Option<KnownPrim>,
    get_span: Span,
    set_span: Span,
  ) -> Option<KnownPrim> {
    let storage = storage?;
    let mut unknown = false;
    let mut bind_budget = FACTORY_BIND_BUDGET;
    for statement in &body.statements {
      self.indexes.note_node();
      match statement {
        Statement::VariableDeclaration(declaration) => {
          for declarator in &declaration.declarations {
            if let Some(init) = &declarator.init {
              apply_factory_storage_expr(
                self,
                init,
                storage,
                get_span,
                set_span,
                &mut current,
                &mut unknown,
              );
            }
            apply_factory_binding_pattern(
              self,
              &declarator.id,
              declarator.init.as_ref(),
              storage,
              get_span,
              set_span,
              &mut current,
              &mut unknown,
              &mut bind_budget,
            );
          }
        }
        Statement::ExpressionStatement(statement) => {
          apply_factory_storage_expr(
            self,
            &statement.expression,
            storage,
            get_span,
            set_span,
            &mut current,
            &mut unknown,
          );
        }
        Statement::ReturnStatement(ret) => {
          if let Some(argument) = &ret.argument {
            apply_factory_storage_expr(
              self,
              argument,
              storage,
              get_span,
              set_span,
              &mut current,
              &mut unknown,
            );
          }
        }
        Statement::FunctionDeclaration(function)
          if function_mentions_storage(function.body.as_deref(), storage, self) =>
        {
          unknown = true;
        }
        Statement::EmptyStatement(_) | Statement::DebuggerStatement(_) => {}
        _ => unknown = true,
      }
      if unknown {
        return None;
      }
    }
    current
  }

  fn looks_like_ref_init(&mut self, expression: &Expression<'_>) -> bool {
    matches!(
      self.classify_span_hint(expression),
      Shape::RefLike | Shape::DeepProxy | Shape::ShallowProxy | Shape::ReadonlyProxy
    )
  }

  fn classify_span_hint(&mut self, expression: &Expression<'_>) -> Shape {
    let inner = expression.get_inner_expression();
    if let Some(identifier) = inner.get_identifier_reference()
      && let Some(symbol_id) = self.reference_symbol(identifier)
    {
      return self.classify_symbol(symbol_id, 8);
    }
    if let Expression::CallExpression(call) = inner
      && let Some(info) = self.indexes.calls.get(&span_key(call.span)).copied()
      && let Some(api) = info.api
    {
      return super::shape::classify_vue_result(api, Shape::Unknown, info.has_spread);
    }
    Shape::Unknown
  }

  fn local_aliases(
    &self,
    body: &FunctionBody<'_>,
    track: Option<SymbolId>,
    trigger: Option<SymbolId>,
  ) -> HashMap<SymbolId, SymbolId> {
    let mut aliases = HashMap::new();
    if let Some(track) = track {
      aliases.insert(track, track);
    }
    if let Some(trigger) = trigger {
      aliases.insert(trigger, trigger);
    }
    for statement in &body.statements {
      self.indexes.note_node();
      let Statement::VariableDeclaration(declaration) = statement else {
        continue;
      };
      for declarator in &declaration.declarations {
        self.indexes.note_query();
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          continue;
        };
        let Some(local) = binding.symbol_id.get() else {
          continue;
        };
        if !self.semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
          continue;
        }
        let Some(init) = &declarator.init else {
          continue;
        };
        let Some(ident) = init.get_inner_expression().get_identifier_reference() else {
          continue;
        };
        let Some(target) = self.reference_symbol(ident) else {
          continue;
        };
        if let Some(root) = aliases.get(&target).copied() {
          aliases.insert(local, root);
        }
      }
    }
    aliases
  }

  fn capability_holders(
    &self,
    body: &FunctionBody<'_>,
    aliases: &HashMap<SymbolId, SymbolId>,
  ) -> HashSet<SymbolId> {
    let mut holders = HashSet::new();
    for statement in &body.statements {
      self.indexes.note_node();
      let Statement::VariableDeclaration(declaration) = statement else {
        continue;
      };
      for declarator in &declaration.declarations {
        self.indexes.note_query();
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          continue;
        };
        let Some(local) = binding.symbol_id.get() else {
          continue;
        };
        let Some(init) = &declarator.init else {
          continue;
        };
        if object_or_array_holds_capability(init, aliases, self) {
          holders.insert(local);
        }
      }
    }
    let mut progressed = true;
    let mut rounds = 0_u8;
    while progressed && rounds < 8 {
      progressed = false;
      rounds = rounds.saturating_add(1);
      for statement in &body.statements {
        self.indexes.note_node();
        let Statement::VariableDeclaration(declaration) = statement else {
          continue;
        };
        for declarator in &declaration.declarations {
          self.indexes.note_query();
          let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
            continue;
          };
          let Some(local) = binding.symbol_id.get() else {
            continue;
          };
          if holders.contains(&local) {
            continue;
          }
          if !self.semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
            continue;
          }
          let Some(init) = &declarator.init else {
            continue;
          };
          let Some(ident) = init.get_inner_expression().get_identifier_reference() else {
            continue;
          };
          let Some(target) = self.reference_symbol(ident) else {
            continue;
          };
          let root = self.indexes.root_of(target);
          if holders.contains(&target) || holders.contains(&root) {
            holders.insert(local);
            progressed = true;
          }
        }
      }
    }
    holders
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "slot analysis threads track/trigger/storage/holder proofs"
  )]
  fn analyze_slot(
    &mut self,
    slot: &SlotFn<'_>,
    track: Option<SymbolId>,
    trigger: Option<SymbolId>,
    storage: Option<SymbolId>,
    aliases: &HashMap<SymbolId, SymbolId>,
    holders: &HashSet<SymbolId>,
    is_get: bool,
  ) -> BodyProof {
    let mut proof = BodyProof::default();
    if !slot.eager {
      proof.unknown_flow = true;
      return proof;
    }
    let ctx =
      WalkCtx { track, trigger, storage, aliases, holders, setter_param: slot.first_param, is_get };
    walk_param_defaults(self, slot.params, &ctx, &mut proof);
    walk_body(self, slot.body, slot.expression, &ctx, &mut proof);
    proof
  }

  fn unknown_custom_ref_borrow(&self, root: SymbolId, factory_span: Span) -> bool {
    if self.declaration_is_exported(root) {
      return true;
    }
    if self.indexes.has_member_mutation(root) {
      return true;
    }
    for use_site in self.indexes.arg_uses_of(root) {
      self.indexes.note_query();
      match (use_site.api, use_site.index) {
        (Some("watch" | "triggerRef"), 0) => {}
        (_, _) if use_site.call_span == factory_span => {}
        _ => return true,
      }
    }
    let members = self.indexes.symbols_of_root(root);
    if members.is_empty() {
      return self.symbol_has_unknown_borrow(root, factory_span);
    }
    for symbol_id in members {
      self.indexes.note_query();
      if self.symbol_has_unknown_borrow(*symbol_id, factory_span) {
        return true;
      }
    }
    false
  }

  fn symbol_has_unknown_borrow(&self, symbol_id: SymbolId, factory_span: Span) -> bool {
    for reference in self.semantic.symbol_references(symbol_id) {
      self.indexes.note_query();
      if !self.reference_is_known_borrow(reference.node_id(), factory_span) {
        return true;
      }
    }
    false
  }

  fn declaration_is_exported(&self, symbol_id: SymbolId) -> bool {
    let mut node_id = self.semantic.scoping().symbol_declaration(symbol_id);
    for _ in 0..8 {
      self.indexes.note_query();
      match self.semantic.nodes().kind(node_id) {
        AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => return true,
        AstKind::Program(_) => return false,
        _ => node_id = self.semantic.nodes().parent_id(node_id),
      }
    }
    false
  }

  fn reference_is_known_borrow(&self, node_id: NodeId, factory_span: Span) -> bool {
    let parent = skip_ts_parent(self.semantic, node_id, self.indexes.work_counter());
    match self.semantic.nodes().kind(parent) {
      AstKind::StaticMemberExpression(member) if member.property.name.as_str() == "value" => true,
      AstKind::VariableDeclarator(declarator) => {
        matches!(
          declarator.id,
          BindingPattern::BindingIdentifier(ref binding)
            if binding.symbol_id.get().is_some_and(|symbol_id| {
              self.semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable)
            })
        )
      }
      AstKind::CallExpression(call) => {
        if span_covers(call.callee.span(), self.semantic.nodes().kind(node_id).span()) {
          return false;
        }
        self.known_watch_or_trigger_arg(
          call.span,
          factory_span,
          self.semantic.nodes().kind(node_id).span(),
        )
      }
      AstKind::ArrayExpression(array) => {
        let Some((_, call)) = enclosing_call(self.semantic, parent, self.indexes.work_counter())
        else {
          return false;
        };
        span_covers(array.span, self.semantic.nodes().kind(node_id).span())
          && self.known_watch_or_trigger_arg(call.span, factory_span, array.span)
      }
      _ => false,
    }
  }

  fn known_watch_or_trigger_arg(
    &self,
    call_span: Span,
    factory_span: Span,
    ident_span: Span,
  ) -> bool {
    self.indexes.note_query();
    if call_span == factory_span {
      return true;
    }
    let Some(info) = self.indexes.calls.get(&span_key(call_span)).copied() else {
      return false;
    };
    matches!(info.api, Some("watch" | "triggerRef"))
      && info.first_arg.is_some_and(|argument| span_covers(argument, ident_span))
  }

  fn first_active_consumer(&self, root: SymbolId) -> Option<ConsumerSite> {
    let mut sites = Vec::new();
    for use_site in self.indexes.arg_uses_of(root) {
      self.indexes.note_query();
      if use_site.api == Some("watch")
        && use_site.index == 0
        && let Some(site) = self.proven_consumer(*use_site, None)
      {
        sites.push(site);
      }
    }
    for read in self.indexes.value_reads_of(root) {
      self.indexes.note_query();
      let Some(callable) = read.callable else {
        continue;
      };
      if !self.callback_read_is_executed(read) {
        continue;
      }
      let Some(fn_span) = self.indexes.function_by_node.get(&callable).copied() else {
        continue;
      };
      self.indexes.note_query();
      let site = self
        .indexes
        .effect_calls
        .get(&span_key(fn_span))
        .copied()
        .or_else(|| self.indexes.watch_getters.get(&span_key(fn_span)).copied());
      let Some(effect) = site else {
        continue;
      };
      if let Some(consumer) = self.proven_consumer(effect, Some(read.node_id)) {
        sites.push(consumer);
      }
    }
    sites.sort_by_key(|site| site.offset);
    sites.into_iter().next()
  }

  fn proven_consumer(&self, use_site: ArgUse, read_node: Option<NodeId>) -> Option<ConsumerSite> {
    let reach = classify_reach(self.semantic, use_site.node_id, self.indexes.work_counter());
    if !reach.is_straight() {
      return None;
    }
    if let Some(read_node) = read_node {
      let read_reach = classify_reach(self.semantic, read_node, self.indexes.work_counter());
      if !read_reach.is_straight() {
        return None;
      }
    }
    let options = self.indexes.watch_options_of(use_site.call_span, use_site.api);
    if options.immediately_active(use_site.api) != Some(true) {
      return None;
    }
    Some(self.consumer_from_arg(use_site))
  }

  fn callback_read_is_executed(&self, read: &super::index::ValueRead) -> bool {
    let Some(callable) = read.callable else {
      return false;
    };
    let Some(body) = callable_body(self.semantic, callable) else {
      return false;
    };
    !preceding_statement_blocks_read(
      &body.statements,
      read.span,
      self.semantic,
      self.indexes.work_counter(),
    )
  }

  fn consumer_from_arg(&self, use_site: ArgUse) -> ConsumerSite {
    let handle = self.indexes.call_result(use_site.call_span);
    ConsumerSite {
      span: use_site.call_span,
      offset: use_site.offset,
      block: use_site.block,
      handle,
    }
  }

  fn handle_inactive_from(&self, consumer: ConsumerSite) -> Option<usize> {
    let handle = consumer.handle?;
    let root = self.indexes.root_of(handle);
    self.indexes.note_query();
    if self.indexes.reassigned.contains(&root) || self.indexes.escaped.contains(&root) {
      return Some(consumer.offset);
    }
    self.indexes.first_inactivity_after(root, consumer.block, consumer.offset)
  }

  fn first_changed_write(
    &self,
    root: SymbolId,
    summary: &FactorySummary,
    consumer: ConsumerSite,
  ) -> Option<super::index::ValueWrite> {
    let init = summary.storage_init.as_ref()?;
    if self.indexes.value_writes_mixed(root) {
      return None;
    }
    let inactive_at = self.handle_inactive_from(consumer);
    let mut current = init.clone();
    for write in self.indexes.value_writes_of(root) {
      self.indexes.note_query();
      if write.block != consumer.block {
        continue;
      }
      if write.offset <= consumer.offset {
        current = literal_to_prim(write.literal)?;
        continue;
      }
      if inactive_at.is_some_and(|offset| write.offset >= offset) {
        break;
      }
      if self.indexes.has_event_between(consumer.block, consumer.offset, write.offset) {
        continue;
      }
      let next = literal_to_prim(write.literal)?;
      if next != current {
        return Some(*write);
      }
      current = next;
    }
    None
  }

  fn has_trigger_ref_after(&self, root: SymbolId, write_offset: usize) -> bool {
    let work = self.indexes.work_counter();
    timeline::after(work, self.indexes.arg_uses_of(root), write_offset).iter().any(|use_site| {
      self.indexes.note_query();
      use_site.api == Some("triggerRef") && use_site.index == 0
    })
  }

  fn push_lost(
    &mut self,
    span: Span,
    source: Span,
    consumer: Span,
    write: Span,
    reason: CustomRefLostNotificationReason,
  ) {
    self.facts.custom_ref_lost_notification.push(CustomRefLostNotificationFact {
      span: self.span(span),
      source_span: self.span(source),
      consumer_span: self.span(consumer),
      write_span: self.span(write),
      reason,
    });
  }
}

const fn factory_params_ok(len: usize, has_rest: bool) -> bool {
  !has_rest && len <= 2
}

const fn span_covers(outer: Span, inner: Span) -> bool {
  outer.start <= inner.start && inner.end <= outer.end
}

const fn literal_to_prim(literal: WriteLiteral) -> Option<KnownPrim> {
  match literal {
    WriteLiteral::Number(bits) => Some(KnownPrim::Number(bits)),
    WriteLiteral::Bool(value) => Some(KnownPrim::Bool(value)),
    WriteLiteral::Undefined => Some(KnownPrim::Undefined),
    WriteLiteral::Other => None,
  }
}

fn param_symbol(info: Option<&super::index::FunctionInfo>, index: usize) -> Option<SymbolId> {
  info.and_then(|info| info.params.get(index).copied().flatten())
}

const fn three_get(proof: &BodyProof) -> Three {
  if proof.track_sync || proof.backing_read {
    Three::Present
  } else if proof.unknown_flow || proof.track_passed || proof.helper_call || proof.param_member {
    Three::Unknown
  } else {
    Three::Missing
  }
}

const fn three_set(proof: &BodyProof) -> Three {
  if proof.trigger_sync || proof.trigger_deferred || proof.backing_write {
    Three::Present
  } else if proof.unknown_flow || proof.trigger_passed || proof.helper_call || proof.param_member {
    Three::Unknown
  } else {
    Three::Missing
  }
}

fn return_object_from_body<'a>(
  body: &'a FunctionBody<'a>,
  expression_arrow: bool,
) -> Option<&'a ObjectExpression<'a>> {
  if expression_arrow {
    let Statement::ExpressionStatement(statement) = body.statements.first()? else {
      return None;
    };
    return as_object(&statement.expression);
  }
  let mut returned = None;
  for statement in &body.statements {
    match statement {
      Statement::ReturnStatement(ret) => {
        if returned.is_some() {
          return None;
        }
        returned = as_object(ret.argument.as_ref()?);
      }
      Statement::VariableDeclaration(_) | Statement::FunctionDeclaration(_) => {}
      _ => return None,
    }
  }
  returned
}

fn as_object<'a>(expression: &'a Expression<'a>) -> Option<&'a ObjectExpression<'a>> {
  match expression.get_inner_expression() {
    Expression::ObjectExpression(object) => Some(object),
    _ => None,
  }
}

fn slots_of_object<'a>(
  object: &'a ObjectExpression<'a>,
) -> Option<(Option<SlotFn<'a>>, Option<SlotFn<'a>>)> {
  let mut get = None;
  let mut set = None;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return None,
      ObjectPropertyKind::ObjectProperty(prop) => {
        if prop.kind != PropertyKind::Init {
          return None;
        }
        let name = prop.key.static_name()?;
        if name == "__proto__" || name == "prototype" {
          return None;
        }
        if name != "get" && name != "set" {
          continue;
        }
        let slot = function_slot(&prop.value)?;
        if name == "get" {
          get = Some(slot);
        } else {
          set = Some(slot);
        }
      }
    }
  }
  Some((get, set))
}

fn function_slot<'a>(expression: &'a Expression<'a>) -> Option<SlotFn<'a>> {
  match expression.get_inner_expression() {
    Expression::FunctionExpression(function) => {
      let body = function.body.as_ref()?;
      Some(SlotFn {
        span: function.span,
        body,
        params: &function.params,
        expression: false,
        first_param: first_identifier_param(&function.params),
        eager: !function.generator && !function.r#async,
      })
    }
    Expression::ArrowFunctionExpression(arrow) => Some(SlotFn {
      span: arrow.span,
      body: &arrow.body,
      params: &arrow.params,
      expression: arrow.expression,
      first_param: first_identifier_param(&arrow.params),
      eager: !arrow.r#async,
    }),
    _ => None,
  }
}

fn first_identifier_param(params: &FormalParameters<'_>) -> Option<SymbolId> {
  params.items.first().and_then(|item| match &item.pattern {
    BindingPattern::BindingIdentifier(binding) => binding.symbol_id.get(),
    _ => None,
  })
}

fn walk_body(
  collector: &mut Collector<'_>,
  body: &FunctionBody<'_>,
  expression: bool,
  ctx: &WalkCtx<'_>,
  proof: &mut BodyProof,
) {
  if expression {
    if let Some(Statement::ExpressionStatement(statement)) = body.statements.first() {
      note_return_expr(collector, &statement.expression, ctx, proof);
      walk_expr(collector, &statement.expression, ctx, true, proof);
    } else {
      proof.unknown_flow = true;
    }
    return;
  }
  walk_statements(collector, &body.statements, ctx, proof);
}

fn walk_statements(
  collector: &mut Collector<'_>,
  statements: &[Statement<'_>],
  ctx: &WalkCtx<'_>,
  proof: &mut BodyProof,
) {
  for statement in statements {
    if proof.nodes >= MAX_BODY_NODES {
      proof.unknown_flow = true;
      return;
    }
    proof.nodes = proof.nodes.saturating_add(1);
    collector.indexes.note_node();
    match statement {
      Statement::ExpressionStatement(statement) => {
        walk_expr(collector, &statement.expression, ctx, true, proof);
      }
      Statement::ReturnStatement(ret) => {
        if let Some(argument) = &ret.argument {
          note_return_expr(collector, argument, ctx, proof);
          walk_expr(collector, argument, ctx, true, proof);
        }
        return;
      }
      Statement::VariableDeclaration(declaration) => {
        for declarator in &declaration.declarations {
          if let Some(init) = &declarator.init {
            walk_expr(collector, init, ctx, true, proof);
          }
          walk_binding_pattern(collector, &declarator.id, ctx, proof);
        }
      }
      Statement::IfStatement(statement)
        if !ctx.is_get
          && is_same_value_guard(statement, ctx.storage, ctx.setter_param, collector) => {}
      Statement::FunctionDeclaration(_) => {}
      _ => {
        proof.unknown_flow = true;
        return;
      }
    }
  }
}

fn note_return_expr(
  collector: &Collector<'_>,
  expression: &Expression<'_>,
  ctx: &WalkCtx<'_>,
  proof: &mut BodyProof,
) {
  if ident_symbol(collector, expression) == ctx.storage && ctx.storage.is_some() {
    proof.storage_return = true;
  } else if mentions_storage(expression, ctx.storage, collector) {
    proof.getter_projection = true;
  }
}

fn walk_expr(
  collector: &mut Collector<'_>,
  expression: &Expression<'_>,
  ctx: &WalkCtx<'_>,
  executed: bool,
  proof: &mut BodyProof,
) {
  if proof.nodes >= MAX_BODY_NODES {
    proof.unknown_flow = true;
    return;
  }
  proof.nodes = proof.nodes.saturating_add(1);
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) => {
      let Some(symbol_id) = collector.reference_symbol(identifier) else {
        return;
      };
      if executed && ctx.storage == Some(symbol_id) {
        proof.storage_read = true;
      }
      if let Some(kind) = ctx.aliases.get(&symbol_id).copied() {
        if !executed {
          return;
        }
        if Some(kind) == ctx.track || Some(kind) == ctx.trigger {
          proof.param_member = true;
        }
      }
    }
    Expression::CallExpression(call) => {
      handle_call(collector, call, ctx, executed, proof);
    }
    Expression::NewExpression(new_expr) => {
      if executed {
        proof.unknown_flow = true;
      }
      let timer = is_timer_callee(&new_expr.callee, collector);
      walk_expr(collector, &new_expr.callee, ctx, executed, proof);
      walk_call_args(collector, &new_expr.arguments, ctx, executed, timer, proof);
    }
    Expression::AssignmentExpression(assignment) => {
      note_assignment(collector, assignment, ctx, executed, proof);
      walk_assignment_target(collector, &assignment.left, ctx, executed, proof);
      walk_expr(collector, &assignment.right, ctx, executed, proof);
    }
    Expression::UpdateExpression(update) => {
      if executed {
        if let Some(ident) = update_target_ident(update)
          && let Some(symbol_id) = collector.reference_symbol(ident)
        {
          if ctx.storage == Some(symbol_id) {
            proof.storage_read = true;
            proof.storage_write = true;
            proof.storage_other_write = true;
          }
          if ctx.setter_param == Some(symbol_id) {
            proof.param_mutated = true;
          }
        } else {
          proof.unknown_flow = true;
        }
      }
      walk_simple_assignment_target(collector, &update.argument, ctx, executed, proof);
    }
    Expression::StaticMemberExpression(member) => {
      if let Some(object) = member.object.get_inner_expression().get_identifier_reference() {
        note_backing(collector, object, member.property.name.as_str(), false, executed, proof);
        if let Some(symbol_id) = collector.reference_symbol(object)
          && ctx.aliases.contains_key(&symbol_id)
        {
          proof.param_member = true;
        }
      }
      walk_expr(collector, &member.object, ctx, executed, proof);
    }
    Expression::ComputedMemberExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
      walk_expr(collector, &member.expression, ctx, executed, proof);
    }
    Expression::PrivateFieldExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
    }
    Expression::SequenceExpression(sequence) => {
      for item in &sequence.expressions {
        walk_expr(collector, item, ctx, executed, proof);
      }
    }
    Expression::UnaryExpression(unary) => {
      walk_expr(collector, &unary.argument, ctx, executed, proof);
    }
    Expression::BinaryExpression(binary) => {
      walk_expr(collector, &binary.left, ctx, executed, proof);
      walk_expr(collector, &binary.right, ctx, executed, proof);
    }
    Expression::LogicalExpression(logical) => {
      walk_expr(collector, &logical.left, ctx, executed, proof);
      match taken_logical_side(
        logical.operator,
        &logical.left,
        collector.semantic,
        collector.indexes.work_counter(),
      ) {
        TakenLogical::Both => walk_expr(collector, &logical.right, ctx, executed, proof),
        TakenLogical::Unknown if executed => proof.unknown_flow = true,
        TakenLogical::LeftOnly | TakenLogical::Unknown => {}
      }
    }
    Expression::ConditionalExpression(conditional) => {
      walk_expr(collector, &conditional.test, ctx, executed, proof);
      match taken_conditional_branch(&conditional.test, collector.indexes.work_counter()) {
        TakenBranch::Consequent => {
          walk_expr(collector, &conditional.consequent, ctx, executed, proof);
        }
        TakenBranch::Alternate => {
          walk_expr(collector, &conditional.alternate, ctx, executed, proof);
        }
        TakenBranch::Unknown if executed => proof.unknown_flow = true,
        TakenBranch::Unknown => {}
      }
    }
    Expression::ChainExpression(_)
    | Expression::AwaitExpression(_)
    | Expression::YieldExpression(_)
      if executed =>
    {
      proof.unknown_flow = true;
    }
    Expression::ObjectExpression(object) => {
      walk_object_expr(collector, object, ctx, executed, proof);
    }
    Expression::ArrayExpression(array) => {
      for element in &array.elements {
        match element {
          ArrayExpressionElement::SpreadElement(spread) => {
            walk_expr(collector, &spread.argument, ctx, executed, proof);
          }
          ArrayExpressionElement::Elision(_) => {}
          other => {
            if let Some(expr) = other.as_expression() {
              walk_expr(collector, expr, ctx, executed && !is_function_expr(expr), proof);
            } else if executed {
              proof.unknown_flow = true;
            }
          }
        }
      }
    }
    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
      if mentions_params(expression, ctx.track, ctx.trigger, ctx.aliases, collector) =>
    {
      if executed {
        proof.helper_call = true;
      } else if ctx.is_get {
        proof.track_passed = true;
      } else {
        proof.trigger_deferred = true;
      }
    }
    Expression::TemplateLiteral(template) => {
      for item in &template.expressions {
        walk_expr(collector, item, ctx, executed, proof);
      }
    }
    Expression::TaggedTemplateExpression(tagged) => {
      if executed {
        proof.helper_call = true;
      }
      walk_expr(collector, &tagged.tag, ctx, executed, proof);
      for item in &tagged.quasi.expressions {
        walk_expr(collector, item, ctx, executed, proof);
      }
    }
    Expression::ArrowFunctionExpression(_)
    | Expression::FunctionExpression(_)
    | Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::ThisExpression(_)
    | Expression::Super(_)
    | Expression::ImportMeta(_)
    | Expression::NewTarget(_) => {}
    _ if executed => proof.unknown_flow = true,
    _ => {}
  }
}

fn handle_call(
  collector: &mut Collector<'_>,
  call: &CallExpression<'_>,
  ctx: &WalkCtx<'_>,
  executed: bool,
  proof: &mut BodyProof,
) {
  let timer = is_timer_callee(&call.callee, collector);
  let callee = call.callee.get_inner_expression();
  if let Some(identifier) = callee.get_identifier_reference()
    && let Some(symbol_id) = collector.reference_symbol(identifier)
  {
    if let Some(kind) = ctx.aliases.get(&symbol_id).copied() {
      if executed {
        if Some(kind) == ctx.track {
          proof.track_sync = true;
        }
        if Some(kind) == ctx.trigger {
          proof.trigger_sync = true;
        }
      } else if Some(kind) == ctx.trigger {
        proof.trigger_deferred = true;
      } else if Some(kind) == ctx.track {
        proof.track_deferred = true;
      }
    } else if executed && !timer {
      proof.helper_call = true;
    }
  } else if executed && !timer {
    match callee {
      Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
        if mentions_params(&call.callee, ctx.track, ctx.trigger, ctx.aliases, collector) {
          proof.helper_call = true;
        }
      }
      Expression::StaticMemberExpression(member) => {
        if member_callee_can_hold_capability(collector, member, ctx) {
          proof.param_member = true;
        }
      }
      Expression::ComputedMemberExpression(member) => {
        if computed_callee_can_hold_capability(collector, member, ctx) {
          proof.param_member = true;
        }
      }
      Expression::TaggedTemplateExpression(_) => {
        proof.helper_call = true;
      }
      _ => {}
    }
  }
  walk_call_args(collector, &call.arguments, ctx, executed, timer, proof);
}

fn walk_call_args(
  collector: &mut Collector<'_>,
  arguments: &[Argument<'_>],
  ctx: &WalkCtx<'_>,
  executed: bool,
  timer: bool,
  proof: &mut BodyProof,
) {
  for argument in arguments {
    match argument {
      Argument::SpreadElement(spread) => {
        if executed {
          proof.unknown_flow = true;
        }
        walk_expr(collector, &spread.argument, ctx, executed, proof);
      }
      other => {
        let Some(arg) = other.as_expression() else {
          if executed {
            proof.unknown_flow = true;
          }
          continue;
        };
        note_arg(collector, arg, ctx, timer, proof);
        let arg_executed = executed && !is_function_expr(arg);
        walk_expr(collector, arg, ctx, arg_executed, proof);
      }
    }
  }
}

fn walk_object_expr(
  collector: &mut Collector<'_>,
  object: &ObjectExpression<'_>,
  ctx: &WalkCtx<'_>,
  executed: bool,
  proof: &mut BodyProof,
) {
  for property in &object.properties {
    match property {
      ObjectPropertyKind::ObjectProperty(property) => {
        match computed_key_expression(property) {
          Some(key) => walk_expr(collector, key, ctx, executed, proof),
          None if property.computed && executed => proof.unknown_flow = true,
          None => {}
        }
        if object_value_is_eager(property) {
          walk_expr(collector, &property.value, ctx, executed, proof);
        } else if is_nested_callable(&property.value) {
          walk_expr(collector, &property.value, ctx, false, proof);
        } else if executed {
          proof.unknown_flow = true;
        }
      }
      ObjectPropertyKind::SpreadProperty(spread) => {
        walk_expr(collector, &spread.argument, ctx, executed, proof);
      }
    }
  }
}

fn walk_param_defaults(
  collector: &mut Collector<'_>,
  params: &FormalParameters<'_>,
  ctx: &WalkCtx<'_>,
  proof: &mut BodyProof,
) {
  for item in &params.items {
    walk_binding_pattern(collector, &item.pattern, ctx, proof);
    if let Some(default) = &item.initializer {
      walk_expr(collector, default, ctx, true, proof);
    }
  }
  if let Some(rest) = &params.rest {
    walk_binding_pattern(collector, &rest.rest.argument, ctx, proof);
  }
}

fn walk_binding_pattern(
  collector: &mut Collector<'_>,
  pattern: &BindingPattern<'_>,
  ctx: &WalkCtx<'_>,
  proof: &mut BodyProof,
) {
  if proof.nodes >= MAX_BODY_NODES {
    proof.unknown_flow = true;
    return;
  }
  proof.nodes = proof.nodes.saturating_add(1);
  collector.indexes.note_node();
  match pattern {
    BindingPattern::BindingIdentifier(_) => {}
    BindingPattern::AssignmentPattern(inner) => {
      walk_binding_pattern(collector, &inner.left, ctx, proof);
      walk_expr(collector, &inner.right, ctx, true, proof);
    }
    BindingPattern::ObjectPattern(object) => {
      for property in &object.properties {
        if property.computed {
          match property.key.as_expression() {
            Some(key) => walk_expr(collector, key, ctx, true, proof),
            None => proof.unknown_flow = true,
          }
        }
        walk_binding_pattern(collector, &property.value, ctx, proof);
      }
      if let Some(rest) = &object.rest {
        walk_binding_pattern(collector, &rest.argument, ctx, proof);
      }
    }
    BindingPattern::ArrayPattern(array) => {
      for element in array.elements.iter().flatten() {
        walk_binding_pattern(collector, element, ctx, proof);
      }
      if let Some(rest) = &array.rest {
        walk_binding_pattern(collector, &rest.argument, ctx, proof);
      }
    }
  }
}

fn walk_assignment_target(
  collector: &mut Collector<'_>,
  target: &AssignmentTarget<'_>,
  ctx: &WalkCtx<'_>,
  executed: bool,
  proof: &mut BodyProof,
) {
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(_) => {}
    AssignmentTarget::TSAsExpression(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    AssignmentTarget::StaticMemberExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
      walk_expr(collector, &member.expression, ctx, executed, proof);
    }
    AssignmentTarget::PrivateFieldExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      for element in array.elements.iter().flatten() {
        walk_assignment_maybe_default(collector, element, ctx, executed, proof);
      }
      if let Some(rest) = &array.rest {
        walk_assignment_target(collector, &rest.target, ctx, executed, proof);
      }
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      for property in &object.properties {
        match property {
          AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
            if let Some(init) = &property.init {
              walk_expr(collector, init, ctx, executed, proof);
            }
          }
          AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
            if property.computed {
              match property.name.as_expression() {
                Some(key) => walk_expr(collector, key, ctx, executed, proof),
                None if executed => proof.unknown_flow = true,
                None => {}
              }
            }
            walk_assignment_maybe_default(collector, &property.binding, ctx, executed, proof);
          }
        }
      }
      if let Some(rest) = &object.rest {
        walk_assignment_target(collector, &rest.target, ctx, executed, proof);
      }
    }
  }
}

fn walk_assignment_maybe_default(
  collector: &mut Collector<'_>,
  target: &AssignmentTargetMaybeDefault<'_>,
  ctx: &WalkCtx<'_>,
  executed: bool,
  proof: &mut BodyProof,
) {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(inner) => {
      walk_assignment_target(collector, &inner.binding, ctx, executed, proof);
      walk_expr(collector, &inner.init, ctx, executed, proof);
    }
    other => {
      if let Some(target) = other.as_assignment_target() {
        walk_assignment_target(collector, target, ctx, executed, proof);
      } else if executed {
        proof.unknown_flow = true;
      }
    }
  }
}

fn walk_simple_assignment_target(
  collector: &mut Collector<'_>,
  target: &SimpleAssignmentTarget<'_>,
  ctx: &WalkCtx<'_>,
  executed: bool,
  proof: &mut BodyProof,
) {
  match target {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(_) => {}
    SimpleAssignmentTarget::TSAsExpression(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    SimpleAssignmentTarget::TSNonNullExpression(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    SimpleAssignmentTarget::TSTypeAssertion(inner) => {
      walk_expr(collector, &inner.expression, ctx, executed, proof);
    }
    SimpleAssignmentTarget::StaticMemberExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
    }
    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
      walk_expr(collector, &member.expression, ctx, executed, proof);
    }
    SimpleAssignmentTarget::PrivateFieldExpression(member) => {
      walk_expr(collector, &member.object, ctx, executed, proof);
    }
  }
}

fn note_arg(
  collector: &Collector<'_>,
  arg: &Expression<'_>,
  ctx: &WalkCtx<'_>,
  timer: bool,
  proof: &mut BodyProof,
) {
  if let Some(ident) = arg.get_inner_expression().get_identifier_reference()
    && let Some(symbol_id) = collector.reference_symbol(ident)
    && let Some(kind) = ctx.aliases.get(&symbol_id).copied()
  {
    if Some(kind) == ctx.track {
      if timer {
        proof.track_deferred = true;
      } else {
        proof.track_passed = true;
      }
    }
    if Some(kind) == ctx.trigger {
      if timer {
        proof.trigger_deferred = true;
      } else {
        proof.trigger_passed = true;
      }
    }
  }
  if mentions_params(arg, ctx.track, ctx.trigger, ctx.aliases, collector) {
    if timer {
      if ctx.track.is_some() {
        proof.track_deferred = true;
      }
      if ctx.trigger.is_some() {
        proof.trigger_deferred = true;
      }
    } else {
      proof.helper_call = true;
    }
  }
}

fn note_backing(
  collector: &mut Collector<'_>,
  object: &oxc_ast::ast::IdentifierReference<'_>,
  property: &str,
  write: bool,
  executed: bool,
  proof: &mut BodyProof,
) {
  if !executed {
    return;
  }
  let Some(symbol_id) = collector.reference_symbol(object) else {
    return;
  };
  let shape = collector.classify_symbol(symbol_id, 8);
  let reactive =
    matches!(shape, Shape::RefLike | Shape::DeepProxy | Shape::ShallowProxy | Shape::ReadonlyProxy);
  if property == "value" && matches!(shape, Shape::RefLike) {
    if write {
      proof.backing_write = true;
    } else {
      proof.backing_read = true;
    }
  } else if reactive && write {
    proof.backing_write = true;
  } else if reactive {
    proof.backing_read = true;
  }
}

fn mentions_params(
  expression: &Expression<'_>,
  track: Option<SymbolId>,
  trigger: Option<SymbolId>,
  aliases: &HashMap<SymbolId, SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  let mut found = false;
  let mut stack = vec![expression.get_inner_expression()];
  let mut budget = 32_u8;
  while let Some(current) = stack.pop() {
    collector.indexes.note_node();
    if budget == 0 {
      return true;
    }
    budget = budget.saturating_sub(1);
    match current {
      Expression::Identifier(identifier) => {
        if let Some(symbol_id) = collector.reference_symbol(identifier)
          && aliases.contains_key(&symbol_id)
          && (track.is_none() && trigger.is_none()
            || aliases
              .get(&symbol_id)
              .is_some_and(|kind| Some(*kind) == track || Some(*kind) == trigger))
        {
          found = true;
        }
      }
      Expression::CallExpression(call) => {
        stack.push(call.callee.get_inner_expression());
        for argument in &call.arguments {
          if let Some(arg) = argument.as_expression() {
            stack.push(arg.get_inner_expression());
          }
        }
      }
      Expression::ObjectExpression(object) => {
        for property in &object.properties {
          if let ObjectPropertyKind::ObjectProperty(property) = property {
            stack.push(property.value.get_inner_expression());
          } else if let ObjectPropertyKind::SpreadProperty(spread) = property {
            stack.push(spread.argument.get_inner_expression());
          }
        }
      }
      Expression::ArrayExpression(array) => {
        for element in &array.elements {
          if let Some(expr) = element.as_expression() {
            stack.push(expr.get_inner_expression());
          }
        }
      }
      Expression::StaticMemberExpression(member) => {
        stack.push(member.object.get_inner_expression());
      }
      Expression::ComputedMemberExpression(member) => {
        stack.push(member.object.get_inner_expression());
        stack.push(member.expression.get_inner_expression());
      }
      Expression::TaggedTemplateExpression(tagged) => {
        stack.push(tagged.tag.get_inner_expression());
      }
      Expression::NewExpression(new_expr) => {
        stack.push(new_expr.callee.get_inner_expression());
        for argument in &new_expr.arguments {
          if let Some(arg) = argument.as_expression() {
            stack.push(arg.get_inner_expression());
          }
        }
      }
      Expression::ArrowFunctionExpression(arrow) => {
        for statement in &arrow.body.statements {
          if let Statement::ExpressionStatement(statement) = statement {
            stack.push(statement.expression.get_inner_expression());
          }
          if let Statement::ReturnStatement(ret) = statement
            && let Some(argument) = &ret.argument
          {
            stack.push(argument.get_inner_expression());
          }
        }
      }
      Expression::FunctionExpression(function) => {
        if let Some(body) = &function.body {
          for statement in &body.statements {
            if let Statement::ExpressionStatement(statement) = statement {
              stack.push(statement.expression.get_inner_expression());
            }
            if let Statement::ReturnStatement(ret) = statement
              && let Some(argument) = &ret.argument
            {
              stack.push(argument.get_inner_expression());
            }
          }
        }
      }
      _ => {}
    }
  }
  found
}

fn is_timer_callee(callee: &Expression<'_>, collector: &Collector<'_>) -> bool {
  let Some(identifier) = callee.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  collector.reference_symbol(identifier).is_none()
    && matches!(
      identifier.name.as_str(),
      "setTimeout" | "setInterval" | "queueMicrotask" | "requestAnimationFrame" | "setImmediate"
    )
}

fn is_same_value_guard(
  statement: &oxc_ast::ast::IfStatement<'_>,
  storage: Option<SymbolId>,
  setter_param: Option<SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  if statement.alternate.is_some() {
    return false;
  }
  if !if_consequent_is_return(&statement.consequent) {
    return false;
  }
  let Expression::BinaryExpression(binary) = statement.test.get_inner_expression() else {
    return false;
  };
  if !matches!(binary.operator, BinaryOperator::StrictEquality) {
    return false;
  }
  let left = ident_symbol(collector, &binary.left);
  let right = ident_symbol(collector, &binary.right);
  matches!(
    (left, right),
    (Some(a), Some(b))
      if (Some(a) == setter_param && Some(b) == storage)
        || (Some(b) == setter_param && Some(a) == storage)
  )
}

fn if_consequent_is_return(statement: &Statement<'_>) -> bool {
  match statement {
    Statement::ReturnStatement(ret) => ret.argument.is_none(),
    Statement::BlockStatement(block) => {
      matches!(
        block.body.as_slice(),
        [Statement::ReturnStatement(ret)] if ret.argument.is_none()
      )
    }
    _ => false,
  }
}

fn ident_symbol(collector: &Collector<'_>, expression: &Expression<'_>) -> Option<SymbolId> {
  let ident = expression.get_inner_expression().get_identifier_reference()?;
  collector.reference_symbol(ident)
}

fn assignment_target_ident<'a>(
  target: &'a oxc_ast::ast::AssignmentTarget<'a>,
) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
  match target {
    oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(ident) => Some(ident),
    _ => None,
  }
}

fn assignment_member<'a>(
  target: &'a oxc_ast::ast::AssignmentTarget<'a>,
) -> Option<(&'a oxc_ast::ast::IdentifierReference<'a>, &'a str, bool)> {
  match target {
    oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) => {
      let object = member.object.get_inner_expression().get_identifier_reference()?;
      Some((object, member.property.name.as_str(), true))
    }
    _ => None,
  }
}

fn update_target_ident<'a>(
  update: &'a oxc_ast::ast::UpdateExpression<'a>,
) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
  match &update.argument {
    oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(ident) => Some(ident),
    _ => None,
  }
}

fn known_prim(
  expression: &Expression<'_>,
  symbol_of: impl Fn(&oxc_ast::ast::IdentifierReference<'_>) -> Option<SymbolId>,
) -> Option<KnownPrim> {
  match expression.get_inner_expression() {
    Expression::NumericLiteral(literal) => Some(KnownPrim::Number(literal.value.to_bits())),
    Expression::BooleanLiteral(literal) => Some(KnownPrim::Bool(literal.value)),
    Expression::StringLiteral(literal) => Some(KnownPrim::Str(literal.value.to_string())),
    Expression::BigIntLiteral(literal) => {
      Some(KnownPrim::BigInt(literal.raw.as_ref()?.to_string()))
    }
    Expression::Identifier(identifier)
      if identifier.name.as_str() == "undefined" && symbol_of(identifier).is_none() =>
    {
      Some(KnownPrim::Undefined)
    }
    _ => None,
  }
}

fn is_function_expr(expression: &Expression<'_>) -> bool {
  matches!(
    expression.get_inner_expression(),
    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
  )
}

fn object_or_array_holds_capability(
  expression: &Expression<'_>,
  aliases: &HashMap<SymbolId, SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  match expression.get_inner_expression() {
    Expression::ObjectExpression(object) => {
      object.properties.iter().any(|property| match property {
        ObjectPropertyKind::SpreadProperty(spread) => {
          mentions_any_alias(&spread.argument, aliases, collector)
        }
        ObjectPropertyKind::ObjectProperty(property) => {
          mentions_any_alias(&property.value, aliases, collector)
        }
      })
    }
    Expression::ArrayExpression(array) => array.elements.iter().any(|element| {
      element.as_expression().is_some_and(|expr| mentions_any_alias(expr, aliases, collector))
    }),
    _ => false,
  }
}

fn mentions_any_alias(
  expression: &Expression<'_>,
  aliases: &HashMap<SymbolId, SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  mentions_params(expression, None, None, aliases, collector)
}

fn member_callee_can_hold_capability(
  collector: &Collector<'_>,
  member: &oxc_ast::ast::StaticMemberExpression<'_>,
  ctx: &WalkCtx<'_>,
) -> bool {
  holder_or_unknown_local_object(&member.object, ctx, collector)
}

fn computed_callee_can_hold_capability(
  collector: &Collector<'_>,
  member: &oxc_ast::ast::ComputedMemberExpression<'_>,
  ctx: &WalkCtx<'_>,
) -> bool {
  holder_or_unknown_local_object(&member.object, ctx, collector)
    || mentions_params(&member.expression, ctx.track, ctx.trigger, ctx.aliases, collector)
}

fn holder_or_unknown_local_object(
  object: &Expression<'_>,
  ctx: &WalkCtx<'_>,
  collector: &Collector<'_>,
) -> bool {
  let Some(ident) = object.get_inner_expression().get_identifier_reference() else {
    return true;
  };
  let Some(symbol_id) = collector.reference_symbol(ident) else {
    return false;
  };
  collector.indexes.note_query();
  let _proven_holder = ctx.aliases.contains_key(&symbol_id)
    || ctx.holders.contains(&symbol_id)
    || ctx.holders.contains(&collector.indexes.root_of(symbol_id));
  true
}

fn mentions_storage(
  expression: &Expression<'_>,
  storage: Option<SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  let Some(storage) = storage else {
    return false;
  };
  ident_symbol(collector, expression) == Some(storage)
    || match expression.get_inner_expression() {
      Expression::BinaryExpression(binary) => {
        mentions_storage(&binary.left, Some(storage), collector)
          || mentions_storage(&binary.right, Some(storage), collector)
      }
      Expression::UnaryExpression(unary) => {
        mentions_storage(&unary.argument, Some(storage), collector)
      }
      Expression::CallExpression(call) => call.arguments.iter().any(|argument| {
        argument
          .as_expression()
          .is_some_and(|expr| mentions_storage(expr, Some(storage), collector))
      }),
      _ => false,
    }
}

fn return_object_escapes_capabilities(
  object: &ObjectExpression<'_>,
  aliases: &HashMap<SymbolId, SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  object.properties.iter().any(|property| match property {
    ObjectPropertyKind::SpreadProperty(spread) => {
      mentions_any_alias(&spread.argument, aliases, collector)
    }
    ObjectPropertyKind::ObjectProperty(property) => {
      let Some(name) = property.key.static_name() else {
        return mentions_any_alias(&property.value, aliases, collector);
      };
      if name == "get" || name == "set" {
        return false;
      }
      mentions_any_alias(&property.value, aliases, collector)
    }
  })
}

fn note_assignment(
  collector: &mut Collector<'_>,
  assignment: &oxc_ast::ast::AssignmentExpression<'_>,
  ctx: &WalkCtx<'_>,
  executed: bool,
  proof: &mut BodyProof,
) {
  if !executed {
    return;
  }
  if assignment_target_writes_symbol(&assignment.left, ctx.setter_param, collector) {
    proof.param_mutated = true;
  }
  if let Some((object, property, write)) = assignment_member(&assignment.left) {
    note_backing(collector, object, property, write, executed, proof);
  }
  if !assignment_target_writes_symbol(&assignment.left, ctx.storage, collector) {
    return;
  }
  proof.storage_write = true;
  let identity = assignment.operator == AssignmentOperator::Assign
    && assignment_target_ident(&assignment.left).is_some()
    && ident_symbol(collector, &assignment.right) == ctx.setter_param
    && ctx.setter_param.is_some()
    && !proof.param_mutated;
  if identity {
    proof.storage_from_param = true;
  } else {
    proof.storage_other_write = true;
  }
}

fn assignment_target_writes_symbol(
  target: &AssignmentTarget<'_>,
  symbol: Option<SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  let Some(symbol) = symbol else {
    return false;
  };
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(ident) => {
      collector.reference_symbol(ident) == Some(symbol)
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      object.properties.iter().any(|property| match property {
        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
          collector.reference_symbol(&property.binding) == Some(symbol)
        }
        AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
          assignment_maybe_default_writes_symbol(&property.binding, Some(symbol), collector)
        }
      }) || object
        .rest
        .as_ref()
        .is_some_and(|rest| assignment_target_writes_symbol(&rest.target, Some(symbol), collector))
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      array
        .elements
        .iter()
        .flatten()
        .any(|element| assignment_maybe_default_writes_symbol(element, Some(symbol), collector))
        || array.rest.as_ref().is_some_and(|rest| {
          assignment_target_writes_symbol(&rest.target, Some(symbol), collector)
        })
    }
    AssignmentTarget::TSAsExpression(inner) => {
      assignment_expression_writes_symbol(&inner.expression, Some(symbol), collector)
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      assignment_expression_writes_symbol(&inner.expression, Some(symbol), collector)
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      assignment_expression_writes_symbol(&inner.expression, Some(symbol), collector)
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      assignment_expression_writes_symbol(&inner.expression, Some(symbol), collector)
    }
    _ => false,
  }
}

fn assignment_maybe_default_writes_symbol(
  target: &AssignmentTargetMaybeDefault<'_>,
  symbol: Option<SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(inner) => {
      assignment_target_writes_symbol(&inner.binding, symbol, collector)
    }
    other => other
      .as_assignment_target()
      .is_some_and(|target| assignment_target_writes_symbol(target, symbol, collector)),
  }
}

fn assignment_expression_writes_symbol(
  expression: &Expression<'_>,
  symbol: Option<SymbolId>,
  collector: &Collector<'_>,
) -> bool {
  ident_symbol(collector, expression) == symbol
}

#[expect(
  clippy::too_many_lines,
  reason = "one factory storage inventory covers assignment, update, sequence, and nested ownership"
)]
fn apply_factory_storage_expr(
  collector: &mut Collector<'_>,
  expression: &Expression<'_>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  if *unknown {
    return;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::AssignmentExpression(assignment) => {
      apply_factory_storage_expr(
        collector,
        &assignment.right,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_assignment_target(
        collector,
        &assignment.left,
        Some(&assignment.right),
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      if assignment_target_writes_symbol(&assignment.left, Some(storage), collector) {
        if assignment.operator == AssignmentOperator::Assign
          && let Some(prim) =
            known_prim(&assignment.right, |ident| collector.reference_symbol(ident))
        {
          *current = Some(prim);
        } else {
          *unknown = true;
        }
      }
    }
    Expression::UpdateExpression(update) => {
      apply_factory_simple_assignment_target(
        collector,
        &update.argument,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      if let Some(ident) = update_target_ident(update)
        && collector.reference_symbol(ident) == Some(storage)
      {
        *unknown = true;
      }
    }
    Expression::SequenceExpression(sequence) => {
      for item in &sequence.expressions {
        apply_factory_storage_expr(collector, item, storage, get_span, set_span, current, unknown);
      }
    }
    Expression::UnaryExpression(unary) => {
      apply_factory_storage_expr(
        collector,
        &unary.argument,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::BinaryExpression(binary) => {
      apply_factory_storage_expr(
        collector,
        &binary.left,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &binary.right,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::LogicalExpression(logical) => {
      apply_factory_storage_expr(
        collector,
        &logical.left,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      match taken_logical_side(
        logical.operator,
        &logical.left,
        collector.semantic,
        collector.indexes.work_counter(),
      ) {
        TakenLogical::Both => apply_factory_storage_expr(
          collector,
          &logical.right,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        ),
        TakenLogical::LeftOnly => {}
        TakenLogical::Unknown => *unknown = true,
      }
    }
    Expression::ConditionalExpression(conditional) => {
      apply_factory_storage_expr(
        collector,
        &conditional.test,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      match taken_conditional_branch(&conditional.test, collector.indexes.work_counter()) {
        TakenBranch::Consequent => apply_factory_storage_expr(
          collector,
          &conditional.consequent,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        ),
        TakenBranch::Alternate => apply_factory_storage_expr(
          collector,
          &conditional.alternate,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        ),
        TakenBranch::Unknown => *unknown = true,
      }
    }
    Expression::CallExpression(call) => {
      apply_factory_storage_expr(
        collector,
        &call.callee,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_call_args(
        collector,
        &call.arguments,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::NewExpression(new_expr) => {
      apply_factory_storage_expr(
        collector,
        &new_expr.callee,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_call_args(
        collector,
        &new_expr.arguments,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ObjectExpression(object) => {
      for property in &object.properties {
        match property {
          ObjectPropertyKind::ObjectProperty(property) => {
            match computed_key_expression(property) {
              Some(key) => apply_factory_storage_expr(
                collector, key, storage, get_span, set_span, current, unknown,
              ),
              None if property.computed => *unknown = true,
              None => {}
            }
            apply_factory_storage_expr(
              collector,
              &property.value,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
          ObjectPropertyKind::SpreadProperty(spread) => {
            apply_factory_storage_expr(
              collector,
              &spread.argument,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
        }
      }
    }
    Expression::ArrayExpression(array) => {
      for element in &array.elements {
        match element {
          ArrayExpressionElement::SpreadElement(spread) => {
            apply_factory_storage_expr(
              collector,
              &spread.argument,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
          ArrayExpressionElement::Elision(_) => {}
          other => {
            if let Some(expr) = other.as_expression() {
              apply_factory_storage_expr(
                collector, expr, storage, get_span, set_span, current, unknown,
              );
            } else {
              *unknown = true;
            }
          }
        }
      }
    }
    Expression::StaticMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ComputedMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &member.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ParenthesizedExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.span == get_span || arrow.span == set_span {
        return;
      }
      if function_mentions_storage(Some(&arrow.body), storage, collector) {
        *unknown = true;
      }
    }
    Expression::FunctionExpression(function) => {
      if function.span == get_span || function.span == set_span {
        return;
      }
      if function_mentions_storage(function.body.as_deref(), storage, collector) {
        *unknown = true;
      }
    }
    Expression::TemplateLiteral(template) => {
      for item in &template.expressions {
        apply_factory_storage_expr(collector, item, storage, get_span, set_span, current, unknown);
      }
    }
    Expression::PrivateFieldExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::Identifier(_)
    | Expression::ThisExpression(_)
    | Expression::Super(_)
    | Expression::ImportMeta(_)
    | Expression::NewTarget(_) => {}
    _ => *unknown = true,
  }
}

fn apply_factory_call_args(
  collector: &mut Collector<'_>,
  arguments: &[Argument<'_>],
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  for argument in arguments {
    match argument {
      Argument::SpreadElement(spread) => {
        apply_factory_storage_expr(
          collector,
          &spread.argument,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        );
      }
      other => {
        if let Some(arg) = other.as_expression() {
          apply_factory_storage_expr(collector, arg, storage, get_span, set_span, current, unknown);
        } else {
          *unknown = true;
        }
      }
    }
  }
}

enum Retrieved<'a> {
  Missing,
  Value(&'a Expression<'a>),
  Unknown,
}

enum ProtoKind<'a> {
  Standard,
  Null,
  Unknown,
  Inherited(Box<SourceProps<'a>>),
}

struct SourceProps<'a> {
  last: HashMap<String, &'a Expression<'a>>,
  proto: ProtoKind<'a>,
}

fn is_object_proto_key(name: &str) -> bool {
  matches!(
    name,
    "constructor"
      | "hasOwnProperty"
      | "isPrototypeOf"
      | "propertyIsEnumerable"
      | "toLocaleString"
      | "toString"
      | "valueOf"
  )
}

fn empty_source_props<'a>() -> SourceProps<'a> {
  SourceProps { last: HashMap::new(), proto: ProtoKind::Standard }
}

fn is_proto_setter(property: &oxc_ast::ast::ObjectProperty<'_>) -> bool {
  !property.computed
    && !property.shorthand
    && !property.method
    && property.kind == PropertyKind::Init
    && property.key.static_name().as_deref() == Some("__proto__")
}

const fn consume_bind(budget: &mut u32) -> bool {
  if *budget == 0 {
    return false;
  }
  *budget = budget.saturating_sub(1);
  true
}

#[expect(
  clippy::too_many_arguments,
  reason = "factory binding walk threads storage inventory, initializer, and the bind budget"
)]
fn apply_factory_binding_pattern(
  collector: &mut Collector<'_>,
  pattern: &BindingPattern<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
  budget: &mut u32,
) {
  if *unknown {
    return;
  }
  if !consume_bind(budget) {
    *unknown = true;
    return;
  }
  collector.indexes.note_node();
  match pattern {
    BindingPattern::BindingIdentifier(_) => {}
    BindingPattern::AssignmentPattern(_) | BindingPattern::ArrayPattern(_) => *unknown = true,
    BindingPattern::ObjectPattern(object) => {
      if object.rest.is_some() {
        *unknown = true;
        return;
      }
      let summary = source.map_or_else(
        || Some(empty_source_props()),
        |expr| summarize_source_object(expr, collector, budget),
      );
      if source.is_some() && summary.is_none() {
        *unknown = true;
        return;
      }
      for property in &object.properties {
        if *unknown {
          return;
        }
        if !consume_bind(budget) {
          *unknown = true;
          return;
        }
        collector.indexes.note_node();
        if property.computed {
          match property.key.as_expression() {
            Some(key) => apply_factory_storage_expr(
              collector, key, storage, get_span, set_span, current, unknown,
            ),
            None => *unknown = true,
          }
        }
        if *unknown {
          return;
        }
        let retrieved = if property.computed {
          Retrieved::Unknown
        } else if let Some(name) = property.key.static_name() {
          lookup_source_prop(summary.as_ref(), name.as_ref(), collector, budget)
        } else {
          Retrieved::Unknown
        };
        match &property.value {
          BindingPattern::AssignmentPattern(inner) => {
            let activate = match retrieved {
              Retrieved::Missing => Some(true),
              Retrieved::Value(expr) => proven_retrieved_undefined(expr, collector, budget),
              Retrieved::Unknown => None,
            };
            match activate {
              Some(true) => apply_factory_storage_expr(
                collector,
                &inner.right,
                storage,
                get_span,
                set_span,
                current,
                unknown,
              ),
              Some(false) => {}
              None => *unknown = true,
            }
            let nested = match activate {
              Some(true) => Some(&inner.right),
              Some(false) => match retrieved {
                Retrieved::Value(expr) => Some(expr),
                Retrieved::Missing | Retrieved::Unknown => None,
              },
              None => None,
            };
            apply_factory_binding_pattern(
              collector,
              &inner.left,
              nested,
              storage,
              get_span,
              set_span,
              current,
              unknown,
              budget,
            );
          }
          BindingPattern::BindingIdentifier(_) => {}
          other => {
            let nested = match retrieved {
              Retrieved::Value(expr) => Some(expr),
              Retrieved::Missing | Retrieved::Unknown => {
                *unknown = true;
                None
              }
            };
            apply_factory_binding_pattern(
              collector, other, nested, storage, get_span, set_span, current, unknown, budget,
            );
          }
        }
      }
    }
  }
}

fn summarize_source_object<'a>(
  expression: &'a Expression<'a>,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> Option<SourceProps<'a>> {
  if !consume_bind(budget) {
    return None;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::SequenceExpression(sequence) => {
      sequence.expressions.last().and_then(|item| summarize_source_object(item, collector, budget))
    }
    Expression::ObjectExpression(object) => {
      let mut last = HashMap::new();
      let mut proto = ProtoKind::Standard;
      for property in &object.properties {
        if !consume_bind(budget) {
          return None;
        }
        collector.indexes.work_counter().add_object_entries(1);
        match property {
          ObjectPropertyKind::SpreadProperty(_) => return None,
          ObjectPropertyKind::ObjectProperty(property) => {
            if is_proto_setter(property) {
              proto = classify_proto_value(&property.value, collector, budget);
              continue;
            }
            if property.computed || property.method || property.kind != PropertyKind::Init {
              return None;
            }
            let name = property.key.static_name()?;
            collector.indexes.work_counter().add_writes(1);
            last.insert(name.into_owned(), &property.value);
          }
        }
      }
      Some(SourceProps { last, proto })
    }
    _ => None,
  }
}

fn classify_proto_value<'a>(
  expression: &'a Expression<'a>,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> ProtoKind<'a> {
  if !consume_bind(budget) {
    return ProtoKind::Unknown;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::NullLiteral(_) => ProtoKind::Null,
    Expression::ObjectExpression(_) => summarize_source_object(expression, collector, budget)
      .map_or(ProtoKind::Unknown, |nested| ProtoKind::Inherited(Box::new(nested))),
    Expression::SequenceExpression(sequence) => sequence
      .expressions
      .last()
      .map_or(ProtoKind::Unknown, |item| classify_proto_value(item, collector, budget)),
    _ => ProtoKind::Unknown,
  }
}

fn lookup_source_prop<'a>(
  summary: Option<&SourceProps<'a>>,
  name: &str,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> Retrieved<'a> {
  collector.indexes.note_query();
  let Some(summary) = summary else {
    return Retrieved::Unknown;
  };
  if !consume_bind(budget) {
    return Retrieved::Unknown;
  }
  if let Some(expr) = summary.last.get(name) {
    return Retrieved::Value(expr);
  }
  match &summary.proto {
    ProtoKind::Null => Retrieved::Missing,
    ProtoKind::Standard => {
      if is_object_proto_key(name) {
        Retrieved::Unknown
      } else {
        Retrieved::Missing
      }
    }
    ProtoKind::Inherited(inner) => {
      lookup_source_prop(Some(inner.as_ref()), name, collector, budget)
    }
    ProtoKind::Unknown => Retrieved::Unknown,
  }
}

fn proven_retrieved_undefined(
  expression: &Expression<'_>,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> Option<bool> {
  if !consume_bind(budget) {
    return None;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) if identifier.name.as_str() == "undefined" => {
      match collector.reference_symbol(identifier) {
        None => Some(true),
        Some(symbol) => {
          let prim = const_known_prim(collector, symbol, budget)?;
          Some(matches!(prim, KnownPrim::Undefined))
        }
      }
    }
    Expression::Identifier(identifier) => {
      let symbol = collector.reference_symbol(identifier)?;
      let prim = const_known_prim(collector, symbol, budget)?;
      Some(matches!(prim, KnownPrim::Undefined))
    }
    Expression::NullLiteral(_)
    | Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::ObjectExpression(_)
    | Expression::ArrayExpression(_)
    | Expression::FunctionExpression(_)
    | Expression::ArrowFunctionExpression(_)
    | Expression::ClassExpression(_) => Some(false),
    Expression::SequenceExpression(sequence) => sequence
      .expressions
      .last()
      .and_then(|item| proven_retrieved_undefined(item, collector, budget)),
    _ => None,
  }
}

fn const_known_prim(
  collector: &Collector<'_>,
  symbol: SymbolId,
  budget: &mut u32,
) -> Option<KnownPrim> {
  if !consume_bind(budget) {
    return None;
  }
  collector.indexes.note_query();
  if !collector.semantic.scoping().symbol_flags(symbol).contains(SymbolFlags::ConstVariable) {
    return None;
  }
  let mut node_id = collector.semantic.scoping().symbol_declaration(symbol);
  for _ in 0..8 {
    if !consume_bind(budget) {
      return None;
    }
    collector.indexes.note_node();
    match collector.semantic.nodes().kind(node_id) {
      AstKind::VariableDeclarator(declarator) => {
        let init = declarator.init.as_ref()?;
        return known_prim(init, |ident| collector.reference_symbol(ident));
      }
      AstKind::Program(_) => return None,
      _ => node_id = collector.semantic.nodes().parent_id(node_id),
    }
  }
  None
}

#[expect(
  clippy::too_many_arguments,
  reason = "object assignment uses the same factory inventory plus a cached source summary"
)]
fn apply_factory_object_assignment(
  collector: &mut Collector<'_>,
  object: &oxc_ast::ast::ObjectAssignmentTarget<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  if object.rest.is_some() {
    *unknown = true;
    return;
  }
  let mut budget = FACTORY_BIND_BUDGET;
  let summary = source.map_or_else(
    || Some(empty_source_props()),
    |expr| summarize_source_object(expr, collector, &mut budget),
  );
  if source.is_some() && summary.is_none() {
    *unknown = true;
    return;
  }
  for property in &object.properties {
    if *unknown {
      return;
    }
    if !consume_bind(&mut budget) {
      *unknown = true;
      return;
    }
    collector.indexes.note_node();
    match property {
      AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
        let retrieved = lookup_source_prop(
          summary.as_ref(),
          property.binding.name.as_str(),
          collector,
          &mut budget,
        );
        if let Some(default) = &property.init {
          let activate = match retrieved {
            Retrieved::Missing => Some(true),
            Retrieved::Value(expr) => proven_retrieved_undefined(expr, collector, &mut budget),
            Retrieved::Unknown => None,
          };
          match activate {
            Some(true) => apply_factory_storage_expr(
              collector, default, storage, get_span, set_span, current, unknown,
            ),
            Some(false) => {}
            None => *unknown = true,
          }
        }
      }
      AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
        if property.computed {
          match property.name.as_expression() {
            Some(key) => apply_factory_storage_expr(
              collector, key, storage, get_span, set_span, current, unknown,
            ),
            None => *unknown = true,
          }
        }
        if *unknown {
          return;
        }
        let retrieved = if property.computed {
          Retrieved::Unknown
        } else if let Some(name) = property.name.static_name() {
          lookup_source_prop(summary.as_ref(), name.as_ref(), collector, &mut budget)
        } else {
          Retrieved::Unknown
        };
        match &property.binding {
          AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(inner) => {
            let activate = match retrieved {
              Retrieved::Missing => Some(true),
              Retrieved::Value(expr) => proven_retrieved_undefined(expr, collector, &mut budget),
              Retrieved::Unknown => None,
            };
            match activate {
              Some(true) => apply_factory_storage_expr(
                collector,
                &inner.init,
                storage,
                get_span,
                set_span,
                current,
                unknown,
              ),
              Some(false) => {}
              None => *unknown = true,
            }
            let nested = match activate {
              Some(true) => Some(&inner.init),
              Some(false) => match retrieved {
                Retrieved::Value(expr) => Some(expr),
                Retrieved::Missing | Retrieved::Unknown => None,
              },
              None => None,
            };
            apply_factory_assignment_target(
              collector,
              &inner.binding,
              nested,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
          other => {
            let nested = match retrieved {
              Retrieved::Value(expr) => Some(expr),
              Retrieved::Missing | Retrieved::Unknown => {
                if other.as_assignment_target().is_some_and(|target| {
                  !matches!(target, AssignmentTarget::AssignmentTargetIdentifier(_))
                }) {
                  *unknown = true;
                }
                None
              }
            };
            apply_factory_assignment_maybe_default(
              collector, other, nested, storage, get_span, set_span, current, unknown,
            );
          }
        }
      }
    }
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "factory assignment-target walk threads storage inventory plus the rhs used for default activation"
)]
fn apply_factory_assignment_target(
  collector: &mut Collector<'_>,
  target: &AssignmentTarget<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(_) => {}
    AssignmentTarget::TSAsExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::StaticMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &member.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::PrivateFieldExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::ArrayAssignmentTarget(_) => *unknown = true,
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      apply_factory_object_assignment(
        collector, object, source, storage, get_span, set_span, current, unknown,
      );
    }
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "factory assignment defaults thread storage inventory plus the extracted source value"
)]
fn apply_factory_assignment_maybe_default(
  collector: &mut Collector<'_>,
  target: &AssignmentTargetMaybeDefault<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(inner) => {
      if source.is_some() {
        apply_factory_assignment_target(
          collector,
          &inner.binding,
          source,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        );
      } else {
        *unknown = true;
      }
    }
    other => {
      if let Some(target) = other.as_assignment_target() {
        apply_factory_assignment_target(
          collector, target, source, storage, get_span, set_span, current, unknown,
        );
      } else {
        *unknown = true;
      }
    }
  }
}

fn apply_factory_simple_assignment_target(
  collector: &mut Collector<'_>,
  target: &SimpleAssignmentTarget<'_>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  match target {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(_) => {}
    SimpleAssignmentTarget::TSAsExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::TSNonNullExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::TSTypeAssertion(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::StaticMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &member.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::PrivateFieldExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
  }
}

fn function_mentions_storage(
  body: Option<&FunctionBody<'_>>,
  storage: SymbolId,
  collector: &Collector<'_>,
) -> bool {
  let Some(body) = body else {
    return false;
  };
  body.statements.iter().any(|statement| match statement {
    Statement::ExpressionStatement(statement) => {
      mentions_storage(&statement.expression, Some(storage), collector)
    }
    Statement::ReturnStatement(ret) => ret
      .argument
      .as_ref()
      .is_some_and(|argument| mentions_storage(argument, Some(storage), collector)),
    Statement::VariableDeclaration(declaration) => {
      declaration.declarations.iter().any(|declarator| {
        declarator
          .init
          .as_ref()
          .is_some_and(|init| mentions_storage(init, Some(storage), collector))
      })
    }
    _ => false,
  })
}
