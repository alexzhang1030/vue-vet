//! Late bound-`onCleanup` cancellation guard on the lifetime index.
//!
//! A run-local boolean flag that is registered after a source-dependent `await`
//! cannot observe the invalidation that caused that await to be stale. Bound
//! `onCleanup` still attaches for *later* invalidation; that is a different
//! contract from `onWatcherCleanup` owner loss.

use std::collections::{HashMap, HashSet};

#[cfg(test)]
use std::cell::Cell;

use oxc_ast::{
  AstKind,
  ast::{
    AssignmentOperator, AssignmentTarget, CallExpression, Expression, ObjectPropertyKind,
    PropertyKind, UnaryOperator, VariableDeclarationKind,
  },
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  LateCancellationGuardFact, LateCancellationGuardKind, ReactivityLifetimeFacts, WatcherApiKind,
};

use super::index::LifetimeIndex;
use super::resolve::{
  FunctionResolver, argument_expression, binding_symbol, call_has_spread, callback_parameter_at,
  cleanup_parameter_index, enclosing_function, referenced_symbol, vue_callee_export,
};
use crate::facts::source_span;

const ALIAS_BOUND: usize = 8;
const EXPR_BOUND: usize = 8;

/// One saturating cell — same ZST-in-production contract as
/// `source_contracts::stats::WorkCounter`.
#[derive(Default)]
pub(super) struct SaturatingCount {
  #[cfg(test)]
  value: Cell<usize>,
}

impl SaturatingCount {
  #[cfg(test)]
  fn add(&self, n: usize) {
    self.value.set(self.value.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add(&self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) const fn get(&self) -> usize {
    self.value.get()
  }

  #[cfg(not(test))]
  #[expect(clippy::unused_self, reason = "production snapshot is always zero")]
  pub(super) const fn get(&self) -> usize {
    0
  }
}

/// Settlement-specific counters. Production `SettlementWork` is a ZST.
#[derive(Default)]
pub(super) struct SettlementWork {
  pub ast: SaturatingCount,
  pub references: SaturatingCount,
  pub wrappers: SaturatingCount,
  pub aliases: SaturatingCount,
  pub joins: SaturatingCount,
  pub sorts: SaturatingCount,
  pub queries: SaturatingCount,
}

impl SettlementWork {
  fn add_ast(&self, n: usize) {
    self.ast.add(n);
  }

  fn add_references(&self, n: usize) {
    self.references.add(n);
  }

  fn add_wrappers(&self, n: usize) {
    self.wrappers.add(n);
  }

  fn add_aliases(&self, n: usize) {
    self.aliases.add(n);
  }

  fn add_joins(&self, n: usize) {
    self.joins.add(n);
  }

  fn add_sorts(&self, n: usize) {
    self.sorts.add(n);
  }

  fn add_queries(&self, n: usize) {
    self.queries.add(n);
  }

  #[must_use]
  pub const fn total(&self) -> usize {
    self
      .ast
      .get()
      .saturating_add(self.references.get())
      .saturating_add(self.wrappers.get())
      .saturating_add(self.aliases.get())
      .saturating_add(self.joins.get())
      .saturating_add(self.sorts.get())
      .saturating_add(self.queries.get())
  }
}

#[derive(Clone, Copy)]
struct AwaitSite {
  node_id: NodeId,
  span: Span,
  result: Option<SymbolId>,
}

#[derive(Clone, Copy)]
struct CallSite {
  node_id: NodeId,
  span: Span,
  callee: Option<SymbolId>,
  vue_cleanup: bool,
}

#[derive(Clone, Copy)]
struct IfSite {
  node_id: NodeId,
  span: Span,
  consequent: Span,
  has_alternate: bool,
}

#[derive(Clone, Copy)]
struct WriteSite {
  node_id: NodeId,
  span: Span,
  sink: Option<SymbolId>,
}

#[derive(Clone, Copy)]
struct FalseInit {
  function: NodeId,
  span: Span,
}

struct SettlementIndex<'a> {
  enclosing: &'a mut HashMap<NodeId, Option<NodeId>>,
  awaits: HashMap<NodeId, Vec<AwaitSite>>,
  calls: HashMap<NodeId, Vec<CallSite>>,
  ifs: HashMap<NodeId, Vec<IfSite>>,
  writes: HashMap<NodeId, Vec<WriteSite>>,
  false_inits: HashMap<SymbolId, FalseInit>,
  /// Per-flag functions that assign `= true`, indexed once during observe.
  true_assigns: HashMap<SymbolId, HashSet<NodeId>>,
  ref_allocs: HashSet<SymbolId>,
  written: HashSet<SymbolId>,
  pending_alias: HashMap<SymbolId, SymbolId>,
  aliases: HashMap<SymbolId, SymbolId>,
  decl_fn: HashMap<SymbolId, Option<NodeId>>,
  uncertain: HashSet<NodeId>,
  escaped: HashSet<SymbolId>,
}

struct PendingFact {
  registration: Span,
  await_span: Span,
  write: Span,
  flag: Span,
}

#[derive(Clone, Copy)]
pub(super) struct SpanOut<'a> {
  pub line_index: &'a vue_vet_core::LineIndex,
  pub sfc_source: &'a str,
  pub script_offset: usize,
}

pub(super) fn emit(
  semantic: &oxc_semantic::Semantic<'_>,
  lifetime: &mut LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  vue_exports: &HashMap<SymbolId, String>,
  spans: SpanOut<'_>,
  facts: &mut ReactivityLifetimeFacts,
) -> SettlementWork {
  let work = SettlementWork::default();
  let mut index = SettlementIndex {
    enclosing: &mut lifetime.enclosing,
    awaits: HashMap::new(),
    calls: HashMap::new(),
    ifs: HashMap::new(),
    writes: HashMap::new(),
    false_inits: HashMap::new(),
    true_assigns: HashMap::new(),
    ref_allocs: HashSet::new(),
    written: HashSet::new(),
    pending_alias: HashMap::new(),
    aliases: HashMap::new(),
    decl_fn: HashMap::new(),
    uncertain: HashSet::new(),
    escaped: HashSet::new(),
  };
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    work.add_ast(1);
    observe(semantic, node_id, node.kind(), vue_exports, &mut index, &work);
  }
  finalize_aliases(&mut index, &work);
  sort_sites(&mut index, &work);

  // One fact per callback: a shared named function used by two `watch` calls
  // has a single late-registration site, which is what the diagnostic names.
  let mut analyzed: HashSet<(NodeId, WatcherApiKind)> = HashSet::new();
  for watcher in &lifetime.watchers {
    let Some(callback) = watcher.callback else {
      continue;
    };
    if !callback.is_async || lifetime.incomplete_registration.contains(&callback.node_id) {
      continue;
    }
    let key = (callback.node_id, watcher.api);
    if !analyzed.insert(key) {
      work.add_queries(1);
      continue;
    }
    let Some(pending) = analyze_callback(
      semantic,
      resolver,
      &mut index,
      watcher.node_id,
      watcher.api,
      callback.node_id,
      &work,
    ) else {
      continue;
    };
    facts.late_cancellation_guards.push(LateCancellationGuardFact {
      kind: LateCancellationGuardKind::AfterSourceDependentAwait,
      api: watcher.api,
      registration_span: span_of(&spans, pending.registration),
      await_span: span_of(&spans, pending.await_span),
      write_span: span_of(&spans, pending.write),
      callback_span: span_of(&spans, callback.span),
      watcher_span: span_of(&spans, watcher.span),
      flag_span: span_of(&spans, pending.flag),
    });
  }
  work
}

fn observe(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  kind: AstKind<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut SettlementIndex<'_>,
  work: &SettlementWork,
) {
  match kind {
    AstKind::AwaitExpression(await_expression) => {
      let Some(function_id) = enclosing_function(semantic, node_id, index.enclosing) else {
        return;
      };
      if index.uncertain.contains(&function_id) {
        return;
      }
      let result = await_result_symbol(semantic, node_id, work);
      index.awaits.entry(function_id).or_default().push(AwaitSite {
        node_id,
        span: await_expression.span,
        result,
      });
    }
    AstKind::CallExpression(call) => {
      observe_call(semantic, node_id, call, vue_exports, index, work);
    }
    AstKind::IfStatement(if_statement) => {
      let Some(function_id) = enclosing_function(semantic, node_id, index.enclosing) else {
        return;
      };
      index.ifs.entry(function_id).or_default().push(IfSite {
        node_id,
        span: if_statement.span,
        consequent: if_statement.consequent.span(),
        has_alternate: if_statement.alternate.is_some(),
      });
    }
    AstKind::AssignmentExpression(assignment) => {
      observe_assignment(
        semantic,
        node_id,
        assignment.operator,
        &assignment.left,
        &assignment.right,
        index,
        work,
      );
    }
    AstKind::VariableDeclarator(declarator) => {
      observe_declarator(semantic, node_id, declarator, vue_exports, index);
    }
    AstKind::ForStatement(_)
    | AstKind::ForInStatement(_)
    | AstKind::ForOfStatement(_)
    | AstKind::WhileStatement(_)
    | AstKind::DoWhileStatement(_)
    | AstKind::SwitchStatement(_)
    | AstKind::TryStatement(_) => {
      if let Some(function_id) = enclosing_function(semantic, node_id, index.enclosing) {
        index.uncertain.insert(function_id);
      }
    }
    AstKind::ReturnStatement(ret) => {
      if let Some(argument) = &ret.argument {
        mark_escaped_identifier(semantic, argument, index, work);
      }
    }
    _ => {}
  }
}

fn observe_call(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut SettlementIndex<'_>,
  work: &SettlementWork,
) {
  let Some(function_id) = enclosing_function(semantic, node_id, index.enclosing) else {
    record_call_argument_escape(semantic, call, index, work);
    return;
  };
  if call_has_spread(call) {
    index.uncertain.insert(function_id);
  }
  let callee =
    call.callee.get_inner_expression().get_identifier_reference().and_then(|identifier| {
      work.add_wrappers(1);
      referenced_symbol(semantic, identifier)
    });
  let vue_cleanup =
    vue_callee_export(semantic, &call.callee, vue_exports) == Some("onWatcherCleanup");
  index.calls.entry(function_id).or_default().push(CallSite {
    node_id,
    span: call.span,
    callee,
    vue_cleanup,
  });
  record_call_argument_escape(semantic, call, index, work);
  if let Some("ref" | "shallowRef") = vue_callee_export(semantic, &call.callee, vue_exports)
    && let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(node_id)
    && let Some(symbol_id) = binding_symbol(&declarator.id)
  {
    index.ref_allocs.insert(symbol_id);
  }
}

fn observe_assignment(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  operator: AssignmentOperator,
  left: &AssignmentTarget<'_>,
  right: &Expression<'_>,
  index: &mut SettlementIndex<'_>,
  work: &SettlementWork,
) {
  if let Some(symbol_id) = assignment_identifier(semantic, left) {
    index.written.insert(symbol_id);
    if operator == AssignmentOperator::Assign
      && matches!(right.get_inner_expression(), Expression::BooleanLiteral(literal) if literal.value)
      && let Some(function_id) = enclosing_function(semantic, node_id, index.enclosing)
    {
      index.true_assigns.entry(symbol_id).or_default().insert(function_id);
    }
  }
  if operator == AssignmentOperator::Assign
    && let Some(sink) = value_write_sink(semantic, left)
    && let Some(function_id) = enclosing_function(semantic, node_id, index.enclosing)
  {
    index.writes.entry(function_id).or_default().push(WriteSite {
      node_id,
      span: left.span(),
      sink: Some(sink),
    });
  }
  mark_escaped_identifier(semantic, right, index, work);
}

fn observe_declarator(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  declarator: &oxc_ast::ast::VariableDeclarator<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut SettlementIndex<'_>,
) {
  let Some(symbol_id) = binding_symbol(&declarator.id) else {
    return;
  };
  let function_id = enclosing_function(semantic, node_id, index.enclosing);
  index.decl_fn.insert(symbol_id, function_id);
  let Some(init) = &declarator.init else {
    return;
  };
  if let Some(function_id) = function_id
    && matches!(init.get_inner_expression(), Expression::BooleanLiteral(literal) if !literal.value)
    && !matches!(declarator.kind, VariableDeclarationKind::Const)
  {
    index
      .false_inits
      .insert(symbol_id, FalseInit { function: function_id, span: declarator.id.span() });
  }
  if let Some(init_symbol) = identifier_symbol(semantic, init) {
    index.pending_alias.insert(symbol_id, init_symbol);
  }
  if let Expression::CallExpression(call) = init.get_inner_expression()
    && let Some("ref" | "shallowRef") = vue_callee_export(semantic, &call.callee, vue_exports)
  {
    index.ref_allocs.insert(symbol_id);
  }
}

fn record_call_argument_escape(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
  index: &mut SettlementIndex<'_>,
  work: &SettlementWork,
) {
  for argument in &call.arguments {
    if let Some(expression) = argument_expression(argument) {
      mark_escaped_identifier(semantic, expression, index, work);
    }
  }
}

fn mark_escaped_identifier(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &mut SettlementIndex<'_>,
  work: &SettlementWork,
) {
  if let Some(symbol_id) = identifier_symbol(semantic, expression) {
    work.add_references(1);
    index.escaped.insert(symbol_id);
  }
}

fn await_result_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  await_id: NodeId,
  work: &SettlementWork,
) -> Option<SymbolId> {
  let mut current = await_id;
  for _ in 0..ALIAS_BOUND {
    work.add_wrappers(1);
    let parent = semantic.nodes().parent_id(current);
    if parent == current {
      return None;
    }
    match semantic.nodes().kind(parent) {
      AstKind::VariableDeclarator(declarator) => return binding_symbol(&declarator.id),
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => current = parent,
      _ => return None,
    }
  }
  None
}

fn finalize_aliases(index: &mut SettlementIndex<'_>, work: &SettlementWork) {
  work.add_aliases(index.pending_alias.len());
  for (symbol_id, target) in &index.pending_alias {
    if !index.written.contains(symbol_id) {
      index.aliases.insert(*symbol_id, *target);
    }
  }
}

fn sort_sites(index: &mut SettlementIndex<'_>, work: &SettlementWork) {
  for sites in index.awaits.values_mut() {
    work.add_sorts(sites.len());
    sites.sort_by_key(|site| site.span.start);
  }
  for sites in index.calls.values_mut() {
    work.add_sorts(sites.len());
    sites.sort_by_key(|site| site.span.start);
  }
  for sites in index.ifs.values_mut() {
    work.add_sorts(sites.len());
    sites.sort_by_key(|site| site.span.start);
  }
  for sites in index.writes.values_mut() {
    work.add_sorts(sites.len());
    sites.sort_by_key(|site| site.span.start);
  }
}

fn analyze_callback(
  semantic: &oxc_semantic::Semantic<'_>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &mut SettlementIndex<'_>,
  watcher_id: NodeId,
  api: WatcherApiKind,
  callback_id: NodeId,
  work: &SettlementWork,
) -> Option<PendingFact> {
  if api == WatcherApiKind::Watch {
    let AstKind::CallExpression(call) = semantic.nodes().kind(watcher_id) else {
      return None;
    };
    if closed_watch_options(call, work)?.once {
      return None;
    }
  }
  if index.uncertain.contains(&callback_id) {
    return None;
  }
  let calls = index.calls.get(&callback_id).cloned().unwrap_or_default();
  let await_count = index.awaits.get(&callback_id).map_or(0, Vec::len);
  let if_count = index.ifs.get(&callback_id).map_or(0, Vec::len);
  let write_count = index.writes.get(&callback_id).map_or(0, Vec::len);
  work.add_queries(
    await_count.saturating_add(if_count).saturating_add(write_count).saturating_add(calls.len()),
  );
  if await_count != 1 || if_count != 1 {
    return None;
  }
  let await_site = *index.awaits.get(&callback_id)?.first()?;
  let if_site = *index.ifs.get(&callback_id)?.first()?;
  if if_site.has_alternate || if_site.span.start < await_site.span.end {
    return None;
  }
  let AstKind::IfStatement(if_statement) = semantic.nodes().kind(if_site.node_id) else {
    return None;
  };
  let flag = not_identifier(semantic, &if_statement.test, work)?;
  work.add_joins(1);
  let false_init = *index.false_inits.get(&flag)?;
  if false_init.function != callback_id || index.escaped.contains(&flag) {
    return None;
  }
  if index.decl_fn.get(&flag).copied().flatten() != Some(callback_id) {
    return None;
  }
  let registrar = callback_parameter_at(semantic, callback_id, cleanup_parameter_index(api))?;
  if index.escaped.contains(&registrar) {
    return None;
  }
  let source_param = match api {
    WatcherApiKind::Watch => callback_parameter_at(semantic, callback_id, 0),
    WatcherApiKind::WatchEffect
    | WatcherApiKind::WatchPostEffect
    | WatcherApiKind::WatchSyncEffect => None,
  };
  let source_ref = watch_source_ref(semantic, index, watcher_id, api, work);
  let AstKind::AwaitExpression(await_expression) = semantic.nodes().kind(await_site.node_id) else {
    return None;
  };
  if !depends_on_source(
    semantic,
    index,
    &await_expression.argument,
    DependNeed { needle: source_param, source_ref, api, callback_id },
    0,
    work,
  ) {
    return None;
  }
  work.add_joins(1);
  let result = await_site.result?;
  let mut late_registration = None;
  let mut sync_registration = false;
  let mut registrar_cleanups = HashSet::new();
  for call in &calls {
    work.add_queries(1);
    if !is_flag_registrar(index, call, registrar, work) {
      continue;
    }
    let AstKind::CallExpression(expression) = semantic.nodes().kind(call.node_id) else {
      continue;
    };
    if call_has_spread(expression) || expression.arguments.len() != 1 {
      return None;
    }
    let Some(cleanup_expr) = expression.arguments.first().and_then(argument_expression) else {
      continue;
    };
    let cleanup_fn = resolver.resolve_expression(cleanup_expr)?;
    if enclosing_function(semantic, cleanup_fn.node_id, index.enclosing) != Some(callback_id) {
      return None;
    }
    if !cleanup_sets_flag(index, flag, cleanup_fn.node_id, work) {
      continue;
    }
    work.add_joins(1);
    registrar_cleanups.insert(cleanup_fn.node_id);
    if call.span.start < await_site.span.end {
      sync_registration = true;
    } else {
      late_registration = Some(call.span);
    }
  }
  if !every_true_assign_is_registrar(index, flag, &registrar_cleanups, work) {
    return None;
  }
  if sync_registration {
    return None;
  }
  let registration = late_registration?;
  let writes = index.writes.get(&callback_id).cloned().unwrap_or_default();
  let mut proven_write = None;
  for write in &writes {
    work.add_queries(1);
    if write.span.start < if_site.consequent.start || write.span.end > if_site.consequent.end {
      continue;
    }
    if write.span.start < await_site.span.end {
      continue;
    }
    let Some(sink) = write.sink else {
      continue;
    };
    if !index.ref_allocs.contains(&sink)
      || index.written.contains(&sink)
      || index.decl_fn.get(&sink).copied().flatten() == Some(callback_id)
    {
      continue;
    }
    let AstKind::AssignmentExpression(assignment) = semantic.nodes().kind(write.node_id) else {
      continue;
    };
    if !depends_on_source(
      semantic,
      index,
      &assignment.right,
      DependNeed {
        needle: Some(result),
        source_ref: None,
        api: WatcherApiKind::Watch,
        callback_id: NodeId::DUMMY,
      },
      0,
      work,
    ) {
      continue;
    }
    work.add_joins(1);
    proven_write = Some(write.span);
    break;
  }
  let write = proven_write?;
  Some(PendingFact { registration, await_span: await_site.span, write, flag: false_init.span })
}

fn is_flag_registrar(
  index: &SettlementIndex<'_>,
  call: &CallSite,
  registrar: SymbolId,
  work: &SettlementWork,
) -> bool {
  if call.vue_cleanup {
    return true;
  }
  call.callee.is_some_and(|symbol| resolve_alias(index, symbol, work) == registrar)
}

fn every_true_assign_is_registrar(
  index: &SettlementIndex<'_>,
  flag: SymbolId,
  registrar_cleanups: &HashSet<NodeId>,
  work: &SettlementWork,
) -> bool {
  let Some(assigners) = index.true_assigns.get(&flag) else {
    return !index.written.contains(&flag);
  };
  work.add_queries(assigners.len());
  !assigners.is_empty() && assigners.iter().all(|function| registrar_cleanups.contains(function))
}

fn cleanup_sets_flag(
  index: &SettlementIndex<'_>,
  flag: SymbolId,
  cleanup_id: NodeId,
  work: &SettlementWork,
) -> bool {
  work.add_queries(1);
  index.true_assigns.get(&flag).is_some_and(|assigners| assigners.contains(&cleanup_id))
}

#[derive(Clone, Copy)]
struct ClosedWatchOptions {
  once: bool,
}

/// Closed object literal only — identifier / spread / computed / non-literal
/// `once` stay Unknown. Mirrors `source_contracts` `closed_watch_options`.
fn closed_watch_options(
  call: &CallExpression<'_>,
  work: &SettlementWork,
) -> Option<ClosedWatchOptions> {
  work.add_queries(1);
  let Some(options_arg) = call.arguments.get(2) else {
    return Some(ClosedWatchOptions { once: false });
  };
  let options_expr = options_arg.as_expression()?;
  let Expression::ObjectExpression(object) = options_expr.get_inner_expression() else {
    return None;
  };
  let mut once = None;
  for property in &object.properties {
    work.add_queries(1);
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return None,
      ObjectPropertyKind::ObjectProperty(prop) => {
        if prop.computed || prop.kind != PropertyKind::Init || prop.method || prop.shorthand {
          return None;
        }
        let name = prop.key.static_name()?;
        if name == "once" {
          let Expression::BooleanLiteral(literal) = prop.value.get_inner_expression() else {
            return None;
          };
          if once.is_some() {
            return None;
          }
          once = Some(literal.value);
        }
      }
    }
  }
  Some(ClosedWatchOptions { once: once.unwrap_or(false) })
}

fn watch_source_ref(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &SettlementIndex<'_>,
  watcher_id: NodeId,
  api: WatcherApiKind,
  work: &SettlementWork,
) -> Option<SymbolId> {
  if api != WatcherApiKind::Watch {
    return None;
  }
  let AstKind::CallExpression(call) = semantic.nodes().kind(watcher_id) else {
    return None;
  };
  let expression = argument_expression(call.arguments.first()?)?;
  let symbol_id = identifier_symbol(semantic, expression)?;
  let resolved = resolve_alias(index, symbol_id, work);
  index.ref_allocs.contains(&resolved).then_some(resolved)
}

fn not_identifier(
  semantic: &oxc_semantic::Semantic<'_>,
  test: &Expression<'_>,
  work: &SettlementWork,
) -> Option<SymbolId> {
  let inner = unwrap_counted(test, work);
  let Expression::UnaryExpression(unary) = inner else {
    return None;
  };
  if unary.operator != UnaryOperator::LogicalNot {
    return None;
  }
  identifier_symbol(semantic, &unary.argument)
}

#[derive(Clone, Copy)]
struct DependNeed {
  needle: Option<SymbolId>,
  source_ref: Option<SymbolId>,
  api: WatcherApiKind,
  callback_id: NodeId,
}

fn depends_on_source(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &SettlementIndex<'_>,
  expression: &Expression<'_>,
  need: DependNeed,
  depth: usize,
  work: &SettlementWork,
) -> bool {
  match depend_kind(semantic, index, expression, need, depth, work) {
    Depend::Yes => true,
    Depend::No | Depend::Unknown => false,
  }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Depend {
  No,
  Yes,
  Unknown,
}

fn depend_kind(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &SettlementIndex<'_>,
  expression: &Expression<'_>,
  need: DependNeed,
  depth: usize,
  work: &SettlementWork,
) -> Depend {
  if depth >= EXPR_BOUND {
    return Depend::Unknown;
  }
  let inner = unwrap_counted(expression, work);
  match inner {
    Expression::Identifier(identifier) => {
      let Some(symbol_id) = referenced_symbol(semantic, identifier) else {
        return Depend::No;
      };
      let resolved = resolve_alias(index, symbol_id, work);
      if need.needle == Some(resolved) {
        return Depend::Yes;
      }
      Depend::No
    }
    Expression::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
      let Some(object) = identifier_symbol(semantic, &member.object) else {
        return Depend::Unknown;
      };
      let resolved = resolve_alias(index, object, work);
      if need.source_ref == Some(resolved) {
        return Depend::Yes;
      }
      if need.api != WatcherApiKind::Watch
        && index.ref_allocs.contains(&resolved)
        && index.decl_fn.get(&resolved).copied().flatten() != Some(need.callback_id)
      {
        return Depend::Yes;
      }
      Depend::No
    }
    Expression::CallExpression(call) => {
      let mut saw_yes =
        depend_kind(semantic, index, &call.callee, need, depth.saturating_add(1), work)
          == Depend::Yes;
      let mut saw_unknown = false;
      for argument in &call.arguments {
        let Some(expression) = argument_expression(argument) else {
          saw_unknown = true;
          continue;
        };
        match depend_kind(semantic, index, expression, need, depth.saturating_add(1), work) {
          Depend::Yes => saw_yes = true,
          Depend::Unknown => saw_unknown = true,
          Depend::No => {}
        }
      }
      if saw_yes {
        Depend::Yes
      } else if saw_unknown {
        Depend::Unknown
      } else {
        Depend::No
      }
    }
    Expression::SequenceExpression(sequence) => {
      sequence.expressions.last().map_or(Depend::No, |expression| {
        work.add_wrappers(1);
        depend_kind(semantic, index, expression, need, depth.saturating_add(1), work)
      })
    }
    Expression::BinaryExpression(binary) => join_depend(
      depend_kind(semantic, index, &binary.left, need, depth.saturating_add(1), work),
      depend_kind(semantic, index, &binary.right, need, depth.saturating_add(1), work),
    ),
    Expression::LogicalExpression(logical) => join_depend(
      depend_kind(semantic, index, &logical.left, need, depth.saturating_add(1), work),
      depend_kind(semantic, index, &logical.right, need, depth.saturating_add(1), work),
    ),
    Expression::TemplateLiteral(template) => {
      let mut kind = Depend::No;
      for expression in &template.expressions {
        kind = join_depend(
          kind,
          depend_kind(semantic, index, expression, need, depth.saturating_add(1), work),
        );
      }
      kind
    }
    Expression::ConditionalExpression(_) | Expression::AwaitExpression(_) => Depend::Unknown,
    _ => Depend::No,
  }
}

const fn join_depend(left: Depend, right: Depend) -> Depend {
  match (left, right) {
    (Depend::Unknown, _) | (_, Depend::Unknown) => Depend::Unknown,
    (Depend::Yes, _) | (_, Depend::Yes) => Depend::Yes,
    (Depend::No, Depend::No) => Depend::No,
  }
}

fn unwrap_counted<'a>(expression: &'a Expression<'a>, work: &SettlementWork) -> &'a Expression<'a> {
  let mut current = expression;
  for _ in 0..256 {
    match current {
      Expression::ParenthesizedExpression(inner) => {
        work.add_wrappers(1);
        current = &inner.expression;
      }
      Expression::TSAsExpression(inner) => {
        work.add_wrappers(1);
        current = &inner.expression;
      }
      Expression::TSSatisfiesExpression(inner) => {
        work.add_wrappers(1);
        current = &inner.expression;
      }
      Expression::TSNonNullExpression(inner) => {
        work.add_wrappers(1);
        current = &inner.expression;
      }
      Expression::TSTypeAssertion(inner) => {
        work.add_wrappers(1);
        current = &inner.expression;
      }
      Expression::UnaryExpression(inner) if inner.operator == UnaryOperator::Void => {
        work.add_wrappers(1);
        current = &inner.argument;
      }
      _ => return current.get_inner_expression(),
    }
  }
  current.get_inner_expression()
}

fn resolve_alias(
  index: &SettlementIndex<'_>,
  symbol_id: SymbolId,
  work: &SettlementWork,
) -> SymbolId {
  let mut current = symbol_id;
  for _ in 0..ALIAS_BOUND {
    work.add_aliases(1);
    match index.aliases.get(&current) {
      Some(&next) if next != current => current = next,
      _ => break,
    }
  }
  current
}

fn identifier_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> Option<SymbolId> {
  referenced_symbol(semantic, expression.get_inner_expression().get_identifier_reference()?)
}

fn assignment_identifier(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTarget<'_>,
) -> Option<SymbolId> {
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
      referenced_symbol(semantic, identifier)
    }
    AssignmentTarget::TSAsExpression(inner) => identifier_symbol(semantic, &inner.expression),
    AssignmentTarget::TSNonNullExpression(inner) => identifier_symbol(semantic, &inner.expression),
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      identifier_symbol(semantic, &inner.expression)
    }
    AssignmentTarget::TSTypeAssertion(inner) => identifier_symbol(semantic, &inner.expression),
    _ => None,
  }
}

fn value_write_sink(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTarget<'_>,
) -> Option<SymbolId> {
  match target {
    AssignmentTarget::StaticMemberExpression(member)
      if member.property.name.as_str() == "value" =>
    {
      identifier_symbol(semantic, &member.object)
    }
    AssignmentTarget::TSAsExpression(inner) => {
      value_write_from_expression(semantic, &inner.expression)
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      value_write_from_expression(semantic, &inner.expression)
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      value_write_from_expression(semantic, &inner.expression)
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      value_write_from_expression(semantic, &inner.expression)
    }
    _ => None,
  }
}

fn value_write_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> Option<SymbolId> {
  match expression.get_inner_expression() {
    Expression::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
      identifier_symbol(semantic, &member.object)
    }
    _ => None,
  }
}

fn span_of(spans: &SpanOut<'_>, span: Span) -> vue_vet_core::SourceSpan {
  source_span(spans.line_index, spans.sfc_source, spans.script_offset, span)
}
