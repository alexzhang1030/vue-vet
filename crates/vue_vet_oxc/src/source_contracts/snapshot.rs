//! Demand-gated JSON clone loss and history snapshot alias facts.

use std::collections::HashMap;

use oxc_ast::{AstKind, ast::CallExpression};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::Span;

use super::Collector;
use super::index::{CallInfo, MemberUse, NestedWrite, Site, SnapshotCall};
use super::proof::{DemandOrigin, classify_reach, enclosing_call, skip_ts_parent};
use super::shape::{Literal, OptionValue, span_key};
use super::timeline;
use vue_vet_core::{JsonCloneLossyTypeFact, RefHistorySnapshotAliasFact};

const DATE_METHODS: &[&str] = &["getTime", "getUTCFullYear"];
const CLONE_OPTION_KEYS: &[&str] =
  &["manual", "clone", "deep", "immediate", "flush", "once", "onTrack", "onTrigger"];
const HISTORY_OPTION_KEYS: &[&str] = &["clone", "dump", "parse", "setSource", "capacity"];
const EXECUTING_HOOKS: &[&str] = &["onTrack", "onTrigger"];

#[derive(Clone, Copy)]
enum HistoryOpKind {
  Commit,
  Undo,
  Redo,
  Reset,
  Clear,
}

#[derive(Clone, Copy)]
struct HistoryOp {
  offset: usize,
  kind: HistoryOpKind,
}

#[derive(Clone)]
struct RecordState {
  dump_offset: usize,
  generation: u32,
  baseline: HashMap<String, Option<Literal>>,
}

#[derive(Clone, Copy)]
struct WriteEvent {
  write: NestedWrite,
  generation: u32,
}

#[derive(Clone)]
struct HistoryPoint {
  offset: usize,
  last: RecordState,
  undo: Vec<RecordState>,
}

#[derive(Clone)]
pub(super) struct HistorySummary {
  writes: Vec<(String, WriteEvent)>,
  points: Vec<HistoryPoint>,
  unknown: bool,
}

#[derive(Default)]
pub(super) struct SnapshotMemo {
  pub date_paths: HashMap<u64, Option<(Span, String, Span)>>,
  pub closed_data: HashMap<u64, bool>,
  pub histories: HashMap<u64, Option<HistorySummary>>,
}

impl Collector<'_> {
  pub(super) fn collect_json_clone_lossy(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.vueuse != Some("useCloned") || info.has_spread {
      return;
    }
    if self.indexes.date_poisoned
      || self.indexes.json_poisoned
      || self.indexes.string_capability_poisoned
    {
      return;
    }
    if !self.json_clone_executes(info) {
      return;
    }
    let Some(argument) = info.first_arg else {
      return;
    };
    let Some((object_span, path, source_span)) = self.date_source_path(argument) else {
      return;
    };
    if self.object_has_key_named(object_span, "toJSON") {
      return;
    }
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    if let Some(demand) = self.cloned_date_demand(node_id, call, &path, origin) {
      self.facts.json_clone_lossy_type.push(JsonCloneLossyTypeFact {
        demand_span: self.span(demand.span),
        source_span: self.span(source_span),
        clone_span: self.span(call.span),
        path,
        method: demand.method,
        output_kind: "string".into(),
      });
    }
  }

  pub(super) fn collect_history_snapshot_alias(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.vueuse != Some("useManualRefHistory") || info.has_spread {
      return;
    }
    let Some(capacity) = self.history_identity_clone(info) else {
      return;
    };
    let Some(argument) = info.first_arg else {
      return;
    };
    let Some((source, object_span)) = self.history_source(argument) else {
      return;
    };
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    let bindings = self.history_bindings(call.span);
    if let Some(bag) = bindings.bag
      && (self.indexes.reassigned.contains(&bag)
        || self.indexes.escaped.contains(&bag)
        || self.indexes.capability_touch.contains(&bag))
    {
      return;
    }
    let Some(summary) = self.history_summary(source, object_span, &bindings, origin, capacity)
    else {
      return;
    };
    if summary.unknown {
      return;
    }
    let mut chosen: Option<(NestedWrite, String, SnapshotCall, &'static str)> = None;
    for demand in self.history_value_demands(&bindings, origin, source) {
      self.indexes.note_query();
      if demand.site.head.offset <= origin.offset {
        continue;
      }
      let Some(record) = self.consumed_record(&summary, &demand) else {
        continue;
      };
      if let Some((write, property)) =
        self.corrupting_write(&summary, &record, demand.site.head.offset)
        && chosen.as_ref().is_none_or(|(current, _, _, _)| write.offset < current.offset)
      {
        chosen = Some((write, property, demand.site, demand.kind));
      }
    }
    if let Some((write, property, demand, kind)) = chosen {
      self.facts.ref_history_snapshot_alias.push(RefHistorySnapshotAliasFact {
        write_span: self.span(write.span),
        record_span: self.span(call.span),
        demand_span: self.span(demand.head.span),
        property,
        demand_kind: kind.into(),
      });
    }
  }

  fn json_clone_executes(&mut self, info: CallInfo) -> bool {
    let Some(options) = info.second_arg else {
      return true;
    };
    if !self.object_is_closed_data(options) {
      return false;
    }
    if !self.object_keys_allowed(options, CLONE_OPTION_KEYS) {
      return false;
    }
    if EXECUTING_HOOKS
      .iter()
      .any(|key| !matches!(self.option_value(options, key), OptionValue::Absent))
    {
      return false;
    }
    match self.option_value(options, "clone") {
      OptionValue::Absent | OptionValue::Known(Literal::Undefined) => {}
      OptionValue::Known(_) | OptionValue::Unknown => return false,
    }
    match self.option_value(options, "immediate") {
      OptionValue::Absent | OptionValue::Known(Literal::Bool(true) | Literal::Undefined) => true,
      OptionValue::Known(_) | OptionValue::Unknown => false,
    }
  }

  fn history_identity_clone(&mut self, info: CallInfo) -> Option<i64> {
    let Some(options) = info.second_arg else {
      return Some(0);
    };
    if !self.object_is_closed_data(options) {
      return None;
    }
    if !self.object_keys_allowed(options, HISTORY_OPTION_KEYS) {
      return None;
    }
    for key in ["dump", "parse", "setSource"] {
      match self.option_value(options, key) {
        OptionValue::Absent | OptionValue::Known(Literal::Undefined) => {}
        OptionValue::Known(_) | OptionValue::Unknown => return None,
      }
    }
    let capacity = match self.option_value(options, "capacity") {
      OptionValue::Absent | OptionValue::Known(Literal::Undefined) => 0,
      OptionValue::Known(Literal::Number(value)) if value >= 1 => value,
      OptionValue::Known(_) | OptionValue::Unknown => return None,
    };
    match self.option_value(options, "clone") {
      OptionValue::Absent | OptionValue::Known(Literal::Undefined | Literal::Bool(false)) => {
        Some(capacity)
      }
      OptionValue::Known(_) | OptionValue::Unknown => None,
    }
  }

  fn object_keys_allowed(&self, span: Span, allowed: &[&str]) -> bool {
    let Some(entries) = self.indexes.object_index.objects.get(&span_key(span)) else {
      self.indexes.note_query();
      return false;
    };
    self.indexes.note_query();
    entries.iter().all(|entry| {
      self.indexes.note_query();
      match entry {
        super::index::ObjectEntry::Data { name, .. } => allowed.contains(&name.as_str()),
        _ => false,
      }
    })
  }

  fn option_value(&self, object: Span, key: &str) -> OptionValue<Literal> {
    self.indexes.option_value(object, key)
  }

  fn object_is_closed_data(&mut self, span: Span) -> bool {
    let key = span_key(span);
    if let Some(hit) = self.snapshot_memo.closed_data.get(&key) {
      self.indexes.note_query();
      return *hit;
    }
    let hit = self.indexes.object_is_closed_data(span);
    self.snapshot_memo.closed_data.insert(key, hit);
    hit
  }

  fn object_has_key_named(&self, span: Span, name: &str) -> bool {
    self.indexes.object_has_key_named(span, name)
  }

  fn date_source_path(&mut self, argument: Span) -> Option<(Span, String, Span)> {
    let object = self.ref_own_object(argument)?;
    let key = span_key(object);
    if let Some(hit) = self.snapshot_memo.date_paths.get(&key) {
      self.indexes.note_query();
      return hit.clone();
    }
    let found = self.compute_date_source_path(object);
    self.snapshot_memo.date_paths.insert(key, found.clone());
    found
  }

  fn compute_date_source_path(&mut self, object: Span) -> Option<(Span, String, Span)> {
    if !self.object_is_closed_data(object) {
      return None;
    }
    let entries = self.indexes.object_index.objects.get(&span_key(object))?;
    let mut found = None;
    for entry in entries {
      self.indexes.note_query();
      let super::index::ObjectEntry::Data { name, value, .. } = entry else {
        return None;
      };
      if self.indexes.is_native_date(*value) {
        if found.is_some() {
          continue;
        }
        found = Some((name.clone(), *value));
      }
    }
    let (path, date) = found?;
    Some((object, path, date))
  }

  fn ref_own_object(&self, span: Span) -> Option<Span> {
    if let Some(object) = self.ref_call_object(span) {
      return Some(object);
    }
    let super::shape::ShapeHint::Identifier(Some(symbol_id), false) =
      self.indexes.hints.get(&span_key(span)).copied()?
    else {
      return None;
    };
    let root = self.indexes.root_of(symbol_id);
    if self.indexes.reassigned.contains(&root)
      || !self.indexes.value_writes.get(&root).is_none_or(Vec::is_empty)
    {
      return None;
    }
    if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      return None;
    }
    let init = self.indexes.init_span.get(&root).copied()?;
    self.ref_call_object(init)
  }

  fn ref_call_object(&self, span: Span) -> Option<Span> {
    let info = self.call_info_at(span)?;
    if info.has_spread {
      return None;
    }
    let api = info.api?;
    if !matches!(api, "ref" | "shallowRef") {
      return None;
    }
    let argument = info.first_arg?;
    match self.indexes.hints.get(&span_key(argument)).copied()? {
      super::shape::ShapeHint::PlainRecord => Some(argument),
      _ => None,
    }
  }

  fn call_info_at(&self, span: Span) -> Option<CallInfo> {
    self.indexes.note_query();
    if let Some(info) = self.indexes.calls.get(&span_key(span)).copied() {
      return Some(info);
    }
    let super::shape::ShapeHint::Call(call) = self.indexes.hints.get(&span_key(span)).copied()?
    else {
      return None;
    };
    self.indexes.calls.get(&span_key(call)).copied()
  }

  fn cloned_date_demand(
    &self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    path: &str,
    origin: DemandOrigin,
  ) -> Option<DateDemand> {
    if let Some(demand) = self.chained_cloned_demand(node_id, path, origin) {
      if self.path_repaired_before(None, call.span, path, origin, demand.offset) {
        return None;
      }
      return Some(demand);
    }
    for (symbol, key) in self.indexes.call_bindings(call.span) {
      self.indexes.note_query();
      if key != "cloned" {
        continue;
      }
      if self.indexes.reassigned.contains(symbol) || self.indexes.escaped.contains(symbol) {
        continue;
      }
      if let Some(demand) = self.path_method_demand(*symbol, &["value", path], origin) {
        if self.path_repaired_before(Some(*symbol), call.span, path, origin, demand.offset) {
          continue;
        }
        return Some(demand);
      }
      if let Some(demand) = self.alias_path_demand(*symbol, path, origin) {
        if self.path_repaired_before(Some(*symbol), call.span, path, origin, demand.offset) {
          continue;
        }
        return Some(demand);
      }
    }
    if let Some(bag) = self.indexes.result_of_call(call.span) {
      if self.indexes.reassigned.contains(&bag)
        || self.indexes.escaped.contains(&bag)
        || self.indexes.capability_touch.contains(&bag)
      {
        return None;
      }
      let demand = self.path_method_demand(bag, &["cloned", "value", path], origin)?;
      if self.path_repaired_before(Some(bag), call.span, path, origin, demand.offset) {
        return None;
      }
      return Some(demand);
    }
    None
  }

  fn chained_cloned_demand(
    &self,
    node_id: NodeId,
    path: &str,
    origin: DemandOrigin,
  ) -> Option<DateDemand> {
    let cloned = skip_ts_parent(self.semantic, node_id, self.indexes.work_counter());
    let AstKind::StaticMemberExpression(member) = self.semantic.nodes().kind(cloned) else {
      return None;
    };
    if member.property.name.as_str() != "cloned" {
      return None;
    }
    let value = skip_ts_parent(self.semantic, cloned, self.indexes.work_counter());
    let AstKind::StaticMemberExpression(member) = self.semantic.nodes().kind(value) else {
      return None;
    };
    if member.property.name.as_str() != "value" {
      return None;
    }
    let field = skip_ts_parent(self.semantic, value, self.indexes.work_counter());
    let AstKind::StaticMemberExpression(member) = self.semantic.nodes().kind(field) else {
      return None;
    };
    if member.property.name.as_str() != path {
      return None;
    }
    let method = skip_ts_parent(self.semantic, field, self.indexes.work_counter());
    let AstKind::StaticMemberExpression(member) = self.semantic.nodes().kind(method) else {
      return None;
    };
    let name = member.property.name.as_str();
    if !DATE_METHODS.contains(&name) {
      return None;
    }
    let (invoke, call) = enclosing_call(self.semantic, method, self.indexes.work_counter())?;
    let reach = classify_reach(self.semantic, invoke, self.indexes.work_counter());
    if !reach.is_straight() {
      return None;
    }
    let owner = self.indexes.owner(invoke);
    let demand = MemberUse {
      head: Site {
        offset: self.span(call.span).offset,
        span: call.span,
        callable: owner.callable,
        region: owner.region.unwrap_or(origin.region),
        reach,
      },
      optional: false,
      call_optional: false,
      role: super::proof::DemandRole::Read,
    };
    if !self.indexes.demand_from(&demand, origin) {
      return None;
    }
    Some(DateDemand { span: call.span, method: name.to_string(), offset: demand.head.offset })
  }

  fn path_method_demand(
    &self,
    root: SymbolId,
    keys: &[&str],
    origin: DemandOrigin,
  ) -> Option<DateDemand> {
    let mut chosen: Option<DateDemand> = None;
    for call in self.indexes.path_calls_on(root) {
      self.indexes.note_query();
      if call.keys.len() != keys.len()
        || !call.keys.iter().zip(keys).all(|(left, right)| left == right)
      {
        continue;
      }
      if !DATE_METHODS.contains(&call.method.as_str()) {
        continue;
      }
      if !self.indexes.demand_from(&call.site, origin) {
        continue;
      }
      if chosen.as_ref().is_none_or(|current| call.site.head.offset < current.offset) {
        chosen = Some(DateDemand {
          span: call.site.head.span,
          method: call.method.clone(),
          offset: call.site.head.offset,
        });
      }
    }
    chosen
  }

  fn alias_path_demand(
    &self,
    cloned: SymbolId,
    path: &str,
    origin: DemandOrigin,
  ) -> Option<DateDemand> {
    let mut chosen = None;
    for (alias, root) in &self.indexes.value_object_alias {
      self.indexes.note_query();
      if *root != cloned || self.indexes.reassigned.contains(alias) {
        continue;
      }
      if let Some(demand) = self.path_method_demand(*alias, &[path], origin)
        && chosen.as_ref().is_none_or(|current: &DateDemand| demand.offset < current.offset)
      {
        chosen = Some(demand);
      }
    }
    chosen
  }

  fn path_repaired_before(
    &self,
    root: Option<SymbolId>,
    call: Span,
    path: &str,
    origin: DemandOrigin,
    demand: usize,
  ) -> bool {
    if let Some(root) = root
      && self.root_value_repaired(root, origin, demand)
    {
      return true;
    }
    if let Some(root) = root {
      for (property, write) in self.indexes.nested_writes_on(root) {
        self.indexes.note_query();
        if property != path || !write.simple_assign {
          continue;
        }
        if write.callable != origin.callable || write.region != origin.region {
          continue;
        }
        if write.offset <= origin.offset || write.offset >= demand {
          continue;
        }
        if self.indexes.has_barrier_between(origin.region, origin.offset, write.offset) {
          continue;
        }
        if self.indexes.is_native_date(write.rhs) || self.indexes.literal_at(write.rhs).is_none() {
          return true;
        }
      }
      for (alias, aliased) in &self.indexes.value_object_alias {
        self.indexes.note_query();
        if *aliased != root {
          continue;
        }
        if self.indexes.reassigned.contains(alias) {
          return true;
        }
        for (property, write) in self.indexes.nested_writes_on(root) {
          self.indexes.note_query();
          if property == path
            && write.simple_assign
            && write.offset > origin.offset
            && write.offset < demand
            && write.callable == origin.callable
          {
            return true;
          }
        }
      }
    }
    if let Some(bag) = self.indexes.result_of_call(call) {
      if self.path_value_repaired(bag, &["cloned"], origin, demand) {
        return true;
      }
      if self.root_value_repaired(bag, origin, demand) {
        return true;
      }
    }
    false
  }

  fn root_value_repaired(&self, root: SymbolId, origin: DemandOrigin, demand: usize) -> bool {
    let work = self.indexes.work_counter();
    timeline::between(work, self.indexes.value_writes_on(root), origin.offset, demand).iter().any(
      |write| {
        self.indexes.note_query();
        write.callable == origin.callable
      },
    )
  }

  fn path_value_repaired(
    &self,
    root: SymbolId,
    keys: &[&str],
    origin: DemandOrigin,
    demand: usize,
  ) -> bool {
    self.indexes.path_value_writes_on(root).iter().any(|(path, write)| {
      self.indexes.note_query();
      path.len() == keys.len()
        && path.iter().zip(keys).all(|(left, right)| left == right)
        && write.simple_assign
        && write.callable == origin.callable
        && write.region == origin.region
        && write.offset > origin.offset
        && write.offset < demand
    })
  }

  fn history_source(&self, argument: Span) -> Option<(SymbolId, Span)> {
    let hint = self.indexes.hints.get(&span_key(argument)).copied()?;
    let super::shape::ShapeHint::Identifier(Some(symbol_id), false) = hint else {
      return None;
    };
    let root = self.indexes.root_of(symbol_id);
    if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      return None;
    }
    if self.indexes.reassigned.contains(&root) {
      return None;
    }
    let init = self.indexes.init_span.get(&root).copied()?;
    let object = self.ref_call_object(init)?;
    Some((root, object))
  }

  fn history_bindings(&self, call: Span) -> HistoryBindings {
    let mut bindings =
      HistoryBindings { bag: self.indexes.result_of_call(call), ..HistoryBindings::default() };
    for (symbol, key) in self.indexes.call_bindings(call) {
      self.indexes.note_query();
      match key.as_str() {
        "commit" => bindings.commit = Some(*symbol),
        "undo" => bindings.undo = Some(*symbol),
        "redo" => bindings.redo = Some(*symbol),
        "reset" => bindings.reset = Some(*symbol),
        "clear" => bindings.clear = Some(*symbol),
        "history" => bindings.history = Some(*symbol),
        "last" => bindings.last = Some(*symbol),
        _ => {}
      }
    }
    bindings
  }

  fn history_summary(
    &mut self,
    source: SymbolId,
    object: Span,
    bindings: &HistoryBindings,
    origin: DemandOrigin,
    capacity: i64,
  ) -> Option<HistorySummary> {
    let key = (u64::try_from(source.index()).unwrap_or(u64::MAX) << 32)
      | u64::try_from(origin.offset).unwrap_or(u64::MAX);
    if let Some(hit) = self.snapshot_memo.histories.get(&key) {
      self.indexes.note_query();
      return hit.clone();
    }
    let built = self.build_history_summary(source, object, bindings, origin, capacity);
    self.snapshot_memo.histories.insert(key, built.clone());
    built
  }

  fn build_history_summary(
    &self,
    source: SymbolId,
    object: Span,
    bindings: &HistoryBindings,
    origin: DemandOrigin,
    capacity: i64,
  ) -> Option<HistorySummary> {
    let mut ops = Vec::new();
    self.collect_ops(bindings, origin, HistoryOpKind::Commit, "commit", &mut ops);
    self.collect_ops(bindings, origin, HistoryOpKind::Undo, "undo", &mut ops);
    self.collect_ops(bindings, origin, HistoryOpKind::Redo, "redo", &mut ops);
    self.collect_ops(bindings, origin, HistoryOpKind::Reset, "reset", &mut ops);
    self.collect_ops(bindings, origin, HistoryOpKind::Clear, "clear", &mut ops);
    ops.sort_by(|left, right| {
      self.indexes.note_query();
      left.offset.cmp(&right.offset)
    });
    let mut props = self.object_literals(object)?;
    let mut generation = 0_u32;
    let mut unknown = false;
    for write in self.indexes.value_writes.get(&source).map_or(&[][..], Vec::as_slice) {
      self.indexes.note_query();
      if write.callable != origin.callable || write.offset >= origin.offset {
        continue;
      }
      generation = generation.saturating_add(1);
      props.clear();
      unknown = true;
    }
    for (property, write) in self.indexes.nested_writes_on(source) {
      self.indexes.note_query();
      if !self.indexes.nested_demand(write, origin)
        && write.offset < origin.offset
        && write.callable == origin.callable
        && write.region == origin.region
      {
        props.insert(property.clone(), self.indexes.literal_at(write.rhs));
      }
    }
    let mut last = RecordState { dump_offset: origin.offset, generation, baseline: props.clone() };
    let mut undo: Vec<RecordState> = Vec::new();
    let mut redo: Vec<RecordState> = Vec::new();
    let mut writes = Vec::new();
    let mut points =
      vec![HistoryPoint { offset: origin.offset, last: last.clone(), undo: Vec::new() }];
    let mut events = Vec::new();
    for op in &ops {
      events.push((op.offset, HistoryEvent::Op(*op)));
    }
    for (property, write) in self.indexes.nested_writes_on(source) {
      self.indexes.note_query();
      if write.offset <= origin.offset || write.callable != origin.callable {
        continue;
      }
      events.push((write.offset, HistoryEvent::Write(property.clone(), *write)));
    }
    for write in self.indexes.value_writes.get(&source).map_or(&[][..], Vec::as_slice) {
      self.indexes.note_query();
      if write.offset <= origin.offset || write.callable != origin.callable {
        continue;
      }
      events.push((write.offset, HistoryEvent::Root));
    }
    events.sort_by(|left, right| {
      self.indexes.note_query();
      left.0.cmp(&right.0)
    });
    for (_, event) in events {
      self.indexes.note_query();
      match event {
        HistoryEvent::Write(property, write) => {
          self.indexes.note_query();
          props.insert(property.clone(), self.indexes.literal_at(write.rhs));
          writes.push((property, WriteEvent { write, generation }));
          self.indexes.note_query();
          points.push(HistoryPoint {
            offset: write.offset,
            last: last.clone(),
            undo: undo.clone(),
          });
        }
        HistoryEvent::Root => {
          generation = generation.saturating_add(1);
          props.clear();
          unknown = true;
        }
        HistoryEvent::Op(op) => {
          match op.kind {
            HistoryOpKind::Commit => {
              self.indexes.note_query();
              undo.insert(0, last.clone());
              last = RecordState { dump_offset: op.offset, generation, baseline: props.clone() };
              if capacity >= 1 {
                let limit = usize::try_from(capacity).unwrap_or(usize::MAX);
                if undo.len() > limit {
                  undo.truncate(limit);
                }
              }
              redo.clear();
            }
            HistoryOpKind::Undo => {
              if let Some(record) = undo.first().cloned() {
                undo.remove(0);
                redo.insert(0, last.clone());
                last = record;
                generation = last.generation;
                props.clone_from(&last.baseline);
              }
            }
            HistoryOpKind::Redo => {
              if let Some(record) = redo.first().cloned() {
                redo.remove(0);
                undo.insert(0, last.clone());
                last = record;
                generation = last.generation;
                props.clone_from(&last.baseline);
              }
            }
            HistoryOpKind::Reset => {
              generation = last.generation;
              props.clone_from(&last.baseline);
            }
            HistoryOpKind::Clear => {
              undo.clear();
              redo.clear();
            }
          }
          points.push(HistoryPoint { offset: op.offset, last: last.clone(), undo: undo.clone() });
        }
      }
    }
    Some(HistorySummary { writes, points, unknown })
  }

  fn collect_ops(
    &self,
    bindings: &HistoryBindings,
    origin: DemandOrigin,
    kind: HistoryOpKind,
    key: &str,
    ops: &mut Vec<HistoryOp>,
  ) {
    let local = match kind {
      HistoryOpKind::Commit => bindings.commit,
      HistoryOpKind::Undo => bindings.undo,
      HistoryOpKind::Redo => bindings.redo,
      HistoryOpKind::Reset => bindings.reset,
      HistoryOpKind::Clear => bindings.clear,
    };
    self.each_method_site(local, bindings.bag, key, |site| {
      if self.indexes.ident_demand(site, origin) && site.head.offset > origin.offset {
        ops.push(HistoryOp { offset: site.head.offset, kind });
      }
    });
  }

  fn each_method_site(
    &self,
    local: Option<SymbolId>,
    bag: Option<SymbolId>,
    key: &str,
    mut visit: impl FnMut(&SnapshotCall),
  ) {
    if let Some(symbol) = local {
      if self.indexes.reassigned.contains(&symbol) {
        return;
      }
      for call in self.indexes.identifier_calls_on(symbol) {
        self.indexes.note_query();
        let site = SnapshotCall::from_call_use(*call);
        visit(&site);
      }
    }
    if let Some(bag) = bag {
      for named in self.indexes.member_calls_on(bag) {
        self.indexes.note_query();
        if named.key == key {
          let site = SnapshotCall {
            head: Site {
              offset: named.site.head.offset,
              span: named.site.head.span,
              callable: named.site.head.callable,
              region: named.site.head.region,
              reach: named.site.head.reach,
            },
            optional: named.site.optional,
          };
          visit(&site);
        }
      }
    }
  }

  fn object_literals(&self, object: Span) -> Option<HashMap<String, Option<Literal>>> {
    let entries = self.indexes.object_index.objects.get(&span_key(object))?;
    let mut props = HashMap::new();
    for entry in entries {
      self.indexes.note_query();
      let super::index::ObjectEntry::Data { name, value, .. } = entry else {
        return None;
      };
      props.insert(name.clone(), self.indexes.literal_at(*value));
    }
    Some(props)
  }

  fn history_value_demands(
    &self,
    bindings: &HistoryBindings,
    origin: DemandOrigin,
    source: SymbolId,
  ) -> Vec<HistoryDemand> {
    let mut demands = Vec::new();
    self.each_method_site(bindings.undo, bindings.bag, "undo", |site| {
      if self.indexes.ident_demand(site, origin)
        && self.source_property_consumed(source, origin, site.head.offset)
      {
        demands.push(HistoryDemand { site: *site, kind: "undo", restore: Restore::Undo });
      }
    });
    self.each_method_site(bindings.reset, bindings.bag, "reset", |site| {
      if self.indexes.ident_demand(site, origin)
        && self.source_property_consumed(source, origin, site.head.offset)
      {
        demands.push(HistoryDemand { site: *site, kind: "reset", restore: Restore::Reset });
      }
    });
    for read in self.snapshot_property_reads(bindings, origin) {
      demands.push(HistoryDemand {
        site: read.0,
        kind: "history",
        restore: Restore::Snapshot(read.1),
      });
    }
    demands
  }

  fn source_property_consumed(&self, source: SymbolId, origin: DemandOrigin, after: usize) -> bool {
    for read in self.indexes.path_reads_on(source) {
      self.indexes.note_query();
      if read.keys.first().is_some_and(|key| key == "value")
        && read.keys.len() >= 2
        && read.site.head.offset > after
        && self.indexes.demand_from(&read.site, origin)
      {
        return true;
      }
    }
    false
  }

  fn snapshot_property_reads(
    &self,
    bindings: &HistoryBindings,
    origin: DemandOrigin,
  ) -> Vec<(SnapshotCall, Option<usize>)> {
    let mut reads = Vec::new();
    for symbol in [bindings.history, bindings.last].into_iter().flatten() {
      if self.indexes.reassigned.contains(&symbol) {
        continue;
      }
      for read in self.indexes.path_reads_on(symbol) {
        self.indexes.note_query();
        if !self.indexes.demand_from(&read.site, origin) {
          continue;
        }
        if !path_reaches_snapshot(&read.keys) {
          continue;
        }
        reads.push((
          SnapshotCall {
            head: Site {
              offset: read.site.head.offset,
              span: read.site.head.span,
              callable: read.site.head.callable,
              region: read.site.head.region,
              reach: read.site.head.reach,
            },
            optional: read.site.optional,
          },
          snapshot_index(&read.keys),
        ));
      }
    }
    if let Some(bag) = bindings.bag {
      for read in self.indexes.path_reads_on(bag) {
        self.indexes.note_query();
        if !self.indexes.demand_from(&read.site, origin) {
          continue;
        }
        let Some(rest) = read
          .keys
          .split_first()
          .and_then(|(head, rest)| matches!(head.as_str(), "history" | "last").then_some(rest))
        else {
          continue;
        };
        if !path_reaches_snapshot(rest) {
          continue;
        }
        reads.push((
          SnapshotCall {
            head: Site {
              offset: read.site.head.offset,
              span: read.site.head.span,
              callable: read.site.head.callable,
              region: read.site.head.region,
              reach: read.site.head.reach,
            },
            optional: read.site.optional,
          },
          snapshot_index(rest),
        ));
      }
    }
    reads
  }

  fn consumed_record(
    &self,
    summary: &HistorySummary,
    demand: &HistoryDemand,
  ) -> Option<RecordState> {
    let point = self.point_before(summary, demand.site.head.offset)?;
    match demand.restore {
      Restore::Undo => point.undo.first().cloned(),
      Restore::Reset => Some(point.last.clone()),
      Restore::Snapshot(index) => {
        let mut records = Vec::with_capacity(point.undo.len().saturating_add(1));
        records.push(point.last.clone());
        records.extend(point.undo.iter().cloned());
        index.map_or_else(|| records.first().cloned(), |index| records.get(index).cloned())
      }
    }
  }

  fn point_before<'a>(
    &self,
    summary: &'a HistorySummary,
    offset: usize,
  ) -> Option<&'a HistoryPoint> {
    let index = summary.points.iter().rposition(|point| {
      self.indexes.note_query();
      point.offset < offset
    })?;
    summary.points.get(index)
  }

  fn corrupting_write(
    &self,
    summary: &HistorySummary,
    record: &RecordState,
    demand: usize,
  ) -> Option<(NestedWrite, String)> {
    let mut last_by_prop: HashMap<String, (NestedWrite, Option<Literal>)> = HashMap::new();
    for (property, event) in &summary.writes {
      self.indexes.note_query();
      if event.generation != record.generation
        || event.write.offset <= record.dump_offset
        || event.write.offset >= demand
        || !event.write.simple_assign
      {
        continue;
      }
      last_by_prop
        .insert(property.clone(), (event.write, self.indexes.literal_at(event.write.rhs)));
    }
    let mut chosen = None;
    for (property, (write, rhs)) in last_by_prop {
      self.indexes.note_query();
      let rhs = rhs?;
      let baseline = record.baseline.get(&property).copied().flatten();
      if !literals_differ(baseline, Some(rhs)) {
        continue;
      }
      if chosen
        .as_ref()
        .is_none_or(|(current, _): &(NestedWrite, String)| write.offset < current.offset)
      {
        chosen = Some((write, property));
      }
    }
    chosen
  }
}

#[derive(Clone)]
struct DateDemand {
  span: Span,
  method: String,
  offset: usize,
}

#[derive(Clone, Copy)]
enum Restore {
  Undo,
  Reset,
  Snapshot(Option<usize>),
}

#[derive(Clone, Copy)]
struct HistoryDemand {
  site: SnapshotCall,
  kind: &'static str,
  restore: Restore,
}

enum HistoryEvent {
  Op(HistoryOp),
  Write(String, NestedWrite),
  Root,
}

const fn literals_differ(left: Option<Literal>, right: Option<Literal>) -> bool {
  match (left, right) {
    (Some(Literal::Bool(left)), Some(Literal::Bool(right))) => left != right,
    (Some(Literal::Number(left)), Some(Literal::Number(right))) => left != right,
    _ => false,
  }
}

#[derive(Default)]
struct HistoryBindings {
  bag: Option<SymbolId>,
  commit: Option<SymbolId>,
  undo: Option<SymbolId>,
  redo: Option<SymbolId>,
  reset: Option<SymbolId>,
  clear: Option<SymbolId>,
  history: Option<SymbolId>,
  last: Option<SymbolId>,
}

fn path_reaches_snapshot(keys: &[String]) -> bool {
  let Some(value) = keys.first() else {
    return false;
  };
  if value != "value" {
    return false;
  }
  keys.iter().any(|key| key == "snapshot") && keys.last().is_some_and(|key| key != "snapshot")
}

fn snapshot_index(keys: &[String]) -> Option<usize> {
  keys.get(1).and_then(|key| key.parse().ok())
}
