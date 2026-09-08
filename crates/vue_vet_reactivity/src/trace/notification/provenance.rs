//! Closed source/view identity and effective payload shape.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression, ObjectPropertyKind, PropertyKind, VariableDeclarationKind},
};
use oxc_semantic::{NodeId, Semantic, SymbolId};
use vue_vet_core::{
  ReactiveBindingKind, ReactiveSourceViewFact, ReactiveViewKind, ScriptKind, SourceSpan,
};

use super::super::kinds::{resolved_vue_callee, source_span};
use super::paths::{peel, static_key_name};
use super::uses::{
  OwnerIndex, UseRole, WorkCounter, binding_symbol_at, enclosing_region, identifier_symbol,
};

pub(super) const ALIAS_BUDGET: u32 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PayloadBranch {
  Plain,
  Wrapped,
  Unknown,
}

#[derive(Debug)]
pub(super) struct SourceRecord {
  pub symbol_id: SymbolId,
  pub name: String,
  pub binding_span: SourceSpan,
  pub creation_span: SourceSpan,
  pub source_kind: ReactiveBindingKind,
  pub view: ReactiveViewKind,
  pub path: Vec<String>,
  pub raw_conversion_span: Option<SourceSpan>,
  pub region: NodeId,
  pub canonical: usize,
  pub payload: BTreeMap<Vec<String>, PayloadBranch>,
  pub valid: bool,
}

impl SourceRecord {
  pub(super) fn to_fact(&self) -> ReactiveSourceViewFact {
    ReactiveSourceViewFact {
      name: self.name.clone(),
      binding_span: self.binding_span,
      creation_span: self.creation_span,
      source_kind: self.source_kind,
      view: self.view,
      path: self.path.clone(),
      raw_conversion_span: self.raw_conversion_span,
    }
  }
}

#[derive(Clone, Copy)]
enum AliasOutcome {
  Hit(usize),
  Uncertain,
}

pub(super) struct ProvenanceIndex {
  pub records: Vec<SourceRecord>,
  record_by_symbol: BTreeMap<SymbolId, usize>,
  by_symbol: BTreeMap<(SymbolId, u32), AliasOutcome>,
}

impl ProvenanceIndex {
  pub(super) fn record(&self, index: usize) -> Option<&SourceRecord> {
    self.records.get(index)
  }

  pub(super) fn is_effectively_valid(&self, index: usize) -> bool {
    let Some(record) = self.records.get(index) else {
      return false;
    };
    self.records.get(record.canonical).is_some_and(|canonical| canonical.valid)
  }

  pub(super) fn canonical_payload(
    &self,
    index: usize,
  ) -> Option<&BTreeMap<Vec<String>, PayloadBranch>> {
    let canonical = self.records.get(index)?.canonical;
    self.records.get(canonical).map(|record| &record.payload)
  }

  fn lookup_record(&self, symbol_id: SymbolId, work: &WorkCounter) -> Option<usize> {
    work.add_lookups(1);
    self.record_by_symbol.get(&symbol_id).copied()
  }

  fn push_record(&mut self, record: SourceRecord, work: &WorkCounter) {
    let symbol_id = record.symbol_id;
    let index = self.records.len();
    self.records.push(record);
    work.add_lookups(1);
    self.record_by_symbol.insert(symbol_id, index);
  }

  pub(super) fn invalidate_symbol(&mut self, symbol_id: SymbolId, work: &WorkCounter) {
    let Some(index) = self.lookup_record(symbol_id, work) else {
      return;
    };
    let Some(canonical) = self.records.get(index).map(|record| record.canonical) else {
      return;
    };
    let Some(record) = self.records.get_mut(canonical) else {
      return;
    };
    if !record.valid {
      return;
    }
    record.valid = false;
  }

  pub(super) fn replace_payload_path(
    &mut self,
    symbol_id: SymbolId,
    path: &[String],
    work: &WorkCounter,
  ) {
    let Some(index) = self.lookup_record(symbol_id, work) else {
      return;
    };
    let Some(canonical) = self.records.get(index).map(|record| record.canonical) else {
      return;
    };
    let Some(kind) = self.records.get(canonical).map(|record| record.source_kind) else {
      return;
    };
    let payload_path = payload_relative(kind, path);
    let Some(payload) = self.records.get_mut(canonical).map(|record| &mut record.payload) else {
      return;
    };
    wipe_prefix(payload, &payload_path, work);
    payload.insert(payload_path, PayloadBranch::Unknown);
  }
}

pub(super) fn collect_provenance(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  owners: &OwnerIndex,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  work: &WorkCounter,
) -> ProvenanceIndex {
  let mut records = Vec::new();
  for node in semantic.nodes() {
    work.add_node_visits(1);
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let parent_id = semantic.nodes().parent_id(declarator.node_id.get());
    let AstKind::VariableDeclaration(declaration) = semantic.nodes().kind(parent_id) else {
      continue;
    };
    if declaration.kind != VariableDeclarationKind::Const {
      continue;
    }
    let Some(init) = &declarator.init else {
      continue;
    };
    let Expression::CallExpression(call) = peel(init) else {
      continue;
    };
    let Some(callee) = resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind)
    else {
      continue;
    };
    let Some(decl_symbol) = binding_symbol_at(owners, identifier.span, work) else {
      continue;
    };
    let region = enclosing_region(semantic, declarator.node_id.get(), work);
    let binding_span = source_span(sfc_source, script_offset, identifier.span);
    let creation_span = source_span(sfc_source, script_offset, call.span);
    match callee.as_str() {
      "shallowRef" | "shallowReactive" => {
        let payload = call
          .arguments
          .first()
          .and_then(oxc_ast::ast::Argument::as_expression)
          .map_or_else(unknown_payload, |expression| {
            classify_payload(semantic, imported_bindings, script_kind, expression, work)
          });
        work.add_payload_entries(payload.len());
        let kind = if callee == "shallowRef" {
          ReactiveBindingKind::ShallowRef
        } else {
          ReactiveBindingKind::ShallowReactive
        };
        let canonical = records.len();
        records.push(SourceRecord {
          symbol_id: decl_symbol,
          name: identifier.name.to_string(),
          binding_span,
          creation_span,
          source_kind: kind,
          view: ReactiveViewKind::ShallowContainer,
          path: Vec::new(),
          raw_conversion_span: None,
          region,
          canonical,
          payload,
          valid: true,
        });
      }
      "reactive" => {
        let Some(argument) = call.arguments.first().and_then(oxc_ast::ast::Argument::as_expression)
        else {
          continue;
        };
        if !proven_plain_proxy_target(argument) {
          continue;
        }
        let payload = classify_payload(semantic, imported_bindings, script_kind, argument, work);
        if payload.get(&Vec::new()) != Some(&PayloadBranch::Plain) {
          continue;
        }
        work.add_payload_entries(payload.len());
        let canonical = records.len();
        records.push(SourceRecord {
          symbol_id: decl_symbol,
          name: identifier.name.to_string(),
          binding_span,
          creation_span,
          source_kind: ReactiveBindingKind::Reactive,
          view: ReactiveViewKind::Proxy,
          path: Vec::new(),
          raw_conversion_span: None,
          region,
          canonical,
          payload,
          valid: true,
        });
      }
      _ => {}
    }
  }

  let mut record_by_symbol = BTreeMap::new();
  for (idx, record) in records.iter().enumerate() {
    work.add_lookups(1);
    record_by_symbol.insert(record.symbol_id, idx);
  }
  let mut index = ProvenanceIndex { records, record_by_symbol, by_symbol: BTreeMap::new() };
  collect_raw_and_aliases(
    semantic,
    imported_bindings,
    owners,
    sfc_source,
    script_offset,
    script_kind,
    &mut index,
    work,
  );
  apply_use_invalidation(&mut index, owners, work);
  index
}

#[expect(clippy::too_many_arguments, reason = "alias collection reuses owner/provenance indexes")]
fn collect_raw_and_aliases(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  owners: &OwnerIndex,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  index: &mut ProvenanceIndex,
  work: &WorkCounter,
) {
  for node in semantic.nodes() {
    work.add_node_visits(1);
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let parent_id = semantic.nodes().parent_id(declarator.node_id.get());
    let AstKind::VariableDeclaration(declaration) = semantic.nodes().kind(parent_id) else {
      continue;
    };
    if declaration.kind != VariableDeclarationKind::Const {
      continue;
    }
    let Some(decl_symbol) = binding_symbol_at(owners, identifier.span, work) else {
      continue;
    };
    if index.lookup_record(decl_symbol, work).is_some() {
      continue;
    }
    let Some(init) = &declarator.init else {
      continue;
    };
    let region = enclosing_region(semantic, declarator.node_id.get(), work);
    if let Expression::CallExpression(call) = peel(init)
      && resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind).as_deref()
        == Some("toRaw")
    {
      let Some(argument) = call.arguments.first().and_then(oxc_ast::ast::Argument::as_expression)
      else {
        continue;
      };
      let Some(root) = peel(argument).get_identifier_reference() else {
        continue;
      };
      let Some(root_symbol) = identifier_symbol(semantic, root) else {
        continue;
      };
      let Some(source_idx) = index.lookup_record(root_symbol, work) else {
        continue;
      };
      let Some(source) = index.records.get(source_idx) else {
        continue;
      };
      if source.view != ReactiveViewKind::Proxy
        || source.source_kind != ReactiveBindingKind::Reactive
        || !index.is_effectively_valid(source_idx)
      {
        continue;
      }
      work.add_aliases(1);
      let creation_span = source.creation_span;
      let source_kind = source.source_kind;
      let canonical = source.canonical;
      let valid = source.valid;
      index.push_record(
        SourceRecord {
          symbol_id: decl_symbol,
          name: identifier.name.to_string(),
          binding_span: source_span(sfc_source, script_offset, identifier.span),
          creation_span,
          source_kind,
          view: ReactiveViewKind::Raw,
          path: Vec::new(),
          raw_conversion_span: Some(source_span(sfc_source, script_offset, call.span)),
          region,
          canonical,
          payload: BTreeMap::new(),
          valid,
        },
        work,
      );
      continue;
    }
    if let Some(alias_of) = peel(init).get_identifier_reference() {
      let Some(alias_symbol) = identifier_symbol(semantic, alias_of) else {
        continue;
      };
      let Some(source_idx) = index.lookup_record(alias_symbol, work) else {
        continue;
      };
      let Some(source) = index.records.get(source_idx) else {
        continue;
      };
      if !index.is_effectively_valid(source_idx) {
        continue;
      }
      work.add_aliases(1);
      if !source.path.is_empty() {
        work.add_copies(source.path.len());
      }
      let alias = SourceRecord {
        symbol_id: decl_symbol,
        name: identifier.name.to_string(),
        binding_span: source_span(sfc_source, script_offset, identifier.span),
        creation_span: source.creation_span,
        source_kind: source.source_kind,
        view: source.view,
        path: source.path.clone(),
        raw_conversion_span: source.raw_conversion_span,
        region,
        canonical: source.canonical,
        payload: BTreeMap::new(),
        valid: source.valid,
      };
      index.push_record(alias, work);
    }
  }
}

fn apply_use_invalidation(index: &mut ProvenanceIndex, owners: &OwnerIndex, work: &WorkCounter) {
  let watched: BTreeSet<SymbolId> = index.record_by_symbol.keys().copied().collect();
  let mut invalid = BTreeSet::new();
  let mut replacements = Vec::new();
  for symbol_id in &watched {
    work.add_lookups(1);
    if owners.exported.contains(symbol_id) {
      invalid.insert(*symbol_id);
    }
    work.add_lookups(1);
    let Some(sites) = owners.by_symbol.get(symbol_id) else {
      continue;
    };
    work.add_use_sites(sites.len());
    for site in sites {
      work.add_candidate_visits(1);
      match &site.role {
        UseRole::Invalid => {
          invalid.insert(*symbol_id);
        }
        UseRole::PayloadReplace { path } => {
          work.add_copies(path.len().max(1));
          replacements.push((*symbol_id, path.clone()));
        }
        UseRole::ClosedAlias
        | UseRole::VueApiArg
        | UseRole::Read
        | UseRole::NestedWrite
        | UseRole::HandleStop => {}
      }
    }
  }
  for symbol_id in invalid {
    index.invalidate_symbol(symbol_id, work);
  }
  for (symbol_id, path) in replacements {
    index.replace_payload_path(symbol_id, &path, work);
  }
}

pub(super) fn resolve_view(
  index: &mut ProvenanceIndex,
  symbol_id: SymbolId,
  budget: u32,
  work: &WorkCounter,
) -> Option<usize> {
  if budget == 0 {
    return None;
  }
  work.add_lookups(1);
  if let Some(outcome) = index.by_symbol.get(&(symbol_id, budget)) {
    return match *outcome {
      AliasOutcome::Hit(idx) => Some(idx),
      AliasOutcome::Uncertain => None,
    };
  }
  let found = index.lookup_record(symbol_id, work);
  let outcome = found.map_or(AliasOutcome::Uncertain, AliasOutcome::Hit);
  work.add_lookups(1);
  index.by_symbol.insert((symbol_id, budget), outcome);
  found
}

fn object_has_internal_marker(object: &oxc_ast::ast::ObjectExpression<'_>) -> bool {
  object.properties.iter().any(|property| {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
      return false;
    };
    static_key_name(&property.key).is_some_and(|key| {
      matches!(key.as_str(), "__v_skip" | "__v_isReadonly" | "__v_isRef" | "__v_raw" | "__proto__")
    })
  })
}

/// Root `reactive({...})` only creates a tracked proxy when the target is a
/// proven-plain object without Vue internal flags or a custom prototype.
fn proven_plain_proxy_target(expression: &Expression<'_>) -> bool {
  let Expression::ObjectExpression(object) = peel(expression) else {
    return false;
  };
  if object_has_internal_marker(object) {
    return false;
  }
  object.properties.iter().all(|property| {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
      return false;
    };
    property.kind == PropertyKind::Init
      && !property.method
      && static_key_name(&property.key).is_some()
  })
}

fn classify_payload(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  expression: &Expression<'_>,
  work: &WorkCounter,
) -> BTreeMap<Vec<String>, PayloadBranch> {
  let mut map = BTreeMap::new();
  classify_payload_into(
    semantic,
    imported_bindings,
    script_kind,
    expression,
    Vec::new(),
    &mut map,
    0,
    work,
  );
  map
}

#[expect(clippy::too_many_arguments, reason = "recursive payload walk carries shared indexes")]
fn classify_payload_into(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  expression: &Expression<'_>,
  path: Vec<String>,
  map: &mut BTreeMap<Vec<String>, PayloadBranch>,
  depth: u32,
  work: &WorkCounter,
) {
  work.add_payload_entries(1);
  if depth > ALIAS_BUDGET {
    wipe_prefix(map, &path, work);
    map.insert(path, PayloadBranch::Unknown);
    return;
  }
  match peel(expression) {
    Expression::ObjectExpression(object) => {
      if object_has_internal_marker(object) {
        wipe_prefix(map, &path, work);
        map.insert(path, PayloadBranch::Unknown);
        return;
      }
      let mut branch = PayloadBranch::Plain;
      for property in &object.properties {
        match property {
          ObjectPropertyKind::SpreadProperty(_) => {
            wipe_prefix(map, &path, work);
            branch = PayloadBranch::Unknown;
          }
          ObjectPropertyKind::ObjectProperty(property) => {
            if property.kind != PropertyKind::Init || property.method {
              wipe_prefix(map, &path, work);
              branch = PayloadBranch::Unknown;
              continue;
            }
            let Some(key) = static_key_name(&property.key) else {
              wipe_prefix(map, &path, work);
              branch = PayloadBranch::Unknown;
              continue;
            };
            work.add_copies(path.len().saturating_add(1));
            let mut child = path.clone();
            child.push(key);
            wipe_prefix(map, &child, work);
            classify_payload_into(
              semantic,
              imported_bindings,
              script_kind,
              &property.value,
              child,
              map,
              depth.saturating_add(1),
              work,
            );
          }
        }
      }
      map.entry(path).or_insert(branch);
    }
    Expression::CallExpression(call) => {
      if let Some(callee) =
        resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind)
        && matches!(
          callee.as_str(),
          "reactive"
            | "ref"
            | "shallowRef"
            | "shallowReactive"
            | "readonly"
            | "shallowReadonly"
            | "markRaw"
            | "computed"
        )
      {
        wipe_prefix(map, &path, work);
        map.insert(path, PayloadBranch::Wrapped);
        return;
      }
      wipe_prefix(map, &path, work);
      map.insert(path, PayloadBranch::Unknown);
    }
    Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::TemplateLiteral(_)
    | Expression::UnaryExpression(_) => {
      map.insert(path, PayloadBranch::Plain);
    }
    _ => {
      wipe_prefix(map, &path, work);
      map.insert(path, PayloadBranch::Unknown);
    }
  }
}

fn wipe_prefix(
  map: &mut BTreeMap<Vec<String>, PayloadBranch>,
  prefix: &[String],
  work: &WorkCounter,
) {
  if prefix.is_empty() {
    work.add_payload_prefix_removals(map.len());
    map.clear();
    return;
  }
  work.add_lookups(1);
  let start = prefix.to_vec();
  let mut keys = Vec::new();
  for (key, _) in map.range(start..) {
    work.add_payload_prefix_visits(1);
    if !key.starts_with(prefix) {
      break;
    }
    work.add_copies(key.len().max(1));
    keys.push(key.clone());
  }
  for key in keys {
    work.add_lookups(1);
    work.add_payload_prefix_removals(1);
    map.remove(&key);
  }
}

fn payload_relative(kind: ReactiveBindingKind, path: &[String]) -> Vec<String> {
  if kind == ReactiveBindingKind::ShallowRef {
    path.get(1..).unwrap_or(&[]).to_vec()
  } else {
    path.to_vec()
  }
}

pub(super) fn payload_path_has_wrapped(
  payload: &BTreeMap<Vec<String>, PayloadBranch>,
  path: &[String],
  work: &WorkCounter,
) -> bool {
  let mut prefix = Vec::new();
  for segment in path {
    work.add_candidate_visits(1);
    prefix.push(segment.clone());
    work.add_lookups(1);
    if payload.get(&prefix) == Some(&PayloadBranch::Wrapped) {
      return true;
    }
  }
  false
}

pub(super) fn payload_allows_nested_write(
  payload: &BTreeMap<Vec<String>, PayloadBranch>,
  write_path: &[String],
  kind: ReactiveBindingKind,
  work: &WorkCounter,
) -> bool {
  let inner = payload_relative(kind, write_path);
  if inner.is_empty() {
    return false;
  }
  let mut prefix = Vec::new();
  work.add_lookups(1);
  if payload.get(&prefix) != Some(&PayloadBranch::Plain) {
    return false;
  }
  for segment in inner.iter().take(inner.len().saturating_sub(1)) {
    work.add_candidate_visits(1);
    prefix.push(segment.clone());
    work.add_lookups(1);
    if payload.get(&prefix) != Some(&PayloadBranch::Plain) {
      return false;
    }
  }
  true
}

fn unknown_payload() -> BTreeMap<Vec<String>, PayloadBranch> {
  let mut map = BTreeMap::new();
  map.insert(Vec::new(), PayloadBranch::Unknown);
  map
}
