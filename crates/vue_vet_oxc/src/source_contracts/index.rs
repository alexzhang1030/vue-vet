//! One-pass indexes: Vue imports, aliases, writes, escapes, span maps.
//!
//! Const alias roots are compressed before mutation indexing so forward
//! references share the same root-keyed summaries. Collection capability is a
//! dedicated poisoned-root set; generic `uncertain` / `escaped` stay source5.
//! Escape walks that exhaust `MAX_ESCAPE_DEPTH` on a leftover
//! logical/conditional/sequence/aggregate do not trust that node: leftover
//! spans are applied during the existing semantic-reference pass, including
//! unresolved native constructor/prototype identifiers. Const aliases of those
//! constructors and `.prototype` objects share one precomputed native-kind
//! identity (`intern_native_ctor`) queried by both ordinary and leftover
//! escapes. A true `globalThis` alias escape taints supported intrinsic
//! constructors.

use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentOperator, AssignmentTarget, AssignmentTargetMaybeDefault,
    AssignmentTargetProperty, BindingPattern, CallExpression, Expression, ForStatementLeft,
    IdentifierReference, NewExpression, ObjectPropertyKind, PropertyKind, SimpleAssignmentTarget,
    StaticMemberExpression, UnaryOperator, VariableDeclarator,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};
use oxc_syntax::reference::ReferenceFlags;
use vue_vet_core::{ScriptKind, SourceSpan};

use super::shape::{
  CollectionCtor, ShapeHint, VueImport, collect_vue_imports, hint_of, intern_extractable_method,
  intern_native_ctor, is_fresh_allocation, is_known_receiver_method, resolve_vue_api, span_key,
  unresolved_collection_kind,
};
use super::stats::WorkCounter;
use crate::facts::source_span;

const MAX_ALIAS_DEPTH: u8 = 8;
const MAX_ROLE_ANCESTORS: u8 = 8;
/// Structured logical/conditional/sequence/aggregate walk. Exhaustion does
/// not trust the leftover node; raising this bound only moves the defect.
const MAX_ESCAPE_DEPTH: u8 = 8;

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

#[derive(Clone, Copy, Debug)]
pub(super) struct CallInfo {
  pub api: Option<&'static str>,
  pub api_from: Option<&'static str>,
  pub first_arg: Option<Span>,
  pub has_spread: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ExtractedMethod {
  pub collection: SymbolId,
  pub method: &'static str,
  pub extract_span: Span,
  pub extract_offset: usize,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Debug)]
pub(super) enum ObjectEntry {
  Spread,
  Computed,
  Accessor { name: Option<String> },
  Data { name: String, value: Span },
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
  pub import_from: HashMap<SymbolId, &'static str>,
  pub alias_root: HashMap<SymbolId, SymbolId>,
  pub escaped: HashSet<SymbolId>,
  pub uncertain: HashSet<SymbolId>,
  pub reassigned: HashSet<SymbolId>,
  pub unknown_member_touch: HashSet<SymbolId>,
  pub capability_poisoned: HashSet<SymbolId>,
  unresolved_escape_spans: Vec<Span>,
  unresolved_global_refs: Vec<(Span, &'static str)>,
  pub tainted_ctors: HashSet<&'static str>,
  pub extracted_methods: HashMap<SymbolId, ExtractedMethod>,
  pub value_writes: HashMap<SymbolId, Vec<ValueWrite>>,
  pub member_writes: HashMap<(SymbolId, String), Vec<MemberWrite>>,
  pub hints: HashMap<u64, ShapeHint>,
  pub calls: HashMap<u64, CallInfo>,
  pub objects: HashMap<u64, Vec<ObjectEntry>>,
  pub object_props: HashMap<u64, HashMap<String, ObjectProp>>,
  pub collections: HashMap<u64, CollectionCtor>,
  pub arrays: HashSet<u64>,
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
  global_this_aliases: HashSet<SymbolId>,
  native_ctor_aliases: HashMap<SymbolId, &'static str>,
  terminations_by_callable: HashMap<Option<NodeId>, Vec<usize>>,
  work: WorkCounter,
}

impl Indexes {
  pub(super) fn build(
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
  ) -> Self {
    let work = WorkCounter::default();
    let (vue_imports, import_from) = collect_vue_imports(semantic, &work);
    let mut indexes = Self {
      vue_imports,
      import_from,
      alias_root: HashMap::new(),
      escaped: HashSet::new(),
      uncertain: HashSet::new(),
      reassigned: HashSet::new(),
      unknown_member_touch: HashSet::new(),
      capability_poisoned: HashSet::new(),
      unresolved_escape_spans: Vec::new(),
      unresolved_global_refs: Vec::new(),
      tainted_ctors: HashSet::new(),
      extracted_methods: HashMap::new(),
      value_writes: HashMap::new(),
      member_writes: HashMap::new(),
      hints: HashMap::new(),
      calls: HashMap::new(),
      objects: HashMap::new(),
      object_props: HashMap::new(),
      collections: HashMap::new(),
      arrays: HashSet::new(),
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
      global_this_aliases: HashSet::new(),
      native_ctor_aliases: HashMap::new(),
      terminations_by_callable: HashMap::new(),
      work,
    };
    indexes.build_owners(semantic);
    indexes.precompute_aliases(semantic);
    indexes.scan(semantic, line_index, sfc_source, script_offset, kind);
    indexes.finish_aliases_and_roles(semantic);
    indexes.summarize_writes();
    for events in indexes.events_by_block.values_mut() {
      events.sort_unstable();
    }
    for offsets in indexes.terminations_by_callable.values_mut() {
      offsets.sort_unstable();
    }
    for writes in indexes.member_writes.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    for writes in indexes.value_writes.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    indexes
  }

  pub(super) fn stats(&self) -> super::stats::SourceContractStats {
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

  pub(super) fn extracted_method(&self, symbol_id: SymbolId) -> Option<ExtractedMethod> {
    self.work.add_queries(1);
    self.extracted_methods.get(&self.root_of(symbol_id)).copied()
  }

  pub(super) fn capability_poisoned(&self, symbol_id: SymbolId) -> bool {
    self.work.add_queries(1);
    self.capability_poisoned.contains(&self.root_of(symbol_id))
  }

  /// Dedicated collection-identity query. Does not read generic `uncertain` /
  /// `escaped` so source5 replacement eligibility stays unchanged.
  /// Roots referenced inside an unresolved leftover escape span are poisoned
  /// during the semantic-reference pass. Unresolved native constructor /
  /// prototype identifiers in that span, and const aliases of those identities,
  /// taint canonical constructor identity. A true `globalThis` alias escape
  /// taints every supported intrinsic.
  pub(super) fn collection_capability_invalid(&self, symbol_id: SymbolId) -> bool {
    self.capability_poisoned(symbol_id)
  }

  pub(super) fn extracted_call_eligible(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    extracted: ExtractedMethod,
    call_offset: usize,
  ) -> bool {
    let owner = self.owner(node_id);
    if owner.callable != extracted.callable {
      self.work.add_queries(1);
      return false;
    }
    if call_offset <= extracted.extract_offset {
      self.work.add_queries(1);
      return false;
    }
    if self.has_callable_termination_between(
      extracted.callable,
      extracted.extract_offset,
      call_offset,
    ) {
      return false;
    }
    self.call_reach_is_straight(semantic, node_id)
  }

  pub(super) fn ctor_tainted(&self, name: &str) -> bool {
    self.work.add_queries(1);
    intern_native_ctor(name).is_some_and(|ctor| self.tainted_ctors.contains(ctor))
  }

  pub(super) fn collection_ctor(&self, span: Span) -> Option<CollectionCtor> {
    self.work.add_queries(1);
    self.collections.get(&span_key(span)).copied()
  }

  pub(super) fn is_array_span(&self, span: Span) -> bool {
    self.work.add_queries(1);
    self.arrays.contains(&span_key(span))
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

  fn owner(&self, node_id: NodeId) -> Owner {
    self.owners.get(&node_id).copied().unwrap_or(Owner { callable: None, block: None })
  }

  fn has_callable_termination_between(
    &self,
    callable: Option<NodeId>,
    start: usize,
    end: usize,
  ) -> bool {
    let Some(offsets) = self.terminations_by_callable.get(&callable) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(offsets, |offset| *offset <= start);
    self.work.add_queries(1);
    offsets.get(index).is_some_and(|offset| *offset < end)
  }

  fn call_reach_is_straight(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut node_id: NodeId,
  ) -> bool {
    for _ in 0..MAX_ROLE_ANCESTORS {
      self.work.add_queries(1);
      let parent = semantic.nodes().parent_id(node_id);
      let current_span = semantic.nodes().kind(node_id).span();
      match semantic.nodes().kind(parent) {
        AstKind::Program(_) | AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
          return true;
        }
        AstKind::LogicalExpression(logical) => {
          if callee_span_matches(&logical.left, current_span) {
            node_id = parent;
            continue;
          }
          return false;
        }
        AstKind::ConditionalExpression(conditional) => {
          if callee_span_matches(&conditional.test, current_span) {
            node_id = parent;
            continue;
          }
          return false;
        }
        AstKind::IfStatement(statement) => {
          if callee_span_matches(&statement.test, current_span) {
            node_id = parent;
            continue;
          }
          return false;
        }
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::TSInstantiationExpression(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::VariableDeclarator(_)
        | AstKind::VariableDeclaration(_)
        | AstKind::BlockStatement(_)
        | AstKind::FunctionBody(_)
        | AstKind::StaticMemberExpression(_)
        | AstKind::ComputedMemberExpression(_)
        | AstKind::CallExpression(_)
        | AstKind::NewExpression(_)
        | AstKind::UnaryExpression(_)
        | AstKind::UpdateExpression(_)
        | AstKind::AssignmentExpression(_)
        | AstKind::SequenceExpression(_)
        | AstKind::ArrayExpression(_)
        | AstKind::ObjectExpression(_)
        | AstKind::SpreadElement(_)
        | AstKind::ReturnStatement(_)
        | AstKind::ThrowStatement(_) => {
          node_id = parent;
        }
        _ => return false,
      }
    }
    false
  }

  fn precompute_aliases(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let mut pending_prototype_aliases = Vec::new();
    for node in semantic.nodes() {
      self.work.add_nodes(1);
      let AstKind::VariableDeclarator(declarator) = node.kind() else {
        continue;
      };
      let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
        continue;
      };
      let Some(local) = binding.symbol_id.get() else {
        continue;
      };
      if !semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
        continue;
      }
      let Some(init) = &declarator.init else {
        continue;
      };
      let inner = init.get_inner_expression();
      if let Expression::StaticMemberExpression(member) = inner {
        self.note_prototype_alias(semantic, local, member, &mut pending_prototype_aliases);
        continue;
      }
      let Some(identifier) = inner.get_identifier_reference() else {
        continue;
      };
      match reference_symbol(semantic, identifier) {
        Some(target) => {
          self.work.add_queries(1);
          self.alias_root.insert(local, target);
        }
        None if identifier.name.as_str() == "globalThis" => {
          self.global_this_aliases.insert(local);
        }
        None => {
          if let Some(ctor) = intern_native_ctor(identifier.name.as_str()) {
            self.record_native_ctor_alias(local, ctor);
          }
        }
      }
    }
    self.canonicalize_aliases();
    self.resolve_prototype_aliases(pending_prototype_aliases);
  }

  fn note_prototype_alias(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    local: SymbolId,
    member: &StaticMemberExpression<'_>,
    pending: &mut Vec<(SymbolId, SymbolId)>,
  ) {
    self.work.add_queries(1);
    if member.property.name.as_str() != "prototype" {
      return;
    }
    let Some(identifier) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    match reference_symbol(semantic, identifier) {
      Some(object) => {
        self.work.add_queries(1);
        pending.push((local, object));
      }
      None => {
        if let Some(ctor) = intern_native_ctor(identifier.name.as_str()) {
          self.record_native_ctor_alias(local, ctor);
        }
      }
    }
  }

  fn resolve_prototype_aliases(&mut self, pending: Vec<(SymbolId, SymbolId)>) {
    for (local, object) in pending {
      self.work.add_queries(1);
      if let Some(ctor) = self.native_ctor_kind(self.root_of(object)) {
        self.record_native_ctor_alias(local, ctor);
      }
    }
  }

  fn record_native_ctor_alias(&mut self, local: SymbolId, ctor: &'static str) {
    self.work.add_queries(1);
    self.native_ctor_aliases.insert(local, ctor);
  }

  fn native_ctor_kind(&self, root: SymbolId) -> Option<&'static str> {
    self.work.add_queries(1);
    self.native_ctor_aliases.get(&root).copied()
  }

  fn native_ctor_of_identifier(
    &self,
    identifier: &IdentifierReference<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
  ) -> Option<&'static str> {
    self.work.add_queries(1);
    reference_symbol(semantic, identifier).map_or_else(
      || intern_native_ctor(identifier.name.as_str()),
      |symbol_id| self.native_ctor_kind(self.root_of(symbol_id)),
    )
  }

  fn canonicalize_aliases(&mut self) {
    let locals: Vec<SymbolId> = self.alias_root.keys().copied().collect();
    for local in locals {
      self.work.add_queries(1);
      let mut current = local;
      let mut depth = 0_u8;
      loop {
        self.work.add_queries(1);
        let Some(&next) = self.alias_root.get(&current) else {
          break;
        };
        if next == current {
          break;
        }
        depth = depth.saturating_add(1);
        if depth > MAX_ALIAS_DEPTH {
          self.capability_poisoned.insert(local);
          current = local;
          break;
        }
        current = next;
      }
      self.alias_root.insert(local, current);
    }
  }

  pub(super) fn note_node(&self) {
    self.work.add_nodes(1);
  }

  pub(super) fn note_query(&self) {
    self.work.add_queries(1);
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
        AstKind::VariableDeclarator(declarator) => self.scan_variable_declarator(
          semantic,
          line_index,
          sfc_source,
          script_offset,
          kind,
          node_id,
          declarator,
        ),
        AstKind::AssignmentExpression(assignment) => {
          self.record_expr(semantic, kind, &assignment.right);
          self.mark_escape_expr(semantic, &assignment.right);
          self.poison_expr(semantic, &assignment.right);
          self.taint_assignment_target(semantic, &assignment.left);
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
          self.poison_simple_target(semantic, &update.argument);
        }
        AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
          self.poison_member_expression(semantic, &unary.argument);
        }
        AstKind::ForInStatement(statement) => {
          self.note_loop_assignment(semantic, &statement.left);
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::ForOfStatement(statement) => {
          self.note_loop_assignment(semantic, &statement.left);
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::CallExpression(call) => {
          self.scan_call(semantic, line_index, sfc_source, script_offset, kind, node_id, call);
        }
        AstKind::NewExpression(expression) => self.scan_new(semantic, kind, expression),
        AstKind::TaggedTemplateExpression(tagged) => {
          self.note_receiver_use(semantic, &tagged.tag);
          for expression in &tagged.quasi.expressions {
            self.mark_escape_expr(semantic, expression);
            self.poison_expr(semantic, expression);
          }
        }
        AstKind::ThrowStatement(statement) => {
          self.record_expr(semantic, kind, &statement.argument);
          self.mark_escape_expr(semantic, &statement.argument);
          self.poison_expr(semantic, &statement.argument);
          self.record_termination(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
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
              self.poison_expr(semantic, &property.value);
            }
          }
        }
        AstKind::ArrayExpression(array) => {
          self.arrays.insert(span_key(array.span));
          for element in &array.elements {
            if let Some(expression) = element.as_expression() {
              self.record_expr(semantic, kind, expression);
              self.mark_escape_expr(semantic, expression);
              self.poison_expr(semantic, expression);
            }
          }
        }
        AstKind::ReturnStatement(statement) => {
          if let Some(argument) = &statement.argument {
            self.record_expr(semantic, kind, argument);
            self.mark_escape_expr(semantic, argument);
            self.poison_expr(semantic, argument);
          }
          self.record_termination(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_event(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
        }
        AstKind::SpreadElement(spread) => {
          self.mark_escape_expr(semantic, &spread.argument);
          self.poison_expr(semantic, &spread.argument);
        }
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
        AstKind::IdentifierReference(identifier) => {
          self.index_unresolved_global_ref(semantic, identifier);
        }
        _ => {}
      }
    }
  }

  fn finish_aliases_and_roles(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let leftover = self.merged_unresolved_escape_ranges();
    for symbol_id in semantic.scoping().symbol_ids() {
      for reference in semantic.symbol_references(symbol_id) {
        self.work.add_references(1);
        let root = self.root_of(symbol_id);
        if !leftover.is_empty() {
          let node_span = semantic.nodes().kind(reference.node_id()).span();
          if self.unresolved_escape_covers(&leftover, node_span) {
            self.capability_poisoned.insert(root);
            self.work.add_queries(1);
            if self.global_this_aliases.contains(&root) {
              self.taint_all_native_ctors();
            }
            if let Some(ctor) = self.native_ctor_kind(root) {
              self.tainted_ctors.insert(ctor);
            }
          }
        }
        let flags = reference.flags();
        let member_payload = flags.intersects(ReferenceFlags::MemberWriteTarget)
          || known_static_member_object_role(semantic, reference.node_id());
        if member_payload {
          if flags.is_write() && !self.has_simple_value_write(root) && !self.has_member_write(root)
          {
            self.uncertain.insert(root);
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
      }
    }
    if !leftover.is_empty() {
      self.apply_unresolved_global_escape_coverage(&leftover);
    }
  }

  fn has_simple_value_write(&self, root: SymbolId) -> bool {
    self.value_write_roots.contains(&root)
  }

  fn has_member_write(&self, root: SymbolId) -> bool {
    self.member_write_roots.contains(&root)
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "scan site needs owner, source map, and declarator"
  )]
  fn scan_variable_declarator(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
    node_id: NodeId,
    declarator: &VariableDeclarator<'_>,
  ) {
    if matches!(
      semantic.nodes().parent_kind(semantic.nodes().parent_id(node_id)),
      AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
    ) && let BindingPattern::BindingIdentifier(binding) = &declarator.id
      && let Some(symbol_id) = binding.symbol_id.get()
    {
      let root = self.root_of(symbol_id);
      self.escaped.insert(root);
      self.capability_poisoned.insert(root);
    }
    if let Some(init) = &declarator.init {
      self.record_expr(semantic, kind, init);
      if let Some(ident) = init.get_inner_expression().get_identifier_reference()
        && let Some(target) = reference_symbol(semantic, ident)
      {
        match &declarator.id {
          BindingPattern::BindingIdentifier(binding) => {
            if let Some(local) = binding.symbol_id.get() {
              let root = self.root_of(target);
              if semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
                self.alias_root.insert(local, root);
              } else {
                self.escaped.insert(root);
                self.capability_poisoned.insert(root);
              }
            }
          }
          BindingPattern::ObjectPattern(_) => {
            self.escaped.insert(self.root_of(target));
          }
          _ => {
            let root = self.root_of(target);
            self.escaped.insert(root);
            self.capability_poisoned.insert(root);
          }
        }
      }
    }
    if let BindingPattern::BindingIdentifier(binding) = &declarator.id
      && let Some(symbol_id) = binding.symbol_id.get()
      && let Some(init) = &declarator.init
    {
      self.init_span.insert(symbol_id, init.span());
    }
    self.record_method_extraction(
      semantic,
      line_index,
      sfc_source,
      script_offset,
      node_id,
      declarator,
    );
  }

  #[expect(clippy::too_many_arguments, reason = "scan site needs owner, source map, and call")]
  fn scan_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    self.record_call(semantic, kind, call);
    let vue_api = self.calls.get(&span_key(call.span)).and_then(|info| info.api);
    for argument in &call.arguments {
      if let Some(expression) = argument.as_expression() {
        self.record_expr(semantic, kind, expression);
        self.mark_escape_expr(semantic, expression);
        if vue_api.is_none() {
          self.poison_expr(semantic, expression);
        }
      }
    }
    self.note_receiver_use(semantic, &call.callee);
    self.record_stmt_site(semantic, line_index, sfc_source, script_offset, node_id);
    self.record_event(semantic, line_index, sfc_source, script_offset, node_id, call.span);
  }

  fn scan_new(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    expression: &NewExpression<'_>,
  ) {
    self.record_expr(semantic, kind, &expression.callee);
    self.note_receiver_use(semantic, &expression.callee);
    for argument in &expression.arguments {
      if let Some(argument) = argument.as_expression() {
        self.record_expr(semantic, kind, argument);
        self.mark_escape_expr(semantic, argument);
        self.poison_expr(semantic, argument);
      }
    }
    if let Some(ctor) =
      unresolved_collection_kind(&expression.callee, |ident| reference_symbol(semantic, ident))
    {
      self.collections.insert(span_key(expression.span), ctor);
    }
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
    if let Expression::ArrayExpression(array) = inner {
      self.arrays.insert(span_key(array.span));
      self.arrays.insert(span_key(expression.span()));
    }
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
    let api_from =
      vue_api_from(&call.callee, &self.import_from, |ident| reference_symbol(semantic, ident));
    let first_arg = if has_spread {
      None
    } else {
      call.arguments.first().and_then(Argument::as_expression).map(GetSpan::span)
    };
    self.calls.insert(span_key(call.span), CallInfo { api, api_from, first_arg, has_spread });
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

  fn poison_expr(&mut self, semantic: &oxc_semantic::Semantic<'_>, expression: &Expression<'_>) {
    self.poison_expr_bounded(semantic, expression, MAX_ESCAPE_DEPTH);
  }

  fn poison_expr_bounded(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    remaining: u8,
  ) {
    self.work.add_queries(1);
    if remaining == 0 {
      self.poison_unresolved_escape(semantic, expression);
      return;
    }
    let next = remaining.saturating_sub(1);
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.capability_poisoned.insert(self.root_of(symbol_id));
        }
        self.taint_native_ctor_identifier(identifier, semantic);
        self.taint_if_global_object(semantic, identifier);
      }
      Expression::LogicalExpression(logical) => {
        self.poison_expr_bounded(semantic, &logical.left, next);
        self.poison_expr_bounded(semantic, &logical.right, next);
      }
      Expression::ConditionalExpression(conditional) => {
        self.poison_expr_bounded(semantic, &conditional.consequent, next);
        self.poison_expr_bounded(semantic, &conditional.alternate, next);
      }
      Expression::SequenceExpression(sequence) => {
        if let Some(last) = sequence.expressions.last() {
          self.poison_expr_bounded(semantic, last, next);
        }
      }
      Expression::ArrayExpression(array) => {
        for element in &array.elements {
          if let Some(element) = element.as_expression() {
            self.poison_expr_bounded(semantic, element, next);
          }
        }
      }
      Expression::ObjectExpression(object) => {
        for property in &object.properties {
          if let ObjectPropertyKind::ObjectProperty(property) = property {
            self.poison_expr_bounded(semantic, &property.value, next);
          }
        }
      }
      inner => self.taint_native_prototype(semantic, inner),
    }
  }

  fn poison_unresolved_escape(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    let inner = expression.get_inner_expression();
    if let Some(identifier) = inner.get_identifier_reference() {
      if let Some(symbol_id) = reference_symbol(semantic, identifier) {
        self.capability_poisoned.insert(self.root_of(symbol_id));
      }
      self.taint_native_ctor_identifier(identifier, semantic);
      self.taint_if_global_object(semantic, identifier);
      return;
    }
    self.unresolved_escape_spans.push(expression.span());
  }

  fn merged_unresolved_escape_ranges(&self) -> Vec<(u32, u32)> {
    self.work.add_queries(1);
    if self.unresolved_escape_spans.is_empty() {
      return Vec::new();
    }
    let mut ranges = Vec::with_capacity(self.unresolved_escape_spans.len());
    for span in &self.unresolved_escape_spans {
      self.work.add_queries(1);
      ranges.push((span.start, span.end));
    }
    self.work.sort_unstable(&mut ranges);
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for range in ranges {
      self.work.add_queries(1);
      match merged.last_mut() {
        Some(last) if range.0 <= last.1 => last.1 = last.1.max(range.1),
        _ => merged.push(range),
      }
    }
    merged
  }

  fn unresolved_escape_covers(&self, ranges: &[(u32, u32)], span: Span) -> bool {
    let index = self.work.partition_point(ranges, |range| range.0 <= span.start);
    self.work.add_queries(1);
    index
      .checked_sub(1)
      .and_then(|index| ranges.get(index))
      .is_some_and(|range| span.end <= range.1)
  }

  fn taint_native_ctor_identifier(
    &mut self,
    identifier: &IdentifierReference<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
  ) {
    if let Some(ctor) = self.native_ctor_of_identifier(identifier, semantic) {
      self.tainted_ctors.insert(ctor);
    }
  }

  fn taint_if_global_object(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    identifier: &IdentifierReference<'_>,
  ) {
    if self.is_global_this_ref(semantic, identifier) {
      self.taint_all_native_ctors();
    }
  }

  fn taint_all_native_ctors(&mut self) {
    self.work.add_queries(1);
    for name in ["Map", "Set", "Array"] {
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      }
    }
  }

  fn index_unresolved_global_ref(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    identifier: &IdentifierReference<'_>,
  ) {
    let Some(name) = intern_unresolved_global_name(identifier.name.as_str()) else {
      return;
    };
    if reference_symbol(semantic, identifier).is_some() {
      return;
    }
    self.work.add_references(1);
    self.unresolved_global_refs.push((identifier.span(), name));
  }

  fn apply_unresolved_global_escape_coverage(&mut self, leftover: &[(u32, u32)]) {
    let refs = std::mem::take(&mut self.unresolved_global_refs);
    for (span, name) in refs {
      self.work.add_references(1);
      if !self.unresolved_escape_covers(leftover, span) {
        continue;
      }
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      } else {
        self.taint_all_native_ctors();
      }
    }
  }

  fn taint_native_prototype(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    inner: &Expression<'_>,
  ) {
    let Expression::StaticMemberExpression(member) = inner else {
      return;
    };
    if member.property.name.as_str() != "prototype" {
      return;
    }
    let Some(identifier) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    if let Some(ctor) = self.native_ctor_of_identifier(identifier, semantic) {
      self.tainted_ctors.insert(ctor);
    }
  }

  fn taint_assignment_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTarget<'_>,
  ) {
    match target {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      AssignmentTarget::StaticMemberExpression(member) => {
        self.taint_native_prototype(semantic, member.object.get_inner_expression());
        if member.property.name.as_str() == "prototype"
          && let Some(identifier) = member.object.get_inner_expression().get_identifier_reference()
        {
          self.taint_native_ctor_identifier(identifier, semantic);
        }
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        self.taint_native_prototype(semantic, member.object.get_inner_expression());
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      AssignmentTarget::ObjectAssignmentTarget(object) => {
        for property in &object.properties {
          match property {
            AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
              self.taint_maybe_default(semantic, &property.binding);
            }
            AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(_) => {}
          }
        }
        if let Some(rest) = &object.rest {
          self.taint_assignment_target(semantic, &rest.target);
        }
      }
      AssignmentTarget::ArrayAssignmentTarget(array) => {
        for element in array.elements.iter().flatten() {
          self.taint_maybe_default(semantic, element);
        }
        if let Some(rest) = &array.rest {
          self.taint_assignment_target(semantic, &rest.target);
        }
      }
      AssignmentTarget::TSAsExpression(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::TSSatisfiesExpression(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::TSNonNullExpression(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::TSTypeAssertion(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::PrivateFieldExpression(_) => {}
    }
  }

  fn taint_maybe_default(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTargetMaybeDefault<'_>,
  ) {
    match target {
      AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
        self.taint_assignment_target(semantic, &with_default.binding);
      }
      other => {
        if let Some(assignment_target) = other.as_assignment_target() {
          self.taint_assignment_target(semantic, assignment_target);
        }
      }
    }
  }

  fn taint_expression_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      Expression::StaticMemberExpression(member) => {
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      Expression::ComputedMemberExpression(member) => {
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      _ => {}
    }
  }

  fn taint_global_ctor_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
    static_key: Option<&str>,
    computed_key: Option<&Expression<'_>>,
  ) {
    self.work.add_queries(1);
    let Some(identifier) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    if !self.is_global_this_ref(semantic, identifier) {
      return;
    }
    if let Some(name) = static_key {
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      }
      return;
    }
    let Some(key) = computed_key else {
      return;
    };
    if let Some(name) = static_string_key(key) {
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      }
    } else {
      self.taint_all_native_ctors();
    }
  }

  fn is_global_this_ref(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    identifier: &IdentifierReference<'_>,
  ) -> bool {
    self.work.add_queries(1);
    reference_symbol(semantic, identifier).map_or_else(
      || identifier.name.as_str() == "globalThis",
      |symbol_id| self.global_this_aliases.contains(&self.root_of(symbol_id)),
    )
  }

  fn note_receiver_use(&mut self, semantic: &oxc_semantic::Semantic<'_>, callee: &Expression<'_>) {
    match callee.get_inner_expression() {
      Expression::StaticMemberExpression(member) => {
        if is_known_receiver_method(member.property.name.as_str()) {
          return;
        }
        self.poison_member_object(semantic, &member.object);
      }
      Expression::ComputedMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
      }
      _ => {}
    }
  }

  fn record_method_extraction(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    declarator: &VariableDeclarator<'_>,
  ) {
    let owner = self.owner(node_id);
    let callable = owner.callable;
    let block = owner.block.unwrap_or(node_id);
    match &declarator.id {
      BindingPattern::BindingIdentifier(binding) => {
        let Some(symbol_id) = binding.symbol_id.get() else {
          return;
        };
        if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
          return;
        }
        let Some(init) = &declarator.init else {
          return;
        };
        let Expression::StaticMemberExpression(member) = init.get_inner_expression() else {
          return;
        };
        let Some(method) = intern_extractable_method(member.property.name.as_str()) else {
          return;
        };
        let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(collection) = reference_symbol(semantic, object) else {
          return;
        };
        let extract_offset = mapped(line_index, sfc_source, script_offset, member.span).offset;
        self.extracted_methods.insert(
          symbol_id,
          ExtractedMethod {
            collection,
            method,
            extract_span: member.span,
            extract_offset,
            callable,
            block,
          },
        );
      }
      BindingPattern::ObjectPattern(pattern) => {
        if pattern.rest.is_some() {
          return;
        }
        let Some(init) = &declarator.init else {
          return;
        };
        let Some(object) = init.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(collection) = reference_symbol(semantic, object) else {
          return;
        };
        let mut pending = Vec::new();
        for property in &pattern.properties {
          let Some(name) = property.key.static_name() else {
            return;
          };
          let Some(method) = intern_extractable_method(name.as_ref()) else {
            return;
          };
          let BindingPattern::BindingIdentifier(binding) = &property.value else {
            return;
          };
          let Some(symbol_id) = binding.symbol_id.get() else {
            return;
          };
          if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
            return;
          }
          let extract_offset =
            mapped(line_index, sfc_source, script_offset, property.span()).offset;
          pending.push((
            symbol_id,
            ExtractedMethod {
              collection,
              method,
              extract_span: property.span(),
              extract_offset,
              callable,
              block,
            },
          ));
        }
        for (symbol_id, extracted) in pending {
          self.extracted_methods.insert(symbol_id, extracted);
        }
      }
      _ => {}
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
        let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(symbol_id) = reference_symbol(semantic, object) else {
          return;
        };
        let root = self.root_of(symbol_id);
        let property = member.property.name.as_str();
        self.capability_poisoned.insert(root);
        if property == "value" {
          if simple {
            self.value_write_roots.insert(root);
            self.value_writes.entry(root).or_default().push(ValueWrite { offset, callable, block });
          } else {
            self.uncertain.insert(root);
          }
        } else {
          self.member_write_roots.insert(root);
          self.member_writes.entry((root, property.to_string())).or_default().push(MemberWrite {
            offset,
            callable,
            block,
            rhs: right.span(),
            simple_assign: simple,
            fresh_alloc: fresh,
          });
        }
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          let root = self.root_of(symbol_id);
          self.unknown_member_touch.insert(root);
          self.capability_poisoned.insert(root);
        }
      }
      AssignmentTarget::ObjectAssignmentTarget(_) | AssignmentTarget::ArrayAssignmentTarget(_) => {
        self.mark_pattern_uncertain(semantic, left);
      }
      _ => {}
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
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          let root = self.root_of(symbol_id);
          self.uncertain.insert(root);
          self.capability_poisoned.insert(root);
        }
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          let root = self.root_of(symbol_id);
          self.unknown_member_touch.insert(root);
          self.uncertain.insert(root);
          self.capability_poisoned.insert(root);
        }
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
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          let root = self.root_of(symbol_id);
          self.uncertain.insert(root);
          self.capability_poisoned.insert(root);
        }
      }
      Expression::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          let root = self.root_of(symbol_id);
          self.unknown_member_touch.insert(root);
          self.uncertain.insert(root);
          self.capability_poisoned.insert(root);
        }
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

  fn note_loop_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    left: &ForStatementLeft<'_>,
  ) {
    self.work.add_queries(1);
    let Some(target) = left.as_assignment_target() else {
      return;
    };
    self.work.add_writes(1);
    self.taint_assignment_target(semantic, target);
    self.mark_pattern_uncertain(semantic, target);
  }

  fn poison_simple_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &SimpleAssignmentTarget<'_>,
  ) {
    match target {
      SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      SimpleAssignmentTarget::StaticMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      SimpleAssignmentTarget::ComputedMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      SimpleAssignmentTarget::TSAsExpression(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::TSNonNullExpression(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::TSTypeAssertion(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::PrivateFieldExpression(_) => {}
    }
  }

  fn poison_member_expression(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      Expression::StaticMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      Expression::ComputedMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          self.unknown_member_touch.insert(self.root_of(symbol_id));
        }
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      _ => {}
    }
  }

  fn poison_member_object(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
  ) {
    if let Some(identifier) = object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.capability_poisoned.insert(self.root_of(symbol_id));
    }
  }

  fn record_termination(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let owner = self.owner(node_id);
    let Some(block) = owner.block else {
      return;
    };
    if !block_is_straight_line(semantic, block) {
      return;
    }
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.terminations_by_callable.entry(owner.callable).or_default().push(offset);
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
      ObjectEntry::Data { name, value } => {
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

fn callee_span_matches(expression: &Expression<'_>, current_span: Span) -> bool {
  expression.span() == current_span || expression.get_inner_expression().span() == current_span
}

fn static_string_key<'a>(expression: &'a Expression<'a>) -> Option<&'a str> {
  match expression.get_inner_expression() {
    Expression::StringLiteral(literal) => Some(literal.value.as_str()),
    Expression::TemplateLiteral(literal)
      if literal.expressions.is_empty() && literal.quasis.len() == 1 =>
    {
      let quasi = literal.quasis.first()?;
      quasi.value.cooked.as_deref().or(Some(quasi.value.raw.as_str()))
    }
    _ => None,
  }
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
        if prop.kind != PropertyKind::Init || prop.method {
          entries.push(ObjectEntry::Accessor {
            name: prop.key.static_name().map(|name| name.to_string()),
          });
          continue;
        }
        match prop.key.static_name() {
          Some(name) => {
            entries.push(ObjectEntry::Data { name: name.to_string(), value: prop.value.span() });
          }
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

fn intern_unresolved_global_name(name: &str) -> Option<&'static str> {
  intern_native_ctor(name).or_else(|| (name == "globalThis").then_some("globalThis"))
}

fn reference_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

fn vue_api_from(
  callee: &Expression<'_>,
  import_from: &HashMap<SymbolId, &'static str>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> Option<&'static str> {
  let callee = callee.get_inner_expression();
  if let Some(identifier) = callee.get_identifier_reference() {
    return symbol_of(identifier).and_then(|symbol_id| import_from.get(&symbol_id).copied());
  }
  let Expression::StaticMemberExpression(member) = callee else {
    return None;
  };
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  symbol_of(object).and_then(|symbol_id| import_from.get(&symbol_id).copied())
}

fn mapped(
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  span: Span,
) -> SourceSpan {
  source_span(line_index, sfc_source, script_offset, span)
}
