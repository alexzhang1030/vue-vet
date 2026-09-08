//! One-pass indexes: Vue imports, aliases, writes, escapes, span maps.

use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentOperator, AssignmentTarget, AssignmentTargetMaybeDefault,
    AssignmentTargetProperty, CallExpression, Expression, IdentifierReference, ObjectPropertyKind,
    PropertyKind, SimpleAssignmentTarget,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};
use oxc_syntax::reference::ReferenceFlags;
use vue_vet_core::{ScriptKind, SourceSpan};

use super::shape::{
  ShapeHint, VueImport, hint_of, is_fresh_allocation, is_unresolved_collection, resolve_vue_api,
  span_key,
};
use super::stats::WorkCounter;
use crate::facts::source_span;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StmtSite {
  pub block: NodeId,
  pub callable: Option<NodeId>,
  pub expr_offset: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ValueWrite {
  pub offset: usize,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MemberWrite {
  pub offset: usize,
  pub callable: Option<NodeId>,
  pub block: NodeId,
  pub rhs: Span,
  pub simple_assign: bool,
  pub fresh_alloc: bool,
}

/// Direct `obj.prop = rhs` only. Assignment patterns restore generic
/// uncertainty on the member object.
#[derive(Clone, Copy)]
struct DirectMemberWrite<'a> {
  offset: usize,
  callable: Option<NodeId>,
  block: NodeId,
  right: &'a Expression<'a>,
  simple: bool,
  fresh: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CallInfo {
  pub api: Option<&'static str>,
  pub first_arg: Option<Span>,
  pub has_spread: bool,
}

#[derive(Clone, Debug)]
pub(super) enum ObjectEntry {
  Spread,
  Computed,
  Accessor { name: Option<String> },
  Data { name: String, key: Span, value: Span },
  Method { name: String, key: Span, value: Span },
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ObjectProp {
  Unknown,
  Value(Span),
}

#[derive(Clone, Copy)]
struct Owner {
  callable: Option<NodeId>,
  block: Option<NodeId>,
}

pub(super) struct Indexes {
  pub vue_imports: HashMap<SymbolId, VueImport>,
  pub alias_root: HashMap<SymbolId, SymbolId>,
  pub escaped: HashSet<SymbolId>,
  pub uncertain: HashSet<SymbolId>,
  pub reassigned: HashSet<SymbolId>,
  pub unknown_member_touch: HashSet<SymbolId>,
  pub value_writes: HashMap<SymbolId, Vec<ValueWrite>>,
  pub member_writes: HashMap<(SymbolId, String), Vec<MemberWrite>>,
  pub hints: HashMap<u64, ShapeHint>,
  pub calls: HashMap<u64, CallInfo>,
  pub objects: HashMap<u64, Vec<ObjectEntry>>,
  pub object_props: HashMap<u64, HashMap<String, ObjectProp>>,
  pub collections: HashSet<u64>,
  pub arrays: HashSet<u64>,
  pub closed_objects: HashMap<u64, bool>,
  pub capability_uncertain: HashSet<SymbolId>,
  pub stmt_site: HashMap<NodeId, StmtSite>,
  pub events_by_block: HashMap<NodeId, Vec<usize>>,
  pub init_span: HashMap<SymbolId, Span>,
  pub value_write_roots: HashSet<SymbolId>,
  pub member_write_roots: HashSet<SymbolId>,
  mixed_value_owners: HashSet<SymbolId>,
  mixed_member_owners: HashSet<(SymbolId, String)>,
  value_write_owner: HashMap<SymbolId, (Option<NodeId>, NodeId)>,
  member_write_owner: HashMap<(SymbolId, String), (Option<NodeId>, NodeId)>,
  owners: HashMap<NodeId, Owner>,
  work: WorkCounter,
}

impl Indexes {
  pub(super) fn build(
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
    vue_imports: HashMap<SymbolId, VueImport>,
    work: WorkCounter,
  ) -> Self {
    let mut indexes = Self {
      vue_imports,
      alias_root: HashMap::new(),
      escaped: HashSet::new(),
      uncertain: HashSet::new(),
      reassigned: HashSet::new(),
      unknown_member_touch: HashSet::new(),
      value_writes: HashMap::new(),
      member_writes: HashMap::new(),
      hints: HashMap::new(),
      calls: HashMap::new(),
      objects: HashMap::new(),
      object_props: HashMap::new(),
      collections: HashSet::new(),
      arrays: HashSet::new(),
      closed_objects: HashMap::new(),
      capability_uncertain: HashSet::new(),
      stmt_site: HashMap::new(),
      events_by_block: HashMap::new(),
      init_span: HashMap::new(),
      value_write_roots: HashSet::new(),
      member_write_roots: HashSet::new(),
      mixed_value_owners: HashSet::new(),
      mixed_member_owners: HashSet::new(),
      value_write_owner: HashMap::new(),
      member_write_owner: HashMap::new(),
      owners: HashMap::new(),
      work,
    };
    indexes.build_owners(semantic);
    indexes.scan(semantic, line_index, sfc_source, script_offset, kind);
    indexes.finish_aliases_and_roles(semantic);
    indexes.summarize_writes();
    indexes.precompute_closed_objects();
    for events in indexes.events_by_block.values_mut() {
      events.sort_unstable();
    }
    for writes in indexes.member_writes.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    for writes in indexes.value_writes.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    indexes
  }

  pub(super) const fn stats(&self) -> super::stats::SourceContractStats {
    self.work.snapshot()
  }

  pub(super) fn root_of(&self, symbol_id: SymbolId) -> SymbolId {
    self.alias_root.get(&symbol_id).copied().unwrap_or(symbol_id)
  }

  pub(super) fn payload_uncertain(&self, symbol_id: SymbolId) -> bool {
    let root = self.root_of(symbol_id);
    self.uncertain.contains(&root)
      || self.escaped.contains(&root)
      || self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
  }

  pub(super) fn has_event_between(&self, block: NodeId, start: usize, end: usize) -> bool {
    let Some(events) = self.events_by_block.get(&block) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(events, |offset| *offset <= start);
    self.work.add_queries(1);
    events.get(index).is_some_and(|offset| *offset < end)
  }

  pub(super) fn last_value_write_before(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueWrite> {
    if self.mixed_value_owners.contains(&root) {
      self.work.add_queries(1);
      return None;
    }
    let Some(writes) = self.value_writes.get(&root) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(writes, |write| write.offset < offset);
    self.work.add_queries(1);
    let prior = index.checked_sub(1).and_then(|index| writes.get(index)).copied()?;
    (prior.callable == callable && prior.block == block).then_some(prior)
  }

  pub(super) fn value_writes_mixed(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.mixed_value_owners.contains(&root)
  }

  pub(super) fn value_write_owner_mismatch(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> bool {
    self.work.add_queries(1);
    self.value_write_owner.get(&root).is_some_and(|owner| *owner != (callable, block))
  }

  pub(super) fn member_writes_mixed(&self, root: SymbolId, property: &str) -> bool {
    self.work.add_queries(1);
    self.mixed_member_owners.contains(&(root, property.to_string()))
  }

  pub(super) fn member_write_owner_mismatch(
    &self,
    root: SymbolId,
    property: &str,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> bool {
    self.work.add_queries(1);
    self
      .member_write_owner
      .get(&(root, property.to_string()))
      .is_some_and(|owner| *owner != (callable, block))
  }

  pub(super) fn last_member_write_before(
    &self,
    root: SymbolId,
    property: &str,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<MemberWrite> {
    if self.mixed_member_owners.contains(&(root, property.to_string())) {
      self.work.add_queries(1);
      return None;
    }
    let Some(writes) = self.member_writes.get(&(root, property.to_string())) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(writes, |write| write.offset < offset);
    self.work.add_queries(1);
    let prior = index.checked_sub(1).and_then(|index| writes.get(index)).copied()?;
    (prior.callable == callable && prior.block == block).then_some(prior)
  }

  pub(super) fn first_member_write_after(
    &self,
    root: SymbolId,
    property: &str,
    block: NodeId,
    offset: usize,
  ) -> Option<MemberWrite> {
    let Some(writes) = self.member_writes.get(&(root, property.to_string())) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(writes, |write| write.offset <= offset);
    self.work.add_queries(1);
    let next = writes.get(index).copied()?;
    (next.simple_assign && next.fresh_alloc && next.block == block).then_some(next)
  }

  pub(super) fn object_prop(&self, object_span: Span, property: &str) -> Option<ObjectProp> {
    self.work.add_queries(1);
    self.object_props.get(&span_key(object_span)).and_then(|props| props.get(property)).copied()
  }

  /// Proven closed object literal, or `None` when `span` is not an object.
  /// Precomputed once per object span; lookup charges a query, not a rescan.
  pub(super) fn closed_object_literal(&self, span: Span) -> Option<bool> {
    self.work.add_queries(1);
    self.closed_objects.get(&span_key(span)).copied()
  }

  pub(super) fn is_array_literal(&self, span: Span) -> bool {
    self.work.add_queries(1);
    self.arrays.contains(&span_key(span))
  }

  /// Capability-changing mutation or unknown helper use of a construction /
  /// watched root. Ordinary `state.n` writes stay ordinary member writes.
  /// Unknown flow uses the dedicated `capability_uncertain` role index:
  /// storage, return, unknown call, spread, sequence, receiver, dynamic target.
  /// Generic `escaped` / `uncertain` remain watch/reactive-argument facts.
  pub(super) fn construction_mutated(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
      || self.capability_uncertain.contains(&root)
      || self.has_capability_member_write(root)
  }

  pub(super) fn has_capability_member_write(&self, root: SymbolId) -> bool {
    CAPABILITY_KEYS.iter().any(|key| {
      self.work.add_queries(1);
      self.member_writes.contains_key(&(root, (*key).to_string()))
    })
  }

  fn build_owners(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    for (node_id, node) in semantic.nodes().iter_enumerated() {
      self.work.add_owners(1);
      let parent = semantic.nodes().parent_id(node_id);
      let inherited =
        self.owners.get(&parent).copied().unwrap_or(Owner { callable: None, block: None });
      let mut callable = inherited.callable;
      let mut block = inherited.block;
      match node.kind() {
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => callable = Some(node_id),
        AstKind::Program(_) | AstKind::FunctionBody(_) | AstKind::BlockStatement(_) => {
          block = Some(node_id);
        }
        _ => {}
      }
      self.owners.insert(node_id, Owner { callable, block });
    }
  }

  fn summarize_writes(&mut self) {
    for (root, writes) in &self.value_writes {
      let Some(first) = writes.first() else {
        continue;
      };
      self.work.add_writes(1);
      let mixed = writes.iter().skip(1).any(|write| {
        self.work.add_writes(1);
        write.callable != first.callable || write.block != first.block
      });
      if mixed {
        self.mixed_value_owners.insert(*root);
      } else {
        self.value_write_owner.insert(*root, (first.callable, first.block));
      }
    }
    for (key, writes) in &self.member_writes {
      let Some(first) = writes.first() else {
        continue;
      };
      self.work.add_writes(1);
      let mixed = writes.iter().skip(1).any(|write| {
        self.work.add_writes(1);
        write.callable != first.callable || write.block != first.block
      });
      if mixed {
        self.mixed_member_owners.insert(key.clone());
      } else {
        self.member_write_owner.insert(key.clone(), (first.callable, first.block));
      }
    }
  }

  fn precompute_closed_objects(&mut self) {
    for (key, entries) in &self.objects {
      self.work.add_queries(1);
      let mut closed = true;
      for entry in entries {
        self.work.add_object_entries(1);
        match entry {
          ObjectEntry::Spread | ObjectEntry::Computed | ObjectEntry::Accessor { .. } => {
            closed = false;
            break;
          }
          ObjectEntry::Data { name, .. } | ObjectEntry::Method { name, .. }
            if is_capability_key(name) =>
          {
            closed = false;
            break;
          }
          ObjectEntry::Data { .. } | ObjectEntry::Method { .. } => {}
        }
      }
      self.closed_objects.insert(*key, closed);
    }
  }

  fn owner(&self, node_id: NodeId) -> Owner {
    self.owners.get(&node_id).copied().unwrap_or(Owner { callable: None, block: None })
  }

  pub(super) fn note_node(&self) {
    self.work.add_nodes(1);
  }

  pub(super) fn note_query(&self) {
    self.work.add_queries(1);
  }

  pub(super) fn add_queries(&self, n: u64) {
    self.work.add_queries(n);
  }

  pub(super) const fn work(&self) -> &WorkCounter {
    &self.work
  }

  fn scan(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
  ) {
    for (node_id, node) in semantic.nodes().iter_enumerated() {
      self.work.add_nodes(1);
      match node.kind() {
        AstKind::VariableDeclarator(declarator) => {
          if matches!(
            semantic.nodes().parent_kind(semantic.nodes().parent_id(node_id)),
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
          ) && let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
            && let Some(symbol_id) = binding.symbol_id.get()
          {
            self.escaped.insert(self.root_of(symbol_id));
          }
          if let Some(init) = &declarator.init {
            self.record_expr(semantic, kind, init);
            if let Some(ident) = init.get_inner_expression().get_identifier_reference()
              && let Some(target) = reference_symbol(semantic, ident)
            {
              match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(binding) => {
                  if let Some(local) = binding.symbol_id.get() {
                    if semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
                      let root = self.root_of(target);
                      self.alias_root.insert(local, root);
                    } else {
                      self.escaped.insert(self.root_of(target));
                    }
                  }
                }
                _ => {
                  self.escaped.insert(self.root_of(target));
                }
              }
            }
          }
          if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
            && let Some(symbol_id) = binding.symbol_id.get()
            && let Some(init) = &declarator.init
          {
            self.init_span.insert(symbol_id, init.span());
          }
        }
        AstKind::AssignmentExpression(assignment) => {
          self.record_expr(semantic, kind, &assignment.right);
          self.mark_escape_expr(semantic, &assignment.right);
          let offset = mapped(line_index, sfc_source, script_offset, assignment.span).offset;
          self.index_assignment(
            semantic,
            node_id,
            offset,
            assignment.operator,
            &assignment.left,
            &assignment.right,
          );
          self.record_stmt_site(semantic, line_index, sfc_source, script_offset, node_id);
        }
        AstKind::UpdateExpression(update) => {
          if let SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) = &update.argument
            && let Some(symbol_id) = reference_symbol(semantic, identifier)
          {
            self.reassigned.insert(self.root_of(symbol_id));
          }
        }
        AstKind::CallExpression(call) => {
          self.record_call(semantic, kind, call);
          for argument in &call.arguments {
            if let Some(expression) = argument.as_expression() {
              self.record_expr(semantic, kind, expression);
              self.mark_escape_expr(semantic, expression);
            }
          }
          self.record_stmt_site(semantic, line_index, sfc_source, script_offset, node_id);
          self.record_event(semantic, line_index, sfc_source, script_offset, node_id, call.span);
        }
        AstKind::NewExpression(expression) => {
          self.record_expr(semantic, kind, &expression.callee);
          for argument in &expression.arguments {
            if let Some(arg) = argument.as_expression() {
              self.record_expr(semantic, kind, arg);
              self.mark_escape_expr(semantic, arg);
            }
          }
          if is_unresolved_collection(&expression.callee, |ident| reference_symbol(semantic, ident))
          {
            self.collections.insert(span_key(expression.span));
          }
        }
        AstKind::ObjectExpression(object) => {
          let entries = object_entries(object);
          let props = summarize_object_props(&entries, &self.work);
          self.object_props.insert(span_key(object.span), props);
          self.objects.insert(span_key(object.span), entries);
          for property in &object.properties {
            if let ObjectPropertyKind::ObjectProperty(property) = property {
              self.record_expr(semantic, kind, &property.value);
              self.mark_escape_expr(semantic, &property.value);
            }
          }
        }
        AstKind::ArrayExpression(array) => {
          self.arrays.insert(span_key(array.span));
          for element in &array.elements {
            if let Some(expression) = element.as_expression() {
              self.record_expr(semantic, kind, expression);
              self.mark_escape_expr(semantic, expression);
            }
          }
        }
        AstKind::ReturnStatement(statement) => {
          if let Some(argument) = &statement.argument {
            self.record_expr(semantic, kind, argument);
            self.mark_escape_expr(semantic, argument);
          }
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::SpreadElement(spread) => self.mark_escape_expr(semantic, &spread.argument),
        AstKind::IfStatement(statement) => {
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::ForStatement(statement) => {
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::WhileStatement(statement) => {
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::SwitchStatement(statement) => {
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::TryStatement(statement) => {
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
          if let AstKind::VariableDeclaration(_) = semantic.nodes().parent_kind(node_id) {
            // handled via BindingIdentifier export flags below
          }
        }
        _ => {}
      }
    }
  }

  fn finish_aliases_and_roles(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    for symbol_id in semantic.scoping().symbol_ids() {
      for reference in semantic.symbol_references(symbol_id) {
        self.work.add_references(1);
        let root = self.root_of(symbol_id);
        let flags = reference.flags();
        let member_payload = flags.intersects(ReferenceFlags::MemberWriteTarget)
          || known_static_member_object_role(semantic, reference.node_id());
        if member_payload {
          if flags.is_write() && !self.has_simple_value_write(root) && !self.has_member_write(root)
          {
            self.uncertain.insert(root);
          }
          if known_static_member_object_role(semantic, reference.node_id())
            && self.static_member_chain_capability_uncertain(semantic, reference.node_id())
          {
            self.capability_uncertain.insert(root);
          }
          continue;
        }
        if flags.is_write() {
          self.reassigned.insert(root);
          continue;
        }
        if known_const_alias_role(semantic, reference.node_id()) {
          continue;
        }
        self.uncertain.insert(root);
        if !self.known_vue_source_argument_role(semantic, reference.node_id()) {
          self.capability_uncertain.insert(root);
        }
      }
    }
  }

  fn known_vue_source_argument_role(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
  ) -> bool {
    self.work.add_queries(1);
    let ident_span = semantic.nodes().kind(node_id).span();
    let mut current = node_id;
    for _ in 0..MAX_ROLE_ANCESTORS {
      let parent_id = semantic.nodes().parent_id(current);
      self.work.add_queries(1);
      match semantic.nodes().kind(parent_id) {
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::ChainExpression(_) => {
          current = parent_id;
        }
        AstKind::CallExpression(call) => {
          let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
            return false;
          };
          if !is_known_constructor_or_watch(info.api) {
            return false;
          }
          let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
            return false;
          };
          let inner = first.get_inner_expression();
          return inner.span() == ident_span || first.span() == ident_span;
        }
        _ => return false,
      }
    }
    false
  }

  /// Receiver call / `new` / tagged template / unknown member-chain use of a
  /// static member object. Ordinary `state.n` reads and writes stay outside
  /// this set. Import sources, JSX member tags, and decorator expressions stay
  /// on the default (proven) branch; those positions bind no JS `this` receiver.
  fn static_member_chain_capability_uncertain(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
  ) -> bool {
    let mut current = node_id;
    let mut current_span = semantic.nodes().kind(node_id).span();
    for _ in 0..MAX_ROLE_ANCESTORS {
      let parent_id = semantic.nodes().parent_id(current);
      self.work.add_queries(1);
      match semantic.nodes().kind(parent_id) {
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::TSInstantiationExpression(_)
        | AstKind::ChainExpression(_) => {
          current = parent_id;
          current_span = semantic.nodes().kind(parent_id).span();
        }
        AstKind::StaticMemberExpression(member) => {
          if !callee_span_matches(&member.object, current_span) {
            return false;
          }
          current = parent_id;
          current_span = member.span();
        }
        AstKind::ComputedMemberExpression(member) => {
          return callee_span_matches(&member.object, current_span);
        }
        AstKind::CallExpression(call) => {
          return callee_span_matches(&call.callee, current_span);
        }
        AstKind::NewExpression(expression) => {
          return callee_span_matches(&expression.callee, current_span);
        }
        AstKind::TaggedTemplateExpression(tagged) => {
          return callee_span_matches(&tagged.tag, current_span);
        }
        _ => return false,
      }
    }
    true
  }

  fn has_simple_value_write(&self, root: SymbolId) -> bool {
    self.value_write_roots.contains(&root)
  }

  fn has_member_write(&self, root: SymbolId) -> bool {
    self.member_write_roots.contains(&root)
  }

  fn record_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    expression: &Expression<'_>,
  ) {
    let inner = expression.get_inner_expression();
    self
      .hints
      .insert(span_key(inner.span()), hint_of(inner, |ident| reference_symbol(semantic, ident)));
    self.hints.insert(
      span_key(expression.span()),
      hint_of(inner, |ident| reference_symbol(semantic, ident)),
    );
    if let Expression::CallExpression(call) = inner {
      self.record_call(semantic, kind, call);
    }
  }

  fn record_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    call: &CallExpression<'_>,
  ) {
    let has_spread = call.arguments.iter().any(Argument::is_spread);
    let api = resolve_vue_api(
      &call.callee,
      &self.vue_imports,
      |ident| reference_symbol(semantic, ident),
      kind,
    );
    let first_arg = if has_spread {
      None
    } else {
      call.arguments.first().and_then(Argument::as_expression).map(GetSpan::span)
    };
    self.calls.insert(span_key(call.span), CallInfo { api, first_arg, has_spread });
  }

  fn mark_escape_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    if let Some(identifier) = expression.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.escaped.insert(self.root_of(symbol_id));
    }
  }

  fn index_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    offset: usize,
    operator: AssignmentOperator,
    left: &AssignmentTarget<'_>,
    right: &Expression<'_>,
  ) {
    let simple = operator == AssignmentOperator::Assign;
    let fresh = simple && is_fresh_allocation(right, |ident| reference_symbol(semantic, ident));
    let owner = self.owner(node_id);
    let callable = owner.callable;
    let block = owner.block.unwrap_or(node_id);
    match left {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
      }
      AssignmentTarget::StaticMemberExpression(member) => {
        self.record_static_member_assignment(
          semantic,
          &member.object,
          member.property.name.as_str(),
          DirectMemberWrite { offset, callable, block, right, simple, fresh },
        );
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          self.unknown_member_touch.insert(self.root_of(symbol_id));
        }
      }
      AssignmentTarget::ObjectAssignmentTarget(_) | AssignmentTarget::ArrayAssignmentTarget(_) => {
        self.mark_pattern_uncertain(semantic, left);
      }
      _ => {}
    }
  }

  fn record_static_member_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
    property: &str,
    write: DirectMemberWrite<'_>,
  ) {
    let Some(object) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let root = self.root_of(symbol_id);
    if property == "value" {
      if write.simple {
        self.value_write_roots.insert(root);
        self.value_writes.entry(root).or_default().push(ValueWrite {
          offset: write.offset,
          callable: write.callable,
          block: write.block,
        });
      } else {
        self.uncertain.insert(root);
      }
    } else {
      self.member_write_roots.insert(root);
      self.member_writes.entry((root, property.to_string())).or_default().push(MemberWrite {
        offset: write.offset,
        callable: write.callable,
        block: write.block,
        rhs: write.right.span(),
        simple_assign: write.simple,
        fresh_alloc: write.fresh,
      });
    }
  }

  fn mark_pattern_uncertain(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTarget<'_>,
  ) {
    match target {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
      }
      AssignmentTarget::StaticMemberExpression(member) => {
        self.record_pattern_static_member(semantic, &member.object, member.property.name.as_str());
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        self.record_pattern_dynamic_member(semantic, &member.object);
      }
      AssignmentTarget::ObjectAssignmentTarget(object) => {
        for property in &object.properties {
          match property {
            AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
              self.mark_maybe_default_uncertain(semantic, &property.binding);
            }
            AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
              if let Some(symbol_id) = reference_symbol(semantic, &property.binding) {
                self.reassigned.insert(self.root_of(symbol_id));
              }
            }
          }
        }
        if let Some(rest) = &object.rest {
          self.mark_pattern_uncertain(semantic, &rest.target);
        }
      }
      AssignmentTarget::ArrayAssignmentTarget(array) => {
        for element in array.elements.iter().flatten() {
          self.mark_maybe_default_uncertain(semantic, element);
        }
        if let Some(rest) = &array.rest {
          self.mark_pattern_uncertain(semantic, &rest.target);
        }
      }
      AssignmentTarget::TSAsExpression(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::TSSatisfiesExpression(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::TSNonNullExpression(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::TSTypeAssertion(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::PrivateFieldExpression(_) => {}
    }
  }

  /// Patterns restore generic uncertainty on the member object. Ordinary
  /// `state.n` keeps generic uncertainty only; capability keys/dynamic targets
  /// also mark the dedicated capability set.
  fn record_pattern_static_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
    property: &str,
  ) {
    let Some(object) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let root = self.root_of(symbol_id);
    self.uncertain.insert(root);
    if is_capability_key(property) {
      self.capability_uncertain.insert(root);
    }
  }

  fn record_pattern_dynamic_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
  ) {
    let Some(object) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let root = self.root_of(symbol_id);
    self.unknown_member_touch.insert(root);
    self.uncertain.insert(root);
    self.capability_uncertain.insert(root);
  }

  fn mark_expression_target_uncertain(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
      }
      Expression::StaticMemberExpression(member) => {
        self.record_pattern_static_member(semantic, &member.object, member.property.name.as_str());
      }
      Expression::ComputedMemberExpression(member) => {
        self.record_pattern_dynamic_member(semantic, &member.object);
      }
      _ => {}
    }
  }

  fn mark_maybe_default_uncertain(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTargetMaybeDefault<'_>,
  ) {
    match target {
      AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
        self.mark_pattern_uncertain(semantic, &with_default.binding);
      }
      other => {
        if let Some(assignment_target) = other.as_assignment_target() {
          self.mark_pattern_uncertain(semantic, assignment_target);
        }
      }
    }
  }

  fn record_stmt_site(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
  ) {
    let parent_id = semantic.nodes().parent_id(node_id);
    let AstKind::ExpressionStatement(statement) = semantic.nodes().kind(parent_id) else {
      return;
    };
    let block_id = semantic.nodes().parent_id(parent_id);
    if !block_is_straight_line(semantic, block_id) {
      return;
    }
    let span = mapped(line_index, sfc_source, script_offset, statement.span);
    let owner = self.owner(node_id);
    self.stmt_site.insert(
      node_id,
      StmtSite { block: block_id, callable: owner.callable, expr_offset: span.offset },
    );
  }

  fn record_event(
    &mut self,
    _semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let Some(block) = self.owner(node_id).block else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.events_by_block.entry(block).or_default().push(offset);
  }
}

fn summarize_object_props(
  entries: &[ObjectEntry],
  work: &WorkCounter,
) -> HashMap<String, ObjectProp> {
  struct Track {
    last_data: Option<(usize, Span)>,
    accessor: bool,
  }
  let mut tracks: HashMap<String, Track> = HashMap::new();
  let mut last_uncertain = None;
  for (index, entry) in entries.iter().enumerate() {
    work.add_object_entries(1);
    match entry {
      ObjectEntry::Spread | ObjectEntry::Computed => last_uncertain = Some(index),
      ObjectEntry::Accessor { name } => {
        last_uncertain = Some(index);
        if let Some(name) = name {
          tracks
            .entry(name.clone())
            .or_insert(Track { last_data: None, accessor: false })
            .accessor = true;
        }
      }
      ObjectEntry::Data { name, value, .. } | ObjectEntry::Method { name, value, .. } => {
        tracks
          .entry(name.clone())
          .or_insert(Track { last_data: None, accessor: false })
          .last_data = Some((index, *value));
      }
    }
  }
  let mut props = HashMap::new();
  for (name, track) in tracks {
    work.add_object_entries(1);
    if track.accessor {
      props.insert(name, ObjectProp::Unknown);
      continue;
    }
    let Some((index, span)) = track.last_data else {
      props.insert(name, ObjectProp::Unknown);
      continue;
    };
    work.add_object_entries(1);
    if last_uncertain.is_some_and(|event| event > index) {
      props.insert(name, ObjectProp::Unknown);
    } else {
      props.insert(name, ObjectProp::Value(span));
    }
  }
  props
}

const CAPABILITY_KEYS: &[&str] = &[
  "__v_isRef",
  "__v_skip",
  "__v_isReadonly",
  "__v_raw",
  "__v_isReactive",
  "__v_isShallow",
  "__proto__",
  "prototype",
];

fn is_capability_key(name: &str) -> bool {
  CAPABILITY_KEYS.contains(&name)
}

const MAX_ROLE_ANCESTORS: u8 = 8;

/// Exact span identity for a callee / tag / member object, including the
/// inner expression after TS and parenthesis wrappers.
fn callee_span_matches(expression: &Expression<'_>, current_span: Span) -> bool {
  expression.span() == current_span || expression.get_inner_expression().span() == current_span
}

fn is_known_constructor_or_watch(api: Option<&str>) -> bool {
  matches!(api, Some("reactive" | "shallowReactive" | "readonly" | "shallowReadonly" | "watch"))
}

fn known_static_member_object_role(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  matches!(semantic.nodes().parent_kind(node_id), AstKind::StaticMemberExpression(_))
}

fn known_const_alias_role(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(node_id) else {
    return false;
  };
  matches!(declarator.id, oxc_ast::ast::BindingPattern::BindingIdentifier(ref binding) if {
    binding.symbol_id.get().is_some_and(|symbol_id| {
      semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable)
    })
  })
}

fn object_entries(object: &oxc_ast::ast::ObjectExpression<'_>) -> Vec<ObjectEntry> {
  let mut entries = Vec::new();
  for property_kind in &object.properties {
    match property_kind {
      ObjectPropertyKind::SpreadProperty(_) => entries.push(ObjectEntry::Spread),
      ObjectPropertyKind::ObjectProperty(prop) => {
        if prop.kind != PropertyKind::Init {
          entries.push(ObjectEntry::Accessor {
            name: prop.key.static_name().map(|name| name.to_string()),
          });
          continue;
        }
        match prop.key.static_name() {
          Some(name) if prop.method => entries.push(ObjectEntry::Method {
            name: name.to_string(),
            key: prop.key.span(),
            value: prop.value.span(),
          }),
          Some(name) => entries.push(ObjectEntry::Data {
            name: name.to_string(),
            key: prop.key.span(),
            value: prop.value.span(),
          }),
          None => entries.push(ObjectEntry::Computed),
        }
      }
    }
  }
  entries
}

fn block_is_straight_line(semantic: &oxc_semantic::Semantic<'_>, block_id: NodeId) -> bool {
  match semantic.nodes().kind(block_id) {
    AstKind::Program(_) | AstKind::FunctionBody(_) => true,
    AstKind::BlockStatement(_) => matches!(
      semantic.nodes().parent_kind(block_id),
      AstKind::Function(_)
        | AstKind::FunctionBody(_)
        | AstKind::ArrowFunctionExpression(_)
        | AstKind::Program(_)
    ),
    _ => false,
  }
}

fn reference_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

fn mapped(
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  span: Span,
) -> SourceSpan {
  source_span(line_index, sfc_source, script_offset, span)
}
