//! Template-ref allocation join and demand facts.
//!
//! Indexes template relations and script symbols once, caches joined
//! allocation state by owner/ref, and emits proven pre-flush / memo-blocked
//! demand facts. Oxc AST stays inside this adapter.

mod stats;

use std::collections::{BTreeMap, HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentTarget, BinaryOperator, CallExpression, Expression, IdentifierReference,
    ImportDeclarationSpecifier, ImportOrExportKind, LogicalOperator, ModuleExportName, Statement,
    UnaryOperator,
  },
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  MemoBlockedRefDemandFact, PreFlushTemplateRefDemandFact, ScriptKind, SourceSpan,
  TemplateAllocationFact, TemplateFacts, TemplateRefDemandFacts,
};

use crate::facts::source_span;

pub use stats::TemplateDemandStats;
use stats::WorkCounter;

const ANCESTOR_BUDGET: u8 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitKind {
  Null,
  False,
  True,
  Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FlushKind {
  Pre,
  Sync,
  Post,
  Unknown,
}

#[derive(Clone, Copy)]
struct Owner {
  callable: Option<NodeId>,
  block: Option<NodeId>,
}

#[derive(Clone, Copy)]
struct WatchSite {
  span: Span,
  source: SymbolId,
  callback: NodeId,
  flush: FlushKind,
  handle: Option<SymbolId>,
}

#[derive(Clone, Copy)]
struct DemandSite {
  span: Span,
  root: SymbolId,
  callable: Option<NodeId>,
  offset: usize,
  guarded: bool,
}

#[derive(Clone, Copy)]
struct WriteSite {
  root: SymbolId,
  callable: Option<NodeId>,
  offset: usize,
  span: Span,
  true_literal: bool,
}

#[derive(Clone, Copy)]
struct TickSite {
  callable: Option<NodeId>,
  offset: usize,
  span: Span,
}

struct Indexes {
  vue: HashMap<SymbolId, &'static str>,
  alias: HashMap<SymbolId, SymbolId>,
  escaped: HashSet<SymbolId>,
  reassigned: HashSet<SymbolId>,
  inits: HashMap<SymbolId, InitKind>,
  owners: HashMap<NodeId, Owner>,
  watches: Vec<WatchSite>,
  mounted: HashSet<NodeId>,
  demands: Vec<DemandSite>,
  writes: Vec<WriteSite>,
  ticks: Vec<TickSite>,
  awaits: Vec<TickSite>,
  handle_used: HashSet<SymbolId>,
  computed: HashSet<SymbolId>,
  functions: HashMap<u32, NodeId>,
  demands_by_key: HashMap<(NodeId, SymbolId), Vec<DemandSite>>,
  writes_by_root: HashMap<SymbolId, Vec<WriteSite>>,
  ticks_by_callable: HashMap<NodeId, Vec<TickSite>>,
  awaits_by_callable: HashMap<NodeId, Vec<TickSite>>,
  watches_by_source: HashMap<SymbolId, Vec<WatchSite>>,
  work: WorkCounter,
}

#[must_use]
pub fn collect_template_ref_demand_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  kind: ScriptKind,
  template: Option<&TemplateFacts>,
) -> TemplateRefDemandFacts {
  collect_with_stats(semantic, line_index, sfc_source, script_offset, kind, template, false).0
}

#[must_use]
pub fn collect_with_stats(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  kind: ScriptKind,
  template: Option<&TemplateFacts>,
  force_full: bool,
) -> (TemplateRefDemandFacts, TemplateDemandStats) {
  let work = WorkCounter::default();
  let vue = collect_vue_imports(semantic, &work);
  let has_sink = vue.values().any(|api| matches!(*api, "watch" | "ref" | "onMounted" | "nextTick"));
  let empty: &[vue_vet_core::TemplateAllocationFact] = &[];
  let allocations = template.map_or(empty, |facts| facts.allocations.as_slice());
  if !force_full && (!has_sink || allocations.is_empty() || kind != ScriptKind::Setup) {
    return (TemplateRefDemandFacts::default(), work.snapshot());
  }
  let mut indexes = Indexes {
    vue,
    alias: HashMap::new(),
    escaped: HashSet::new(),
    reassigned: HashSet::new(),
    inits: HashMap::new(),
    owners: HashMap::new(),
    watches: Vec::new(),
    mounted: HashSet::new(),
    demands: Vec::new(),
    writes: Vec::new(),
    ticks: Vec::new(),
    awaits: Vec::new(),
    handle_used: HashSet::new(),
    computed: HashSet::new(),
    functions: HashMap::new(),
    demands_by_key: HashMap::new(),
    writes_by_root: HashMap::new(),
    ticks_by_callable: HashMap::new(),
    awaits_by_callable: HashMap::new(),
    watches_by_source: HashMap::new(),
    work,
  };
  indexes.build(semantic);
  let facts = emit_facts(semantic, &indexes, allocations, line_index, sfc_source, script_offset);
  let stats = indexes.work.snapshot();
  (facts, stats)
}

#[cfg(test)]
#[must_use]
pub fn collect_forced_full(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  kind: ScriptKind,
  template: Option<&TemplateFacts>,
) -> (TemplateRefDemandFacts, TemplateDemandStats) {
  collect_with_stats(semantic, line_index, sfc_source, script_offset, kind, template, true)
}

fn collect_vue_imports(
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> HashMap<SymbolId, &'static str> {
  let mut imports = HashMap::new();
  for node in semantic.nodes() {
    work.add_nodes(1);
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    if declaration.import_kind == ImportOrExportKind::Type {
      continue;
    }
    if intern_source(declaration.source.value.as_str()).is_none() {
      continue;
    }
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    for specifier in specifiers {
      work.add_references(1);
      let Some((imported, local)) = named_specifier(specifier) else {
        continue;
      };
      let Some(api) = intern_api(imported) else {
        continue;
      };
      let Some(symbol) = local.symbol_id.get() else {
        continue;
      };
      imports.insert(symbol, api);
    }
  }
  imports
}

fn named_specifier<'a>(
  specifier: &'a ImportDeclarationSpecifier<'a>,
) -> Option<(&'a str, &'a oxc_ast::ast::BindingIdentifier<'a>)> {
  match specifier {
    ImportDeclarationSpecifier::ImportSpecifier(specifier)
      if specifier.import_kind != ImportOrExportKind::Type =>
    {
      let imported = match &specifier.imported {
        ModuleExportName::IdentifierName(name) => name.name.as_str(),
        ModuleExportName::IdentifierReference(name) => name.name.as_str(),
        ModuleExportName::StringLiteral(name) => name.value.as_str(),
      };
      Some((imported, &specifier.local))
    }
    _ => None,
  }
}

fn intern_source(source: &str) -> Option<&'static str> {
  match source {
    "vue" => Some("vue"),
    "vue-demi" => Some("vue-demi"),
    "@vue/runtime-core" => Some("@vue/runtime-core"),
    "@vue/runtime-dom" => Some("@vue/runtime-dom"),
    "#imports" => Some("#imports"),
    _ => None,
  }
}

fn intern_api(name: &str) -> Option<&'static str> {
  match name {
    "watch" => Some("watch"),
    "ref" => Some("ref"),
    "shallowRef" => Some("shallowRef"),
    "onMounted" => Some("onMounted"),
    "nextTick" => Some("nextTick"),
    "computed" => Some("computed"),
    _ => None,
  }
}

impl Indexes {
  fn build(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    self.build_owners(semantic);
    self.scan(semantic);
    self.finish_handles(semantic);
    self.index_linear_sites();
  }

  fn root_of(&self, symbol: SymbolId) -> SymbolId {
    self.alias.get(&symbol).copied().unwrap_or(symbol)
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
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
          callable = Some(node_id);
          self.functions.insert(node.kind().span().start, node_id);
        }
        AstKind::Program(_) | AstKind::FunctionBody(_) | AstKind::BlockStatement(_) => {
          block = Some(node_id);
        }
        _ => {}
      }
      self.owners.insert(node_id, Owner { callable, block });
    }
  }

  fn scan(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    for (node_id, node) in semantic.nodes().iter_enumerated() {
      self.work.add_nodes(1);
      match node.kind() {
        AstKind::VariableDeclarator(declarator) => {
          self.scan_declarator(semantic, declarator);
        }
        AstKind::CallExpression(call) => self.scan_call(semantic, node_id, call),
        AstKind::AssignmentExpression(assignment) => {
          self.scan_assignment(semantic, node_id, assignment);
        }
        AstKind::UpdateExpression(update) => self.scan_update(semantic, node_id, update),
        AstKind::StaticMemberExpression(member) => {
          self.scan_member_demand(semantic, node_id, member);
        }
        AstKind::AwaitExpression(await_expr) => self.scan_await(semantic, node_id, await_expr),
        _ => {}
      }
    }
  }

  fn scan_declarator(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    declarator: &oxc_ast::ast::VariableDeclarator<'_>,
  ) {
    let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id else {
      if let Some(init) = &declarator.init {
        self.mark_escape_expr(semantic, init);
      }
      return;
    };
    let Some(local) = binding.symbol_id.get() else {
      return;
    };
    let Some(init) = &declarator.init else {
      return;
    };
    if let Expression::Identifier(identifier) = init.get_inner_expression()
      && let Some(target) = reference_symbol(semantic, identifier)
    {
      let flags = semantic.scoping().symbol_flags(local);
      if flags.contains(oxc_semantic::SymbolFlags::ConstVariable) {
        let root = self.root_of(target);
        self.alias.insert(local, root);
        self.work.add_links(1);
      } else {
        self.escaped.insert(self.root_of(target));
      }
      return;
    }
    if let Some((api, arg)) = self.call_api_arg(semantic, init) {
      match api {
        "ref" | "shallowRef" => {
          self.inits.insert(local, classify_init(arg));
        }
        "computed" => {
          self.computed.insert(local);
        }
        _ => {}
      }
    }
  }

  fn scan_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    if call.optional {
      return;
    }
    let Some(api) = self.callee_api(semantic, &call.callee) else {
      self.note_handle_call(semantic, call);
      return;
    };
    match api {
      "watch" => self.record_watch(semantic, node_id, call),
      "onMounted" => self.record_mounted(call),
      _ => {}
    }
  }

  fn record_watch(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    if has_spread(call) {
      return;
    }
    let Some(source_expr) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Expression::Identifier(identifier) = source_expr.get_inner_expression() else {
      return;
    };
    let Some(source) = reference_symbol(semantic, identifier) else {
      return;
    };
    let Some(callback_expr) = call.arguments.get(1).and_then(Argument::as_expression) else {
      return;
    };
    let Some(callback) = self.function_at(callback_expr.get_inner_expression().span()) else {
      return;
    };
    let flush = watch_flush(call);
    let handle = result_symbol(semantic, node_id);
    self.watches.push(WatchSite {
      span: call.span,
      source: self.root_of(source),
      callback,
      flush,
      handle,
    });
  }

  fn record_mounted(&mut self, call: &CallExpression<'_>) {
    if has_spread(call) {
      return;
    }
    let Some(callback_expr) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Some(callback) = self.function_at(callback_expr.get_inner_expression().span()) else {
      return;
    };
    self.mounted.insert(callback);
  }

  fn scan_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    assignment: &oxc_ast::ast::AssignmentExpression<'_>,
  ) {
    match &assignment.left {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        if let Some(symbol) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol));
        }
      }
      AssignmentTarget::StaticMemberExpression(member) => {
        self.record_value_write(semantic, node_id, member, &assignment.right, assignment.span);
      }
      _ => self.mark_assignment_escape(semantic, &assignment.left),
    }
  }

  fn function_at(&self, span: Span) -> Option<NodeId> {
    self.functions.get(&span.start).copied()
  }

  fn scan_update(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    update: &oxc_ast::ast::UpdateExpression<'_>,
  ) {
    let oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(member) = &update.argument
    else {
      return;
    };
    if member.property.name.as_str() != "value" {
      return;
    }
    let Expression::Identifier(identifier) = member.object.get_inner_expression() else {
      return;
    };
    let Some(symbol) = reference_symbol(semantic, identifier) else {
      return;
    };
    let owner = self.owners.get(&node_id).copied().unwrap_or(Owner { callable: None, block: None });
    self.writes.push(WriteSite {
      root: self.root_of(symbol),
      callable: owner.callable,
      offset: usize::try_from(update.span.start).unwrap_or(0),
      span: update.span,
      true_literal: false,
    });
  }

  fn record_value_write(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    member: &oxc_ast::ast::StaticMemberExpression<'_>,
    right: &Expression<'_>,
    span: Span,
  ) {
    if member.property.name.as_str() != "value" {
      return;
    }
    let Expression::Identifier(identifier) = member.object.get_inner_expression() else {
      return;
    };
    let Some(symbol) = reference_symbol(semantic, identifier) else {
      return;
    };
    let owner = self.owners.get(&node_id).copied().unwrap_or(Owner { callable: None, block: None });
    let true_literal =
      matches!(right.get_inner_expression(), Expression::BooleanLiteral(literal) if literal.value);
    self.writes.push(WriteSite {
      root: self.root_of(symbol),
      callable: owner.callable,
      offset: usize::try_from(span.start).unwrap_or(0),
      span,
      true_literal,
    });
  }

  fn scan_member_demand(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    member: &oxc_ast::ast::StaticMemberExpression<'_>,
  ) {
    if member.property.name.as_str() == "value" {
      return;
    }
    let Expression::StaticMemberExpression(inner) = member.object.get_inner_expression() else {
      return;
    };
    if inner.property.name.as_str() != "value" {
      return;
    }
    let Expression::Identifier(identifier) = inner.object.get_inner_expression() else {
      return;
    };
    let Some(symbol) = reference_symbol(semantic, identifier) else {
      return;
    };
    if is_assignment_lhs(semantic, node_id, member.span) {
      return;
    }
    let owner = self.owners.get(&node_id).copied().unwrap_or(Owner { callable: None, block: None });
    let root = self.root_of(symbol);
    let guarded = self.demand_guarded(semantic, node_id, root);
    self.demands.push(DemandSite {
      span: member.span,
      root,
      callable: owner.callable,
      offset: usize::try_from(member.span.start).unwrap_or(0),
      guarded,
    });
  }

  fn demand_guarded(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    root: SymbolId,
  ) -> bool {
    self.demand_ancestor_guarded(semantic, node_id)
      || self.demand_sibling_guarded(semantic, node_id, root)
  }

  fn demand_ancestor_guarded(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut node_id: NodeId,
  ) -> bool {
    for _ in 0..ANCESTOR_BUDGET {
      self.work.add_queries(1);
      let parent = semantic.nodes().parent_id(node_id);
      match semantic.nodes().kind(parent) {
        AstKind::Program(_) | AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
          return false;
        }
        AstKind::ChainExpression(_)
        | AstKind::IfStatement(_)
        | AstKind::ConditionalExpression(_)
        | AstKind::LogicalExpression(_)
        | AstKind::TryStatement(_)
        | AstKind::ForStatement(_)
        | AstKind::WhileStatement(_)
        | AstKind::SwitchStatement(_) => return true,
        AstKind::BinaryExpression(binary)
          if matches!(
            binary.operator,
            BinaryOperator::Inequality
              | BinaryOperator::Equality
              | BinaryOperator::StrictInequality
              | BinaryOperator::StrictEquality
          ) =>
        {
          return true;
        }
        AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
          return true;
        }
        _ => node_id = parent,
      }
    }
    true
  }

  fn demand_sibling_guarded(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut node_id: NodeId,
    root: SymbolId,
  ) -> bool {
    let demand_offset = usize::try_from(semantic.nodes().kind(node_id).span().start).unwrap_or(0);
    for _ in 0..ANCESTOR_BUDGET {
      self.work.add_queries(1);
      let parent = semantic.nodes().parent_id(node_id);
      match semantic.nodes().kind(parent) {
        AstKind::Program(_) | AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
          return false;
        }
        AstKind::FunctionBody(body) => {
          return preceding_ref_guard(&body.statements, demand_offset, semantic, self, root);
        }
        AstKind::BlockStatement(block) => {
          if preceding_ref_guard(&block.body, demand_offset, semantic, self, root) {
            return true;
          }
          node_id = parent;
        }
        _ => node_id = parent,
      }
    }
    false
  }

  fn scan_await(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    await_expr: &oxc_ast::ast::AwaitExpression<'_>,
  ) {
    let owner = self.owners.get(&node_id).copied().unwrap_or(Owner { callable: None, block: None });
    let site = TickSite {
      callable: owner.callable,
      offset: usize::try_from(await_expr.span.start).unwrap_or(0),
      span: await_expr.span,
    };
    self.awaits.push(site);
    let Expression::CallExpression(call) = await_expr.argument.get_inner_expression() else {
      return;
    };
    if call.optional || !call.arguments.is_empty() {
      return;
    }
    if self.callee_api(semantic, &call.callee) != Some("nextTick") {
      return;
    }
    self.ticks.push(site);
  }

  fn index_linear_sites(&mut self) {
    for demand in &self.demands {
      if let Some(callable) = demand.callable {
        self.demands_by_key.entry((callable, demand.root)).or_default().push(*demand);
      }
    }
    for writes in self.demands_by_key.values_mut() {
      writes.sort_by_key(|demand| demand.offset);
    }
    for write in &self.writes {
      self.writes_by_root.entry(write.root).or_default().push(*write);
    }
    for writes in self.writes_by_root.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    for tick in &self.ticks {
      if let Some(callable) = tick.callable {
        self.ticks_by_callable.entry(callable).or_default().push(*tick);
      }
    }
    for ticks in self.ticks_by_callable.values_mut() {
      ticks.sort_by_key(|tick| tick.offset);
    }
    for await_site in &self.awaits {
      if let Some(callable) = await_site.callable {
        self.awaits_by_callable.entry(callable).or_default().push(*await_site);
      }
    }
    for awaits in self.awaits_by_callable.values_mut() {
      awaits.sort_by_key(|await_site| await_site.offset);
    }
    for watch in &self.watches {
      self.watches_by_source.entry(watch.source).or_default().push(*watch);
    }
    for watches in self.watches_by_source.values_mut() {
      watches.sort_by_key(|watch| watch.span.start);
    }
  }

  fn finish_handles(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let handles: Vec<SymbolId> = self.watches.iter().filter_map(|watch| watch.handle).collect();
    for handle in handles {
      self.work.add_references(1);
      for reference in semantic.symbol_references(handle) {
        self.work.add_references(1);
        let parent = semantic.nodes().parent_kind(reference.node_id());
        if matches!(parent, AstKind::CallExpression(_) | AstKind::StaticMemberExpression(_)) {
          self.handle_used.insert(handle);
        }
      }
    }
  }

  fn note_handle_call(&mut self, semantic: &oxc_semantic::Semantic<'_>, call: &CallExpression<'_>) {
    let Expression::Identifier(identifier) = call.callee.get_inner_expression() else {
      if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
        && let Expression::Identifier(identifier) = member.object.get_inner_expression()
        && let Some(symbol) = reference_symbol(semantic, identifier)
      {
        self.handle_used.insert(self.root_of(symbol));
      }
      return;
    };
    if let Some(symbol) = reference_symbol(semantic, identifier) {
      self.handle_used.insert(self.root_of(symbol));
    }
  }

  fn callee_api(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    callee: &Expression<'_>,
  ) -> Option<&'static str> {
    match callee.get_inner_expression() {
      Expression::Identifier(identifier) => {
        let symbol = reference_symbol(semantic, identifier)?;
        self.vue.get(&self.root_of(symbol)).copied()
      }
      _ => None,
    }
  }

  fn call_api_arg<'a>(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    expr: &'a Expression<'a>,
  ) -> Option<(&'static str, &'a Expression<'a>)> {
    let Expression::CallExpression(call) = expr.get_inner_expression() else {
      return None;
    };
    if call.optional || has_spread(call) {
      return None;
    }
    let api = self.callee_api(semantic, &call.callee)?;
    let arg = call.arguments.first().and_then(Argument::as_expression)?;
    Some((api, arg))
  }

  fn mark_escape_expr(&mut self, semantic: &oxc_semantic::Semantic<'_>, expr: &Expression<'_>) {
    if let Expression::Identifier(identifier) = expr.get_inner_expression()
      && let Some(symbol) = reference_symbol(semantic, identifier)
    {
      self.escaped.insert(self.root_of(symbol));
    }
  }

  fn mark_assignment_escape(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTarget<'_>,
  ) {
    if let Some(ident) = target_identifier(target)
      && let Some(symbol) = reference_symbol(semantic, ident)
    {
      self.escaped.insert(self.root_of(symbol));
    }
  }
}

fn classify_init(expr: &Expression<'_>) -> InitKind {
  match expr.get_inner_expression() {
    Expression::NullLiteral(_) => InitKind::Null,
    Expression::BooleanLiteral(literal) if !literal.value => InitKind::False,
    Expression::BooleanLiteral(literal) if literal.value => InitKind::True,
    _ => InitKind::Other,
  }
}

fn has_spread(call: &CallExpression<'_>) -> bool {
  call.arguments.iter().any(|argument| matches!(argument, Argument::SpreadElement(_)))
}

fn watch_flush(call: &CallExpression<'_>) -> FlushKind {
  let Some(options) = call.arguments.get(2).and_then(Argument::as_expression) else {
    return FlushKind::Pre;
  };
  let Expression::ObjectExpression(object) = options.get_inner_expression() else {
    return FlushKind::Unknown;
  };
  let mut flush = FlushKind::Pre;
  for property in &object.properties {
    match property {
      oxc_ast::ast::ObjectPropertyKind::SpreadProperty(_) => return FlushKind::Unknown,
      oxc_ast::ast::ObjectPropertyKind::ObjectProperty(property) => {
        if property.computed || property.kind != oxc_ast::ast::PropertyKind::Init {
          return FlushKind::Unknown;
        }
        let Some(name) = property.key.static_name() else {
          return FlushKind::Unknown;
        };
        if name != "flush" {
          continue;
        }
        flush = match property.value.get_inner_expression() {
          Expression::StringLiteral(literal) => match literal.value.as_str() {
            "pre" => FlushKind::Pre,
            "sync" => FlushKind::Sync,
            "post" => FlushKind::Post,
            _ => FlushKind::Unknown,
          },
          _ => FlushKind::Unknown,
        };
      }
    }
  }
  flush
}

fn is_assignment_lhs(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId, span: Span) -> bool {
  match semantic.nodes().parent_kind(node_id) {
    AstKind::AssignmentExpression(assignment) => assignment.left.span() == span,
    _ => false,
  }
}

fn result_symbol(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> Option<SymbolId> {
  let parent = semantic.nodes().parent_id(node_id);
  match semantic.nodes().kind(parent) {
    AstKind::VariableDeclarator(declarator) => match &declarator.id {
      oxc_ast::ast::BindingPattern::BindingIdentifier(binding) => binding.symbol_id.get(),
      _ => None,
    },
    AstKind::ParenthesizedExpression(_)
    | AstKind::TSAsExpression(_)
    | AstKind::TSSatisfiesExpression(_)
    | AstKind::TSNonNullExpression(_)
    | AstKind::TSTypeAssertion(_) => result_symbol(semantic, parent),
    _ => None,
  }
}

fn reference_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

fn target_identifier<'a>(target: &'a AssignmentTarget<'a>) -> Option<&'a IdentifierReference<'a>> {
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(identifier) => Some(identifier),
    _ => None,
  }
}

struct JoinedAlloc<'a> {
  allocation: &'a TemplateAllocationFact,
  ref_root: SymbolId,
  condition_root: SymbolId,
}

fn emit_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  indexes: &Indexes,
  allocations: &[TemplateAllocationFact],
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> TemplateRefDemandFacts {
  let mut by_ref: BTreeMap<&str, Vec<&TemplateAllocationFact>> = BTreeMap::new();
  for allocation in allocations {
    indexes.work.add_template_nodes(1);
    let Some(name) = allocation.static_ref.as_deref() else {
      continue;
    };
    by_ref.entry(name).or_default().push(allocation);
    indexes.work.add_links(1);
  }

  let mut joined: Vec<JoinedAlloc<'_>> = Vec::new();
  for allocation in allocations {
    let Some(ref_name) = allocation.static_ref.as_deref() else {
      continue;
    };
    indexes.work.add_queries(1);
    let Some(ref_symbol) = root_binding(semantic, ref_name) else {
      continue;
    };
    let ref_root = indexes.root_of(ref_symbol);
    if indexes.inits.get(&ref_root).copied() != Some(InitKind::Null) {
      continue;
    }
    if indexes.escaped.contains(&ref_root) || indexes.reassigned.contains(&ref_root) {
      continue;
    }
    if any_write(indexes, ref_root) {
      continue;
    }
    let Some(condition) = allocation.condition.as_ref() else {
      continue;
    };
    indexes.work.add_expression_ast(1);
    let Some(cond_name) = condition.simple_identifier.as_deref() else {
      continue;
    };
    indexes.work.add_queries(1);
    let Some(cond_symbol) = root_binding(semantic, cond_name) else {
      continue;
    };
    let condition_root = indexes.root_of(cond_symbol);
    if indexes.inits.get(&condition_root).copied() != Some(InitKind::False) {
      continue;
    }
    if indexes.escaped.contains(&condition_root) || indexes.reassigned.contains(&condition_root) {
      continue;
    }
    if ref_has_other_owner(&by_ref, ref_name, cond_name) {
      continue;
    }
    if allocation.is_component
      || allocation.v_show
      || allocation.v_for
      || allocation.slot
      || allocation.transition
      || allocation.callback_ref
      || !is_native_tag(&allocation.tag)
    {
      continue;
    }
    joined.push(JoinedAlloc { allocation, ref_root, condition_root });
    indexes.work.add_links(1);
  }

  let mut facts = TemplateRefDemandFacts::default();
  let mut pre_flush_refs: HashSet<SymbolId> = HashSet::new();

  for joined in &joined {
    let Some(watches) = indexes.watches_by_source.get(&joined.condition_root) else {
      indexes.work.add_comparisons(1);
      continue;
    };
    for watch in watches {
      indexes.work.add_comparisons(1);
      if !matches!(watch.flush, FlushKind::Pre | FlushKind::Sync) {
        continue;
      }
      if watch.handle.is_some_and(|handle| indexes.handle_used.contains(&handle)) {
        continue;
      }
      let Some(demand) = first_unguarded_demand(indexes, watch.callback, joined.ref_root) else {
        continue;
      };
      if has_await_before(indexes, watch.callback, demand.offset) {
        continue;
      }
      let Some(write) = mounted_true_write(indexes, joined.condition_root) else {
        continue;
      };
      let flush = match watch.flush {
        FlushKind::Sync => "sync",
        _ => "pre",
      };
      facts.pre_flush.push(PreFlushTemplateRefDemandFact {
        demand_span: mapped(line_index, sfc_source, script_offset, demand.span),
        condition_write_span: mapped(line_index, sfc_source, script_offset, write.span),
        watch_span: mapped(line_index, sfc_source, script_offset, watch.span),
        allocation_span: joined.allocation.element_span,
        flush: flush.into(),
      });
      pre_flush_refs.insert(joined.ref_root);
    }
  }

  for joined in &joined {
    if pre_flush_refs.contains(&joined.ref_root) {
      continue;
    }
    if !joined.allocation.condition_inside_memo {
      continue;
    }
    let Some(memo) = joined.allocation.memo.as_ref() else {
      continue;
    };
    indexes.work.add_expression_ast(1);
    if !memo.stable_tuple && !memo.empty {
      continue;
    }
    if joined.allocation.nested_memo {
      continue;
    }
    if memo_includes_condition(semantic, memo, joined.condition_root, &indexes.work) {
      continue;
    }
    if memo_deps_unknown(semantic, indexes, memo, joined.condition_root) {
      continue;
    }
    let Some(write) = mounted_true_write(indexes, joined.condition_root) else {
      continue;
    };
    let Some((demand, tick)) = after_tick_demand(indexes, write, joined.ref_root) else {
      continue;
    };
    let ref_span = joined.allocation.ref_span.unwrap_or(joined.allocation.element_span);
    facts.memo_blocked.push(MemoBlockedRefDemandFact {
      demand_span: mapped(line_index, sfc_source, script_offset, demand.span),
      memo_span: memo.span,
      condition_span: joined
        .allocation
        .condition
        .as_ref()
        .map_or(joined.allocation.element_span, |condition| condition.span),
      ref_span,
      source_change_span: mapped(line_index, sfc_source, script_offset, write.span),
      render_boundary_span: mapped(line_index, sfc_source, script_offset, tick.span),
    });
  }

  indexes.work.add_sorts(facts.pre_flush.len().saturating_add(facts.memo_blocked.len()) as u64);
  facts.sort_by_source_order();
  facts
}

fn is_native_tag(tag: &str) -> bool {
  tag.chars().next().is_some_and(|ch| ch.is_ascii_lowercase()) && !tag.contains('-')
}

fn root_binding(semantic: &oxc_semantic::Semantic<'_>, name: &str) -> Option<SymbolId> {
  let scoping = semantic.scoping();
  scoping.get_binding(scoping.root_scope_id(), name.into())
}

fn ref_has_other_owner(
  by_ref: &BTreeMap<&str, Vec<&TemplateAllocationFact>>,
  ref_name: &str,
  condition_name: &str,
) -> bool {
  let Some(owners) = by_ref.get(ref_name) else {
    return true;
  };
  !owners.iter().all(|allocation| {
    allocation.condition.as_ref().and_then(|condition| condition.simple_identifier.as_deref())
      == Some(condition_name)
  })
}

fn first_unguarded_demand(
  indexes: &Indexes,
  callable: NodeId,
  root: SymbolId,
) -> Option<DemandSite> {
  indexes.work.add_queries(1);
  let demands = indexes.demands_by_key.get(&(callable, root))?;
  let mut found = None;
  for demand in demands {
    indexes.work.add_queries(1);
    if !demand.guarded {
      found = Some(*demand);
      break;
    }
  }
  found
}

fn has_await_before(indexes: &Indexes, callable: NodeId, offset: usize) -> bool {
  indexes.work.add_queries(1);
  let Some(awaits) = indexes.awaits_by_callable.get(&callable) else {
    return false;
  };
  for await_site in awaits {
    indexes.work.add_queries(1);
    if await_site.offset < offset {
      return true;
    }
  }
  false
}

fn any_write(indexes: &Indexes, root: SymbolId) -> bool {
  indexes.work.add_queries(1);
  let Some(writes) = indexes.writes_by_root.get(&root) else {
    return false;
  };
  indexes.work.add_queries(writes.len() as u64);
  !writes.is_empty()
}

fn mounted_true_write(indexes: &Indexes, root: SymbolId) -> Option<WriteSite> {
  indexes.work.add_queries(1);
  let writes = indexes.writes_by_root.get(&root)?;
  for write in writes {
    indexes.work.add_queries(1);
    if write.true_literal
      && write.callable.is_some_and(|callable| indexes.mounted.contains(&callable))
    {
      return Some(*write);
    }
  }
  None
}

fn after_tick_demand(
  indexes: &Indexes,
  write: WriteSite,
  root: SymbolId,
) -> Option<(DemandSite, TickSite)> {
  let callable = write.callable?;
  indexes.work.add_queries(1);
  let ticks = indexes.ticks_by_callable.get(&callable)?;
  let mut tick_found = None;
  for tick in ticks {
    indexes.work.add_queries(1);
    if tick.offset > write.offset {
      tick_found = Some(*tick);
      break;
    }
  }
  let tick = tick_found?;
  indexes.work.add_queries(1);
  let demands = indexes.demands_by_key.get(&(callable, root))?;
  let mut demand_found = None;
  for demand in demands {
    indexes.work.add_queries(1);
    if !demand.guarded && demand.offset > tick.offset {
      demand_found = Some(*demand);
      break;
    }
  }
  Some((demand_found?, tick))
}

fn memo_includes_condition(
  semantic: &oxc_semantic::Semantic<'_>,
  memo: &vue_vet_core::TemplateMemoRelation,
  condition_root: SymbolId,
  work: &WorkCounter,
) -> bool {
  let condition_name = semantic.scoping().symbol_name(condition_root);
  work.add_comparisons(1);
  memo.identifiers.as_ref().is_some_and(|names| names.iter().any(|name| name == condition_name))
}

fn memo_deps_unknown(
  semantic: &oxc_semantic::Semantic<'_>,
  indexes: &Indexes,
  memo: &vue_vet_core::TemplateMemoRelation,
  condition_root: SymbolId,
) -> bool {
  let Some(identifiers) = memo.identifiers.as_ref() else {
    return !memo.empty;
  };
  if identifiers.is_empty() {
    return false;
  }
  for name in identifiers {
    indexes.work.add_queries(1);
    let Some(symbol) = root_binding(semantic, name) else {
      return true;
    };
    let root = indexes.root_of(symbol);
    if root == condition_root {
      return true;
    }
    if indexes.computed.contains(&root) {
      return true;
    }
    if indexes.escaped.contains(&root) || indexes.reassigned.contains(&root) {
      return true;
    }
    if any_write(indexes, root) {
      return true;
    }
  }
  false
}

fn preceding_ref_guard(
  statements: &[Statement<'_>],
  demand_offset: usize,
  semantic: &oxc_semantic::Semantic<'_>,
  indexes: &Indexes,
  root: SymbolId,
) -> bool {
  for statement in statements {
    indexes.work.add_queries(1);
    let start = usize::try_from(statement.span().start).unwrap_or(0);
    if start >= demand_offset {
      break;
    }
    if is_ref_early_exit(statement, semantic, indexes, root) {
      return true;
    }
  }
  false
}

fn is_ref_early_exit(
  statement: &Statement<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  indexes: &Indexes,
  root: SymbolId,
) -> bool {
  let Statement::IfStatement(guard) = statement else {
    return false;
  };
  if guard.alternate.is_some() || !is_early_exit(&guard.consequent) {
    return false;
  }
  test_is_absent_ref(&guard.test, semantic, indexes, root)
}

fn is_early_exit(statement: &Statement<'_>) -> bool {
  match statement {
    Statement::ReturnStatement(_)
    | Statement::ThrowStatement(_)
    | Statement::ContinueStatement(_)
    | Statement::BreakStatement(_) => true,
    Statement::BlockStatement(block) => match block.body.as_slice() {
      [only] => is_early_exit(only),
      _ => false,
    },
    _ => false,
  }
}

fn test_is_absent_ref(
  expr: &Expression<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  indexes: &Indexes,
  root: SymbolId,
) -> bool {
  match expr.get_inner_expression() {
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
      expr_is_ref_value(&unary.argument, semantic, indexes, root)
    }
    Expression::BinaryExpression(binary)
      if matches!(binary.operator, BinaryOperator::Equality | BinaryOperator::StrictEquality) =>
    {
      (expr_is_ref_value(&binary.left, semantic, indexes, root) && is_nullish(&binary.right))
        || (expr_is_ref_value(&binary.right, semantic, indexes, root) && is_nullish(&binary.left))
    }
    Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::Or => {
      test_is_absent_ref(&logical.left, semantic, indexes, root)
        && test_is_absent_ref(&logical.right, semantic, indexes, root)
    }
    _ => false,
  }
}

fn expr_is_ref_value(
  expr: &Expression<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  indexes: &Indexes,
  root: SymbolId,
) -> bool {
  let Expression::StaticMemberExpression(member) = expr.get_inner_expression() else {
    return false;
  };
  if member.property.name.as_str() != "value" {
    return false;
  }
  let Expression::Identifier(identifier) = member.object.get_inner_expression() else {
    return false;
  };
  reference_symbol(semantic, identifier).is_some_and(|symbol| indexes.root_of(symbol) == root)
}

fn is_nullish(expr: &Expression<'_>) -> bool {
  match expr.get_inner_expression() {
    Expression::NullLiteral(_) => true,
    Expression::Identifier(identifier) if identifier.name.as_str() == "undefined" => true,
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => true,
    _ => false,
  }
}

fn mapped(
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  span: Span,
) -> SourceSpan {
  source_span(line_index, sfc_source, script_offset, span)
}
