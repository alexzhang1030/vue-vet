use std::collections::{HashMap, HashSet};

use oxc_ast::{AstKind, ast::Expression, ast::Statement};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::Span;
use vue_vet_core::{
  LateScopeDisposeFact, LateWatcherCleanupFact, LateWatcherCleanupKind, OrphanedScopeWatcherFact,
  ReactivityLifetimeFacts, ReturnedWatcherCleanupFact, SourceSpan, WatcherApiKind,
};

use super::index::LifetimeIndex;
use super::resolve::{
  FunctionResolver, callback_parameter_at, cleanup_parameter_index, enclosing_function,
  referenced_symbol, uncertain_to_function,
};
use crate::facts::source_span;

pub(super) fn emit_all(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  facts: &mut ReactivityLifetimeFacts,
) -> usize {
  let mut candidates = 0usize;
  let mut returned_cache: HashMap<NodeId, Vec<ReturnedValue>> = HashMap::new();
  let mut registered_cache: HashMap<(NodeId, WatcherApiKind), HashSet<NodeId>> = HashMap::new();

  for watcher in &index.watchers {
    let Some(callback) = watcher.callback else {
      continue;
    };
    if index.incomplete_registration.contains(&callback.node_id) {
      continue;
    }
    let returned = returned_cache
      .entry(callback.node_id)
      .or_insert_with(|| function_returned_values(semantic, resolver, callback.node_id));
    candidates =
      candidates.saturating_add(index.registers_by_fn.get(&callback.node_id).map_or(0, Vec::len));
    let registered = registered_cache
      .entry((callback.node_id, watcher.api))
      .or_insert_with(|| registered_disposers(index, callback.node_id, watcher.api, semantic));
    candidates = candidates.saturating_add(returned.len());
    for value in returned.iter() {
      if value.identity.is_some_and(|identity| registered.contains(&identity)) {
        continue;
      }
      facts.returned_watcher_cleanups.push(ReturnedWatcherCleanupFact {
        api: watcher.api,
        returned_span: span_of(line_index, sfc_source, script_offset, value.span),
        callback_span: span_of(line_index, sfc_source, script_offset, callback.span),
        registration_span: span_of(line_index, sfc_source, script_offset, watcher.span),
        async_callback: callback.is_async,
      });
    }

    let await_span = index.first_await.get(&callback.node_id).copied();
    if let Some(sites) = index.cleanups_by_fn.get(&callback.node_id) {
      candidates = candidates.saturating_add(sites.len());
      for site in sites {
        if site.suppressed {
          continue;
        }
        if let Some(await_span) = await_span
          && !uncertain_to_function(semantic, site.node_id, callback.node_id)
          && site.span.start >= await_span.end
        {
          facts.late_watcher_cleanups.push(LateWatcherCleanupFact {
            api: watcher.api,
            kind: LateWatcherCleanupKind::Await,
            cleanup_span: span_of(line_index, sfc_source, script_offset, site.span),
            callback_span: span_of(line_index, sfc_source, script_offset, callback.span),
            registration_span: span_of(line_index, sfc_source, script_offset, watcher.span),
            boundary_span: span_of(line_index, sfc_source, script_offset, await_span),
          });
        }
      }
    }
    if let Some(deferred) = index.deferred_cleanups_by_parent.get(&callback.node_id) {
      candidates = candidates.saturating_add(deferred.len());
      for item in deferred {
        if item.site.suppressed {
          continue;
        }
        facts.late_watcher_cleanups.push(LateWatcherCleanupFact {
          api: watcher.api,
          kind: LateWatcherCleanupKind::DeferredCallback,
          cleanup_span: span_of(line_index, sfc_source, script_offset, item.site.span),
          callback_span: span_of(line_index, sfc_source, script_offset, callback.span),
          registration_span: span_of(line_index, sfc_source, script_offset, watcher.span),
          boundary_span: span_of(line_index, sfc_source, script_offset, item.boundary),
        });
      }
    }
  }

  let mut enclosing = index.enclosing.clone();
  for watcher in &index.watchers {
    if !watcher.unused {
      continue;
    }
    let Some(function_id) = enclosing_function(semantic, watcher.node_id, &mut enclosing) else {
      continue;
    };
    candidates = candidates.saturating_add(1);
    let Some(run) = index.selected_runs.get(&function_id).and_then(Option::as_ref) else {
      continue;
    };
    if !run.callback.is_async
      || index.reentered_functions.contains(&function_id)
      || run.scope_symbol.is_some_and(|symbol| index.unproven_scopes.contains(&symbol))
    {
      continue;
    }
    let Some(await_span) = index.first_await.get(&function_id) else {
      continue;
    };
    if watcher.span.start < await_span.end
      || uncertain_to_function(semantic, watcher.node_id, function_id)
    {
      continue;
    }
    facts.orphaned_scope_watchers.push(OrphanedScopeWatcherFact {
      api: watcher.api,
      watcher_span: span_of(line_index, sfc_source, script_offset, watcher.span),
      run_span: span_of(line_index, sfc_source, script_offset, run.run_span),
      await_span: span_of(line_index, sfc_source, script_offset, *await_span),
      owner_span: span_of(line_index, sfc_source, script_offset, run.owner_span),
    });
  }

  for sites in index.disposes_by_fn.values() {
    for dispose in sites {
      candidates = candidates.saturating_add(1);
      if dispose.suppressed {
        continue;
      }
      let Some(function_id) = enclosing_function(semantic, dispose.node_id, &mut enclosing) else {
        continue;
      };
      let Some(run) = index.selected_runs.get(&function_id).and_then(Option::as_ref) else {
        continue;
      };
      if !run.callback.is_async
        || index.reentered_functions.contains(&function_id)
        || run.scope_symbol.is_some_and(|symbol| index.unproven_scopes.contains(&symbol))
      {
        continue;
      }
      let Some(await_span) = index.first_await.get(&function_id) else {
        continue;
      };
      if dispose.span.start < await_span.end
        || uncertain_to_function(semantic, dispose.node_id, function_id)
      {
        continue;
      }
      facts.late_scope_disposes.push(LateScopeDisposeFact {
        dispose_span: span_of(line_index, sfc_source, script_offset, dispose.span),
        callback_span: span_of(line_index, sfc_source, script_offset, run.callback.span),
        run_span: span_of(line_index, sfc_source, script_offset, run.run_span),
        await_span: span_of(line_index, sfc_source, script_offset, *await_span),
        owner_span: span_of(line_index, sfc_source, script_offset, run.owner_span),
      });
    }
  }

  candidates
}

#[derive(Clone, Copy)]
struct ReturnedValue {
  span: Span,
  identity: Option<NodeId>,
}

fn function_returned_values(
  semantic: &oxc_semantic::Semantic<'_>,
  resolver: &mut FunctionResolver<'_, '_>,
  function_id: NodeId,
) -> Vec<ReturnedValue> {
  let mut values = Vec::new();
  match semantic.nodes().kind(function_id) {
    AstKind::ArrowFunctionExpression(arrow) if arrow.expression => {
      if let Some(statement) = arrow.body.statements.first() {
        match statement {
          Statement::ReturnStatement(ret) => {
            if let Some(argument) = &ret.argument
              && let Some(value) = proven_function_value(semantic, resolver, argument, function_id)
            {
              values.push(value);
            }
          }
          Statement::ExpressionStatement(expression) => {
            if let Some(value) =
              proven_function_value(semantic, resolver, &expression.expression, function_id)
            {
              values.push(value);
            }
          }
          _ => {}
        }
      }
    }
    AstKind::ArrowFunctionExpression(arrow) => {
      collect_block_returns(semantic, resolver, &arrow.body.statements, function_id, &mut values);
    }
    AstKind::Function(function) => {
      if function.generator {
        return values;
      }
      if let Some(body) = &function.body {
        collect_block_returns(semantic, resolver, &body.statements, function_id, &mut values);
      }
    }
    _ => {}
  }
  values
}

fn collect_block_returns(
  semantic: &oxc_semantic::Semantic<'_>,
  resolver: &mut FunctionResolver<'_, '_>,
  statements: &[Statement<'_>],
  function_id: NodeId,
  values: &mut Vec<ReturnedValue>,
) {
  for statement in statements {
    match statement {
      Statement::ReturnStatement(ret) => {
        if let Some(argument) = &ret.argument
          && let Some(value) = proven_function_value(semantic, resolver, argument, function_id)
        {
          values.push(value);
        }
      }
      Statement::BlockStatement(block) => {
        collect_block_returns(semantic, resolver, &block.body, function_id, values);
      }
      Statement::IfStatement(if_statement) => {
        collect_statement_returns(
          semantic,
          resolver,
          &if_statement.consequent,
          function_id,
          values,
        );
        if let Some(alternate) = &if_statement.alternate {
          collect_statement_returns(semantic, resolver, alternate, function_id, values);
        }
      }
      _ => {}
    }
  }
}

fn collect_statement_returns(
  semantic: &oxc_semantic::Semantic<'_>,
  resolver: &mut FunctionResolver<'_, '_>,
  statement: &Statement<'_>,
  function_id: NodeId,
  values: &mut Vec<ReturnedValue>,
) {
  match statement {
    Statement::BlockStatement(block) => {
      collect_block_returns(semantic, resolver, &block.body, function_id, values);
    }
    Statement::ReturnStatement(ret) => {
      if let Some(argument) = &ret.argument
        && let Some(value) = proven_function_value(semantic, resolver, argument, function_id)
      {
        values.push(value);
      }
    }
    _ => {}
  }
}

fn proven_function_value(
  semantic: &oxc_semantic::Semantic<'_>,
  resolver: &mut FunctionResolver<'_, '_>,
  expression: &Expression<'_>,
  callback_id: NodeId,
) -> Option<ReturnedValue> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => {
      Some(ReturnedValue { span: arrow.span, identity: Some(arrow.node_id.get()) })
    }
    Expression::FunctionExpression(function) if !function.generator => {
      Some(ReturnedValue { span: function.span, identity: Some(function.node_id.get()) })
    }
    Expression::Identifier(identifier) => {
      let symbol_id = referenced_symbol(semantic, identifier)?;
      let function = resolver.resolve_symbol_id(symbol_id)?;
      if !symbol_visible_from(semantic, symbol_id, callback_id) {
        return None;
      }
      Some(ReturnedValue { span: identifier.span, identity: Some(function.node_id) })
    }
    _ => None,
  }
}

fn symbol_visible_from(
  semantic: &oxc_semantic::Semantic<'_>,
  symbol_id: SymbolId,
  callback_id: NodeId,
) -> bool {
  let declaration_id = semantic.scoping().symbol_declaration(symbol_id);
  if declaration_id == callback_id {
    return true;
  }
  semantic.nodes().ancestor_ids(declaration_id).any(|ancestor| ancestor == callback_id)
    || enclosing_function(semantic, declaration_id, &mut HashMap::new()).is_none()
}

fn registered_disposers(
  index: &LifetimeIndex,
  function_id: NodeId,
  api: WatcherApiKind,
  semantic: &oxc_semantic::Semantic<'_>,
) -> HashSet<NodeId> {
  let mut registered = HashSet::new();
  let cleanup_param = callback_parameter_at(semantic, function_id, cleanup_parameter_index(api));
  let await_end = index.first_await.get(&function_id).map(|span| span.end);
  let Some(sites) = index.registers_by_fn.get(&function_id) else {
    return registered;
  };
  for site in sites {
    let matches_param = cleanup_param.is_some() && site.callee == cleanup_param;
    if matches_param {
      if let Some(identity) = site.identity {
        registered.insert(identity);
      }
      continue;
    }
    if site.vue_cleanup {
      if !site.explicit_owner && await_end.is_some_and(|end| site.span.start >= end) {
        continue;
      }
      if let Some(identity) = site.identity {
        registered.insert(identity);
      }
    }
  }
  registered
}

fn span_of(
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  span: Span,
) -> SourceSpan {
  source_span(line_index, sfc_source, script_offset, span)
}
