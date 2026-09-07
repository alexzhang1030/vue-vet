//! Event-order join of activations, stops, and writes (no W×C clones).

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{AstKind, ast::Argument};
use oxc_semantic::{NodeId, Semantic, SymbolId};
use oxc_span::Span;
use vue_vet_core::{
  NotificationBypassFact, NotificationBypassKind, ReactiveReadKind, ReactiveViewKind, ScriptKind,
  SourceSpan, TrackingScopeFact, TrackingScopeKind,
};

use super::super::follow::FileTraceIndex;
use super::super::kinds::{resolved_vue_callee, source_span};
use super::paths::{
  assignment_member_chain, is_outermost_member, member_node_chain, peel,
  simple_assignment_member_chain,
};
use super::provenance::{
  ProvenanceIndex, payload_allows_nested_write, payload_path_has_wrapped, resolve_view,
};
use super::uses::{
  FlushKind, NotificationWork, OwnerIndex, UseRole, binding_symbol_at, call_is_unconditional,
  enclosing_region, identifier_symbol, watch_effect_flush,
};

fn bypass_kind(
  write_view: ReactiveViewKind,
  consumer_view: ReactiveViewKind,
  source_kind: vue_vet_core::ReactiveBindingKind,
  path: &[String],
) -> Option<NotificationBypassKind> {
  match write_view {
    ReactiveViewKind::ShallowContainer if consumer_view == ReactiveViewKind::ShallowContainer => {
      let frontier = match source_kind {
        vue_vet_core::ReactiveBindingKind::ShallowRef
        | vue_vet_core::ReactiveBindingKind::ShallowReactive => 1,
        _ => return None,
      };
      if path.len() <= frontier {
        return None;
      }
      if source_kind == vue_vet_core::ReactiveBindingKind::ShallowRef
        && path.first().map(String::as_str) != Some("value")
      {
        return None;
      }
      Some(NotificationBypassKind::ShallowNested)
    }
    ReactiveViewKind::Raw if consumer_view == ReactiveViewKind::Proxy && !path.is_empty() => {
      Some(NotificationBypassKind::ToRawWrite)
    }
    _ => None,
  }
}

const ALIAS_BUDGET_JOIN: u32 = super::provenance::ALIAS_BUDGET;

struct ActiveConsumer {
  span: SourceSpan,
  view: ReactiveViewKind,
  handle: Option<SymbolId>,
  activate: u32,
}

enum Event {
  Activate {
    at: u32,
    origin: usize,
    path: Vec<String>,
    region: NodeId,
    consumer_span: SourceSpan,
    view: ReactiveViewKind,
    handle: Option<SymbolId>,
  },
  Stop {
    at: u32,
    handle: SymbolId,
  },
  Write {
    at: u32,
    origin: usize,
    path: Vec<String>,
    region: NodeId,
    write_span: SourceSpan,
    view: ReactiveViewKind,
    source_idx: usize,
  },
}

impl Event {
  const fn at(&self) -> u32 {
    match self {
      Self::Activate { at, .. } | Self::Stop { at, .. } | Self::Write { at, .. } => *at,
    }
  }

  const fn order(&self) -> u8 {
    match self {
      Self::Activate { .. } => 0,
      Self::Stop { .. } => 1,
      Self::Write { .. } => 2,
    }
  }
}

#[expect(clippy::too_many_arguments, reason = "one collect-once join over shared indexes")]
pub(super) fn join_bypasses(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  provenance: &mut ProvenanceIndex,
  owners: &OwnerIndex,
  file_index: &FileTraceIndex,
  scopes: &[TrackingScopeFact],
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  work: &mut NotificationWork,
) -> Vec<NotificationBypassFact> {
  let mut events = Vec::new();
  collect_activation_events(
    semantic,
    imported_bindings,
    provenance,
    owners,
    file_index,
    scopes,
    sfc_source,
    script_offset,
    script_kind,
    &mut events,
    work,
  );
  collect_stop_events(owners, &mut events, work);
  let triggered =
    collect_trigger_events(semantic, imported_bindings, provenance, script_kind, work);
  collect_write_events(
    semantic,
    imported_bindings,
    provenance,
    sfc_source,
    script_offset,
    script_kind,
    &mut events,
    work,
  );
  events.sort_by_key(|event| (event.at(), event.order()));
  work.events += events.len();

  let mut active: BTreeMap<(usize, NodeId, Vec<String>), ActiveConsumer> = BTreeMap::new();
  let mut active_keys_by_handle: BTreeMap<SymbolId, Vec<(usize, NodeId, Vec<String>)>> =
    BTreeMap::new();
  let mut stopped = BTreeSet::new();
  let mut bypasses = Vec::new();

  for event in events {
    match event {
      Event::Activate { origin, path, region, consumer_span, view, handle, at, .. } => {
        work.lookups = work.lookups.saturating_add(1);
        if handle.is_some_and(|handle| stopped.contains(&handle)) {
          continue;
        }
        work.lookups = work.lookups.saturating_add(1);
        work.copies = work.copies.saturating_add(path.len().max(1));
        let key = (origin, region, path);
        let vacant = !active.contains_key(&key);
        active.entry(key.clone()).or_insert(ActiveConsumer {
          span: consumer_span,
          view,
          handle,
          activate: at,
        });
        if vacant && let Some(handle) = handle {
          work.lookups = work.lookups.saturating_add(1);
          active_keys_by_handle.entry(handle).or_default().push(key);
        }
      }
      Event::Stop { handle, .. } => {
        stopped.insert(handle);
        work.lookups = work.lookups.saturating_add(1);
        let Some(keys) = active_keys_by_handle.remove(&handle) else {
          continue;
        };
        for key in keys {
          work.stop_bucket_visits = work.stop_bucket_visits.saturating_add(1);
          work.lookups = work.lookups.saturating_add(1);
          if active.get(&key).is_some_and(|consumer| consumer.handle == Some(handle)) {
            active.remove(&key);
          }
        }
      }
      Event::Write { origin, path, region, write_span, view, source_idx, at, .. } => {
        work.lookups = work.lookups.saturating_add(1);
        let Some(source) = provenance.record(source_idx) else {
          continue;
        };
        if !provenance.is_effectively_valid(source_idx) {
          continue;
        }
        if source.region != region {
          continue;
        }
        work.copies = work.copies.saturating_add(path.len().max(1));
        work.lookups = work.lookups.saturating_add(1);
        let Some(consumer) = active.get(&(origin, region, path.clone())) else {
          continue;
        };
        if consumer.activate >= at {
          continue;
        }
        let Some(kind) = bypass_kind(view, consumer.view, source.source_kind, &path) else {
          continue;
        };
        let Some(payload) = provenance.canonical_payload(source_idx) else {
          continue;
        };
        if !payload_allows_nested_write(payload, &path, source.source_kind)
          || payload_path_has_wrapped(payload, &path)
        {
          continue;
        }
        if kind == NotificationBypassKind::ShallowNested
          && source.source_kind == vue_vet_core::ReactiveBindingKind::ShallowRef
          && triggered.contains(&(region, origin))
        {
          continue;
        }
        bypasses.push(NotificationBypassFact {
          kind,
          write_span,
          source_span: source.creation_span,
          consumer_span: consumer.span,
          path,
          source_name: source.name.clone(),
        });
      }
    }
  }
  bypasses.sort_by_key(|fact| (fact.write_span.offset, fact.kind as u8));
  work.emissions = bypasses.len();
  bypasses
}

#[expect(clippy::too_many_arguments, reason = "shared indexes passed in rather than rebuilt")]
fn collect_activation_events(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  provenance: &mut ProvenanceIndex,
  owners: &OwnerIndex,
  file_index: &FileTraceIndex,
  scopes: &[TrackingScopeFact],
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  events: &mut Vec<Event>,
  work: &mut NotificationWork,
) {
  let mut scope_by_call_offset = BTreeMap::new();
  for (idx, scope) in scopes.iter().enumerate() {
    if matches!(scope.kind, TrackingScopeKind::WatchSyncEffect | TrackingScopeKind::WatchEffect) {
      work.lookups = work.lookups.saturating_add(1);
      scope_by_call_offset.insert(scope.span.offset, idx);
    }
  }
  let read_indexes: Vec<ReadIndex> =
    scopes.iter().map(|scope| ReadIndex::from_scope(scope, work)).collect();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    work.node_visits += 1;
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind)
    else {
      continue;
    };
    let sync = match callee.as_str() {
      "watchSyncEffect" => true,
      "watchEffect" => watch_effect_flush(call) == FlushKind::Sync,
      _ => false,
    };
    if !sync {
      continue;
    }
    if !call_is_unconditional(semantic, node_id, work) {
      continue;
    }
    let Some(callback) = call.arguments.first() else {
      continue;
    };
    let Some(scope_id) = callback_node_id(callback) else {
      continue;
    };
    work.lookups = work.lookups.saturating_add(1);
    let call_offset = script_offset.saturating_add(usize::try_from(call.span.start).unwrap_or(0));
    let Some(&scope_idx) = scope_by_call_offset.get(&call_offset) else {
      continue;
    };
    let Some(scope) = scopes.get(scope_idx) else {
      continue;
    };
    if !matches!(scope.kind, TrackingScopeKind::WatchSyncEffect | TrackingScopeKind::WatchEffect) {
      continue;
    }
    let handle = assigned_handle(semantic, owners, node_id, work);
    if handle.is_some_and(|handle| handle_escaped(owners, handle, work)) {
      continue;
    }
    let region = enclosing_region(semantic, node_id, work);
    let reads = read_indexes.get(scope_idx);
    for owned in file_index.nodes().members(scope_id) {
      work.queries += 1;
      if owned.outside || !is_outermost_member(semantic, owned.id, work) {
        continue;
      }
      let Some((root, path, member_span)) = member_node_chain(semantic, owned.id) else {
        continue;
      };
      if path.is_empty() {
        continue;
      }
      if !unconditional_scope_read_indexed(
        reads,
        root.name.as_str(),
        member_span,
        script_offset,
        work,
      ) {
        continue;
      }
      let Some(symbol_id) = identifier_symbol(semantic, root) else {
        continue;
      };
      work.lookups = work.lookups.saturating_add(1);
      let Some(source_idx) = resolve_view(provenance, symbol_id, ALIAS_BUDGET_JOIN, work) else {
        continue;
      };
      let Some(source) = provenance.record(source_idx) else {
        continue;
      };
      if !provenance.is_effectively_valid(source_idx) || source.view == ReactiveViewKind::Raw {
        continue;
      }
      events.push(Event::Activate {
        at: call.span.start,
        origin: source.creation_span.offset,
        path,
        region,
        consumer_span: source_span(sfc_source, script_offset, member_span),
        view: source.view,
        handle,
      });
    }
  }
}

struct BindingReads {
  starts: Vec<usize>,
  suffix_min_end: Vec<usize>,
}

struct ReadIndex {
  by_binding: BTreeMap<String, BindingReads>,
}

impl ReadIndex {
  fn from_scope(scope: &TrackingScopeFact, work: &mut NotificationWork) -> Self {
    let mut grouped: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for read in &scope.reads {
      work.candidate_visits = work.candidate_visits.saturating_add(1);
      if read.kind != ReactiveReadKind::Unconditional {
        continue;
      }
      work.lookups = work.lookups.saturating_add(1);
      grouped
        .entry(read.binding.clone())
        .or_default()
        .push((read.span.offset, read.span.offset.saturating_add(read.span.length)));
    }
    let mut by_binding = BTreeMap::new();
    for (binding, mut spans) in grouped {
      spans.sort_by_key(|(start, _)| *start);
      let mut suffix_min_end = vec![0; spans.len()];
      let mut min_end = usize::MAX;
      for (rev, (_, end)) in spans.iter().enumerate().rev() {
        min_end = min_end.min(*end);
        if let Some(slot) = suffix_min_end.get_mut(rev) {
          *slot = min_end;
        }
      }
      by_binding.insert(
        binding,
        BindingReads {
          starts: spans.into_iter().map(|(start, _)| start).collect(),
          suffix_min_end,
        },
      );
    }
    Self { by_binding }
  }
}

fn unconditional_scope_read_indexed(
  reads: Option<&ReadIndex>,
  binding: &str,
  member_span: Span,
  script_offset: usize,
  work: &mut NotificationWork,
) -> bool {
  let Some(index) = reads else {
    return false;
  };
  work.lookups = work.lookups.saturating_add(1);
  let Some(bucket) = index.by_binding.get(binding) else {
    return false;
  };
  let member_start = script_offset.saturating_add(usize::try_from(member_span.start).unwrap_or(0));
  let member_end = script_offset.saturating_add(usize::try_from(member_span.end).unwrap_or(0));
  work.lookups = work.lookups.saturating_add(1);
  let pos = bucket.starts.partition_point(|start| *start < member_start);
  bucket.suffix_min_end.get(pos).is_some_and(|min_end| *min_end <= member_end)
}

fn callback_node_id(argument: &Argument<'_>) -> Option<NodeId> {
  match argument {
    Argument::ArrowFunctionExpression(callback) => Some(callback.node_id.get()),
    Argument::FunctionExpression(callback) => Some(callback.node_id.get()),
    _ => None,
  }
}

fn assigned_handle(
  semantic: &Semantic<'_>,
  owners: &OwnerIndex,
  call_id: NodeId,
  work: &mut NotificationWork,
) -> Option<SymbolId> {
  let parent = semantic.nodes().parent_kind(call_id);
  let AstKind::VariableDeclarator(declarator) = parent else {
    return None;
  };
  let oxc_ast::ast::BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
    return None;
  };
  binding_symbol_at(owners, identifier.span, work)
}

fn handle_escaped(owners: &OwnerIndex, handle: SymbolId, work: &mut NotificationWork) -> bool {
  work.lookups = work.lookups.saturating_add(1);
  let Some(sites) = owners.by_symbol.get(&handle) else {
    return false;
  };
  work.use_sites = work.use_sites.saturating_add(sites.len());
  sites.iter().any(|site| {
    matches!(
      site.role,
      UseRole::Invalid
        | UseRole::ClosedAlias
        | UseRole::VueApiArg
        | UseRole::PayloadReplace { .. }
        | UseRole::NestedWrite
    )
  })
}

fn collect_stop_events(owners: &OwnerIndex, events: &mut Vec<Event>, work: &mut NotificationWork) {
  for (symbol_id, sites) in &owners.by_symbol {
    work.lookups = work.lookups.saturating_add(1);
    work.use_sites = work.use_sites.saturating_add(sites.len());
    for site in sites {
      work.candidate_visits = work.candidate_visits.saturating_add(1);
      if matches!(site.role, UseRole::HandleStop) {
        events.push(Event::Stop { at: site.span.start, handle: *symbol_id });
      }
    }
  }
}

fn collect_trigger_events(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  provenance: &mut ProvenanceIndex,
  script_kind: ScriptKind,
  work: &mut NotificationWork,
) -> BTreeSet<(NodeId, usize)> {
  let mut triggered = BTreeSet::new();
  for node in semantic.nodes() {
    work.node_visits += 1;
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    if resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind).as_deref()
      != Some("triggerRef")
    {
      continue;
    }
    for argument in &call.arguments {
      let Some(expression) = argument.as_expression() else {
        continue;
      };
      let Some(identifier) = peel(expression).get_identifier_reference() else {
        continue;
      };
      let Some(symbol_id) = identifier_symbol(semantic, identifier) else {
        continue;
      };
      work.lookups = work.lookups.saturating_add(1);
      let Some(source_idx) = resolve_view(provenance, symbol_id, ALIAS_BUDGET_JOIN, work) else {
        continue;
      };
      let Some(source) = provenance.record(source_idx) else {
        continue;
      };
      triggered.insert((
        enclosing_region(semantic, call.node_id.get(), work),
        source.creation_span.offset,
      ));
    }
  }
  triggered
}

#[expect(clippy::too_many_arguments, reason = "shared indexes passed in rather than rebuilt")]
fn collect_write_events(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  provenance: &mut ProvenanceIndex,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  events: &mut Vec<Event>,
  work: &mut NotificationWork,
) {
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    work.node_visits += 1;
    let chain = match node.kind() {
      AstKind::AssignmentExpression(assignment) if !assignment.operator.is_logical() => {
        assignment_member_chain(&assignment.left)
      }
      AstKind::UpdateExpression(update) => simple_assignment_member_chain(&update.argument),
      _ => None,
    };
    let (symbol_id, path, span, force_raw) = if let Some((root, path, span)) = chain {
      let Some(symbol_id) = identifier_symbol(semantic, root) else {
        continue;
      };
      (symbol_id, path, span, false)
    } else if let AstKind::AssignmentExpression(assignment) = node.kind()
      && !assignment.operator.is_logical()
      && let Some((symbol_id, path, span)) =
        toraw_assignment(semantic, imported_bindings, script_kind, assignment)
    {
      (symbol_id, path, span, true)
    } else {
      continue;
    };
    if path.is_empty() {
      continue;
    }
    work.lookups = work.lookups.saturating_add(1);
    let Some(source_idx) = resolve_view(provenance, symbol_id, ALIAS_BUDGET_JOIN, work) else {
      continue;
    };
    let Some(record) = provenance.record(source_idx) else {
      continue;
    };
    if force_raw && record.view != ReactiveViewKind::Proxy {
      continue;
    }
    let view = if force_raw { ReactiveViewKind::Raw } else { record.view };
    events.push(Event::Write {
      at: span.start,
      origin: record.creation_span.offset,
      path,
      region: enclosing_region(semantic, node_id, work),
      write_span: source_span(sfc_source, script_offset, span),
      view,
      source_idx,
    });
  }
}

fn toraw_assignment<'a>(
  semantic: &'a Semantic<'a>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  assignment: &'a oxc_ast::ast::AssignmentExpression<'a>,
) -> Option<(SymbolId, Vec<String>, Span)> {
  let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assignment.left else {
    return None;
  };
  if member.optional {
    return None;
  }
  let oxc_ast::ast::Expression::CallExpression(call) = peel(&member.object) else {
    return None;
  };
  if resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind).as_deref()
    != Some("toRaw")
  {
    return None;
  }
  let argument = call.arguments.first()?.as_expression()?;
  let identifier = peel(argument).get_identifier_reference()?;
  let symbol_id = identifier_symbol(semantic, identifier)?;
  Some((symbol_id, vec![member.property.name.to_string()], member.span))
}
