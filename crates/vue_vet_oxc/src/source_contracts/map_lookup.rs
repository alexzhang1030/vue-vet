//! Native Map raw/proxy key identity and keyed `forEach` selection facts.

use std::collections::HashMap;

use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentOperator, AssignmentTarget, BinaryOperator, BindingPattern, CallExpression,
    Expression, FormalParameters, FunctionBody, LogicalOperator, Statement,
    VariableDeclarationKind,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{KeyedMapDependencyFact, RawProxyMapKeyFact};

use super::index::{
  CtorKeyState, ExecKind, MapKeyRef, MapOp, MemberCallSite, ProxyFlavor, proxy_flavor,
};
use super::proof::{enclosing_call, skip_ts_parent};
use super::shape::{Shape, ShapeHint, span_key};
use super::stats::WorkCounter;
use super::{Collector, MAX_DEPTH};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum KeyId {
  Raw(SymbolId),
  Proxy { raw: SymbolId, flavor: ProxyFlavor },
  Primitive,
  Unknown,
}

impl KeyId {
  const fn raw_of(self) -> Option<SymbolId> {
    match self {
      Self::Raw(raw) | Self::Proxy { raw, .. } => Some(raw),
      Self::Primitive | Self::Unknown => None,
    }
  }
}

#[derive(Clone, Copy, Debug)]
struct ClassifiedKey {
  id: KeyId,
  span: Span,
  wrapper: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
struct EntryProv {
  span: Span,
  wrapper: Option<Span>,
}

impl Collector<'_> {
  pub(super) fn collect_all_raw_proxy_map_gets(&mut self) {
    if self.indexes.map_intrinsic_poisoned {
      return;
    }
    let roots = self.indexes.map_get_roots();
    for root in roots {
      self.replay_map_gets(root);
    }
  }

  fn replay_map_gets(&mut self, root: SymbolId) {
    let Some(map_root) = self.native_map_root(root) else {
      return;
    };
    let Some(mut entries) = self.constructor_state(map_root) else {
      return;
    };
    let map_owner = self.indexes.declaration_owner(self.semantic, map_root);
    let Some(map_origin) = self.indexes.init_mapped_offset(map_root) else {
      return;
    };
    let get_count = self.indexes.map_get_count(root);
    let mutation_count = self.indexes.map_mutation_count(root);
    let mut op_index = 0;
    let mut unknown = false;
    for get_index in 0..get_count {
      let Some(site) = self.indexes.map_get_at(root, get_index) else {
        break;
      };
      while op_index < mutation_count {
        let Some(op) = self.indexes.map_mutation_at(root, op_index) else {
          break;
        };
        if op.site.offset >= site.offset {
          break;
        }
        self.indexes.note_mutation();
        op_index += 1;
        if unknown {
          continue;
        }
        match self.apply_mutation(map_root, &mut entries, op, site) {
          ApplyResult::Keep => {}
          ApplyResult::Unknown => unknown = true,
        }
      }
      if unknown {
        continue;
      }
      self.emit_get_if_mismatch(map_root, map_owner, map_origin, site, &entries);
    }
  }

  fn emit_get_if_mismatch(
    &mut self,
    map_root: SymbolId,
    map_owner: super::index::Owner,
    map_origin: usize,
    site: MemberCallSite,
    entries: &HashMap<KeyId, EntryProv>,
  ) {
    if site.optional || site.has_spread || site.arg_count != 1 || site.exec != ExecKind::Always {
      return;
    }
    if site.callable != map_owner.callable || Some(site.block) != map_owner.block {
      return;
    }
    let Some(key) = site.first_key else {
      return;
    };
    let lookup = self.classify_key(key);
    if lookup.id.raw_of().is_none() {
      return;
    }
    self.indexes.note_identity();
    if entries.contains_key(&lookup.id) {
      return;
    }
    let Some(stored) = self.opposite_entry(entries, lookup.id) else {
      return;
    };
    let Some(demand) = self.unguarded_get_demand(site.node_id, site, map_origin) else {
      return;
    };
    let Some((stored_kind, lookup_kind, wrapper, stored_key)) = self.pair_spans(lookup, stored)
    else {
      return;
    };
    let _ = map_root;
    self.facts.raw_proxy_map_key.push(RawProxyMapKeyFact {
      demand_span: demand,
      get_span: self.span(site.span),
      stored_key_span: stored_key,
      wrapper_span: wrapper,
      stored_kind,
      lookup_kind,
    });
  }

  fn opposite_entry(
    &self,
    entries: &HashMap<KeyId, EntryProv>,
    lookup: KeyId,
  ) -> Option<(KeyId, EntryProv)> {
    match lookup {
      KeyId::Raw(raw) => {
        self.indexes.note_identity();
        if let Some(prov) = entries.get(&KeyId::Proxy { raw, flavor: ProxyFlavor::Deep }).copied() {
          return Some((KeyId::Proxy { raw, flavor: ProxyFlavor::Deep }, prov));
        }
        self.indexes.note_identity();
        entries
          .get(&KeyId::Proxy { raw, flavor: ProxyFlavor::Shallow })
          .copied()
          .map(|prov| (KeyId::Proxy { raw, flavor: ProxyFlavor::Shallow }, prov))
      }
      KeyId::Proxy { raw, .. } => {
        self.indexes.note_identity();
        entries.get(&KeyId::Raw(raw)).copied().map(|prov| (KeyId::Raw(raw), prov))
      }
      KeyId::Primitive | KeyId::Unknown => None,
    }
  }

  pub(super) fn collect_keyed_map_foreach(&mut self, node_id: NodeId, call: &CallExpression<'_>) {
    if self.indexes.map_intrinsic_poisoned {
      return;
    }
    let Some(site) = self.member_site(node_id, call, "forEach") else {
      return;
    };
    if site.optional || site.has_spread || site.arg_count != 1 || site.exec != ExecKind::Always {
      return;
    }
    let Some(receiver) = self.receiver_symbol(call) else {
      return;
    };
    if self.reactive_map_root(receiver).is_none() {
      return;
    }
    let Some((api, getter)) = self.enclosing_tracking_fn(node_id) else {
      return;
    };
    let Some(body) = function_body(getter) else {
      return;
    };
    let Some(callback) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Some((value_sym, key_sym, callback_body)) = foreach_callback(callback) else {
      return;
    };
    let Some((scratch, key, result_span)) = literal_select_scratch(
      self.semantic,
      body,
      callback_body,
      value_sym,
      key_sym,
      api,
      call.span,
    ) else {
      return;
    };
    let _ = scratch;
    self.facts.keyed_map_dependency.push(KeyedMapDependencyFact {
      for_each_span: self.span(call.span),
      map_span: self.span(Self::receiver_span(call).unwrap_or(call.span)),
      result_span: self.span(result_span),
      key,
      api: api.into(),
    });
  }

  fn member_site(
    &self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    method: &str,
  ) -> Option<MemberCallSite> {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return None;
    };
    if member.property.name.as_str() != method {
      return None;
    }
    self.indexes.member_site(node_id)
  }

  fn receiver_symbol(&self, call: &CallExpression<'_>) -> Option<SymbolId> {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return None;
    };
    let ident = member.object.get_inner_expression().get_identifier_reference()?;
    self.reference_symbol(ident).map(|symbol| {
      self.indexes.note_query();
      self.indexes.root_of(symbol)
    })
  }

  fn receiver_span(call: &CallExpression<'_>) -> Option<Span> {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return None;
    };
    Some(member.object.get_inner_expression().span())
  }

  fn native_map_root(&mut self, root: SymbolId) -> Option<SymbolId> {
    self.indexes.note_query();
    if self.indexes.map_intrinsic_poisoned {
      return None;
    }
    if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      return None;
    }
    if self.indexes.escaped.contains(&root)
      || self.indexes.reassigned.contains(&root)
      || self.indexes.uncertain.contains(&root)
      || self.indexes.unknown_member_touch.contains(&root)
      || self.indexes.map_has_member_write(root)
    {
      return None;
    }
    if self.classify_symbol(root, MAX_DEPTH) != Shape::Collection {
      return None;
    }
    let init = *self.indexes.init_span.get(&root)?;
    let new_span = self.new_span_of(init)?;
    let info = self.indexes.news.get(&span_key(new_span)).copied()?;
    (info.ctor == Some("Map")).then_some(root)
  }

  fn map_ctor_span(&self, span: Span) -> Option<Span> {
    if let Some(new_span) = self.new_span_of(span) {
      let info = self.indexes.news.get(&span_key(new_span)).copied()?;
      return (info.ctor == Some("Map")).then_some(new_span);
    }
    if let Some(info) = self.proxy_call(span)
      && let Some(target) = info.first_arg
      && matches!(info.api, Some("reactive" | "shallowReactive"))
    {
      if let Some(new_span) = self.new_span_of(target) {
        let ctor = self.indexes.news.get(&span_key(new_span)).copied()?;
        return (ctor.ctor == Some("Map")).then_some(new_span);
      }
      let hint = self.indexes.hints.get(&span_key(target)).copied()?;
      if let ShapeHint::Identifier(Some(symbol_id), false) = hint {
        self.indexes.note_query();
        let nested = self.indexes.root_of(symbol_id);
        let init = *self.indexes.init_span.get(&nested)?;
        return self.new_span_of(init).and_then(|new_span| {
          self
            .indexes
            .news
            .get(&span_key(new_span))
            .copied()
            .filter(|ctor| ctor.ctor == Some("Map"))
            .map(|_| new_span)
        });
      }
    }
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    let ShapeHint::Identifier(Some(symbol_id), false) = hint else {
      return None;
    };
    self.indexes.note_query();
    let nested = self.indexes.root_of(symbol_id);
    let init = *self.indexes.init_span.get(&nested)?;
    self.new_span_of(init).and_then(|new_span| {
      self
        .indexes
        .news
        .get(&span_key(new_span))
        .copied()
        .filter(|info| info.ctor == Some("Map"))
        .map(|_| new_span)
    })
  }

  fn reactive_map_root(&mut self, root: SymbolId) -> Option<SymbolId> {
    self.indexes.note_query();
    if self.indexes.map_intrinsic_poisoned {
      return None;
    }
    if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      return None;
    }
    if self.indexes.reassigned.contains(&root)
      || self.indexes.escaped(root)
      || self.indexes.uncertain_root(root)
    {
      return None;
    }
    if self.indexes.map_method_overridden(root, "forEach")
      || self.indexes.map_method_overridden(root, "get")
    {
      return None;
    }
    let shape = self.classify_symbol(root, MAX_DEPTH);
    if shape != Shape::DeepProxy && shape != Shape::ShallowProxy {
      return None;
    }
    let init = *self.indexes.init_span.get(&root)?;
    let info = self.proxy_call(init)?;
    if !info.actual_proxy_origin || !matches!(info.api, Some("reactive" | "shallowReactive")) {
      return None;
    }
    let target = info.first_arg?;
    if let Some(inner) = self.wrapped_map_root(target)
      && !self.indexes.wrapped_map_capability_intact(inner)
    {
      return None;
    }
    self.map_ctor_span(target).map(|_| root)
  }

  fn wrapped_map_root(&self, target: Span) -> Option<SymbolId> {
    let hint = self.indexes.hints.get(&span_key(target)).copied()?;
    let ShapeHint::Identifier(Some(symbol_id), false) = hint else {
      return None;
    };
    self.indexes.note_query();
    Some(self.indexes.root_of(symbol_id))
  }

  fn proxy_call(&self, span: Span) -> Option<super::index::CallInfo> {
    self.indexes.note_query();
    if let Some(info) = self.indexes.calls.get(&span_key(span)).copied() {
      return Some(info);
    }
    let ShapeHint::Call(call_span) = self.indexes.hints.get(&span_key(span)).copied()? else {
      return None;
    };
    self.indexes.calls.get(&span_key(call_span)).copied()
  }

  fn new_span_of(&self, span: Span) -> Option<Span> {
    self.indexes.note_query();
    if self.indexes.news.contains_key(&span_key(span)) {
      return Some(span);
    }
    match self.indexes.hints.get(&span_key(span)).copied()? {
      ShapeHint::New(new_span) => Some(new_span),
      _ => None,
    }
  }

  fn constructor_state(&mut self, map_root: SymbolId) -> Option<HashMap<KeyId, EntryProv>> {
    let init = *self.indexes.init_span.get(&map_root)?;
    let new_span = self.map_ctor_span(init).or_else(|| self.new_span_of(init))?;
    let CtorKeyState::Known(key_count) = self.indexes.ctor_key_count(new_span)? else {
      return None;
    };
    let mut entries = HashMap::new();
    for index in 0..key_count {
      let key = self.indexes.ctor_key_at(new_span, index)?;
      self.indexes.note_entry();
      let classified = self.classify_key(key);
      if matches!(classified.id, KeyId::Unknown) {
        return None;
      }
      self.upsert_keyed(&mut entries, classified);
    }
    Some(entries)
  }

  fn apply_mutation(
    &mut self,
    map_root: SymbolId,
    entries: &mut HashMap<KeyId, EntryProv>,
    op: MapOp,
    get: MemberCallSite,
  ) -> ApplyResult {
    let _ = map_root;
    let write = matches!(op.method, Some("set" | "delete" | "clear") | None);
    if op.site.callable != get.callable || op.site.block != get.block {
      if write && op.site.exec != ExecKind::Never {
        return ApplyResult::Unknown;
      }
      return ApplyResult::Keep;
    }
    match op.site.exec {
      ExecKind::Maybe if write => return ApplyResult::Unknown,
      ExecKind::Never | ExecKind::Maybe => return ApplyResult::Keep,
      ExecKind::Always => {}
    }
    match op.method {
      Some("set") => {
        if op.site.first_key_unknown {
          return ApplyResult::Unknown;
        }
        let Some(key) = op.site.first_key else {
          return ApplyResult::Unknown;
        };
        let classified = self.classify_key(key);
        if matches!(classified.id, KeyId::Unknown) {
          return ApplyResult::Unknown;
        }
        self.upsert_keyed(entries, classified);
        ApplyResult::Keep
      }
      Some("delete") => {
        if op.site.first_key_unknown {
          return ApplyResult::Unknown;
        }
        let Some(key) = op.site.first_key else {
          return ApplyResult::Unknown;
        };
        let classified = self.classify_key(key);
        if matches!(classified.id, KeyId::Unknown) {
          return ApplyResult::Unknown;
        }
        self.indexes.note_identity();
        entries.remove(&classified.id);
        ApplyResult::Keep
      }
      Some("clear") => {
        entries.clear();
        ApplyResult::Keep
      }
      None | Some(_) => ApplyResult::Unknown,
    }
  }

  fn upsert_keyed(&self, entries: &mut HashMap<KeyId, EntryProv>, classified: ClassifiedKey) {
    self.indexes.note_identity();
    entries.insert(classified.id, EntryProv { span: classified.span, wrapper: classified.wrapper });
  }

  fn classify_key(&mut self, key: MapKeyRef) -> ClassifiedKey {
    self.indexes.note_query();
    if key.is_literal {
      return ClassifiedKey { id: KeyId::Primitive, span: key.span, wrapper: None };
    }
    if let Some(raw) = key.proxy_of {
      self.indexes.note_query();
      let raw = self.indexes.root_of(raw);
      if key.actual_proxy
        && let Some(flavor) = key.proxy_flavor
        && self.is_fresh_plain_object(raw)
        && self.proxy_capability_holds(raw)
      {
        return ClassifiedKey {
          id: KeyId::Proxy { raw, flavor },
          span: key.span,
          wrapper: Some(key.span),
        };
      }
      return ClassifiedKey { id: KeyId::Unknown, span: key.span, wrapper: None };
    }
    let Some(symbol) = key.symbol else {
      return ClassifiedKey { id: KeyId::Unknown, span: key.span, wrapper: None };
    };
    self.indexes.note_query();
    let root = self.indexes.root_of(symbol);
    if self.is_fresh_plain_object(root) {
      return ClassifiedKey { id: KeyId::Raw(root), span: key.span, wrapper: None };
    }
    if let Some((raw, flavor, wrapper)) = self.distinct_proxy(root) {
      return ClassifiedKey {
        id: KeyId::Proxy { raw, flavor },
        span: key.span,
        wrapper: Some(wrapper),
      };
    }
    ClassifiedKey { id: KeyId::Unknown, span: key.span, wrapper: None }
  }

  fn is_fresh_plain_object(&mut self, root: SymbolId) -> bool {
    self.indexes.note_query();
    if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      return false;
    }
    if self.indexes.reassigned.contains(&root) {
      return false;
    }
    let Some(init) = self.indexes.init_span.get(&root).copied() else {
      return false;
    };
    self.indexes.object_literals.contains(&span_key(init))
      && self.classify_span(init, MAX_DEPTH) == Shape::PlainRecord
  }

  fn proxy_capability_holds(&self, raw: SymbolId) -> bool {
    let Some(init) = self.indexes.init_span.get(&raw).copied() else {
      return false;
    };
    !self.indexes.object_has_skip_marker(init)
      && !self.indexes.skip_written.contains(&raw)
      && !self.indexes.helper_escaped(raw)
  }

  fn distinct_proxy(&mut self, root: SymbolId) -> Option<(SymbolId, ProxyFlavor, Span)> {
    if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      return None;
    }
    if self.indexes.reassigned.contains(&root) {
      return None;
    }
    let wrapper = self.indexes.wrapper_span(root)?;
    let init = *self.indexes.init_span.get(&root)?;
    let info = self.proxy_call(init)?;
    if !info.actual_proxy_origin {
      return None;
    }
    let flavor = proxy_flavor(info.api?)?;
    let arg = info.first_arg?;
    let hint = self.indexes.hints.get(&span_key(arg)).copied()?;
    let ShapeHint::Identifier(Some(symbol_id), false) = hint else {
      return None;
    };
    self.indexes.note_query();
    let raw = self.indexes.root_of(symbol_id);
    if self.is_fresh_plain_object(raw) && self.proxy_capability_holds(raw) {
      Some((raw, flavor, wrapper))
    } else {
      None
    }
  }

  fn pair_spans(
    &self,
    lookup: ClassifiedKey,
    stored: (KeyId, EntryProv),
  ) -> Option<(String, String, vue_vet_core::SourceSpan, vue_vet_core::SourceSpan)> {
    let wrapper = lookup.wrapper.or(stored.1.wrapper)?;
    let stored_kind = match stored.0 {
      KeyId::Raw(_) => "raw",
      KeyId::Proxy { .. } => "proxy",
      KeyId::Primitive | KeyId::Unknown => return None,
    };
    let lookup_kind = match lookup.id {
      KeyId::Raw(_) => "raw",
      KeyId::Proxy { .. } => "proxy",
      KeyId::Primitive | KeyId::Unknown => return None,
    };
    Some((stored_kind.into(), lookup_kind.into(), self.span(wrapper), self.span(stored.1.span)))
  }

  fn unguarded_get_demand(
    &self,
    node_id: NodeId,
    get: MemberCallSite,
    map_origin: usize,
  ) -> Option<vue_vet_core::SourceSpan> {
    if chain_parent_optional(self.semantic, node_id, self.indexes.work_counter()) {
      return None;
    }
    let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
      return None;
    };
    let parent = skip_ts_parent(self.semantic, node_id, self.indexes.work_counter());
    let demand = match self.semantic.nodes().kind(parent) {
      AstKind::StaticMemberExpression(member) => {
        if member.optional
          || chain_parent_optional(self.semantic, parent, self.indexes.work_counter())
        {
          return None;
        }
        if member.object.get_inner_expression().span() != call.span {
          return None;
        }
        self.span(member.span)
      }
      AstKind::ComputedMemberExpression(member) => {
        if member.optional
          || chain_parent_optional(self.semantic, parent, self.indexes.work_counter())
        {
          return None;
        }
        if member.object.get_inner_expression().span() != call.span {
          return None;
        }
        self.span(member.span)
      }
      AstKind::CallExpression(outer) => {
        if expression_is_callee(&outer.callee, call) {
          self.span(outer.span)
        } else {
          return None;
        }
      }
      AstKind::LogicalExpression(logical)
        if matches!(logical.operator, LogicalOperator::Coalesce | LogicalOperator::Or) =>
      {
        return None;
      }
      AstKind::VariableDeclarator(declarator) => {
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          return None;
        };
        let symbol_id = binding.symbol_id.get()?;
        if !self.semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
          return None;
        }
        self.indexes.note_query();
        let root = self.indexes.root_of(symbol_id);
        if self.indexes.reassigned.contains(&root) || self.indexes.escaped.contains(&root) {
          return None;
        }
        return self.alias_demand(root, get, map_origin);
      }
      _ => return None,
    };
    self.demand_reachable(get, map_origin, parent, demand.offset).then_some(demand)
  }

  fn demand_reachable(
    &self,
    get: MemberCallSite,
    map_origin: usize,
    demand_node: NodeId,
    demand_offset: usize,
  ) -> bool {
    if self.indexes.exec_kind(self.semantic, demand_node) != ExecKind::Always {
      return false;
    }
    let Some(entry) = self.indexes.region_entry(get.region) else {
      return false;
    };
    !self.indexes.has_barrier_between(get.region, entry, map_origin)
      && !self.indexes.has_barrier_between(get.region, map_origin, get.offset)
      && !self.indexes.has_barrier_between(get.region, get.offset, demand_offset)
  }

  fn alias_demand(
    &self,
    root: SymbolId,
    get: MemberCallSite,
    map_origin: usize,
  ) -> Option<vue_vet_core::SourceSpan> {
    let mut demand = None;
    let mut guarded = false;
    for reference in self.semantic.symbol_references(root) {
      self.indexes.note_query();
      if reference.flags().is_write() {
        return None;
      }
      let node_id = reference.node_id();
      if matches!(self.semantic.nodes().parent_kind(node_id), AstKind::VariableDeclarator(_)) {
        continue;
      }
      let parent = skip_ts_parent(self.semantic, node_id, self.indexes.work_counter());
      match self.semantic.nodes().kind(parent) {
        AstKind::StaticMemberExpression(member) => {
          if member.optional
            || chain_parent_optional(self.semantic, parent, self.indexes.work_counter())
          {
            guarded = true;
            continue;
          }
          let object = member.object.get_inner_expression().get_identifier_reference()?;
          if object.span != self.semantic.nodes().kind(node_id).span() {
            return None;
          }
          match self.indexes.exec_kind(self.semantic, parent) {
            ExecKind::Never => continue,
            ExecKind::Maybe => return None,
            ExecKind::Always => {}
          }
          if !self.alias_use_on_path(
            node_id,
            get,
            map_origin,
            parent,
            self.span(member.span).offset,
          ) {
            return None;
          }
          demand.get_or_insert_with(|| self.span(member.span));
        }
        AstKind::ComputedMemberExpression(member) => {
          if member.optional
            || chain_parent_optional(self.semantic, parent, self.indexes.work_counter())
          {
            guarded = true;
            continue;
          }
          let object = member.object.get_inner_expression().get_identifier_reference()?;
          if object.span != self.semantic.nodes().kind(node_id).span() {
            return None;
          }
          match self.indexes.exec_kind(self.semantic, parent) {
            ExecKind::Never => continue,
            ExecKind::Maybe => return None,
            ExecKind::Always => {}
          }
          if !self.alias_use_on_path(
            node_id,
            get,
            map_origin,
            parent,
            self.span(member.span).offset,
          ) {
            return None;
          }
          demand.get_or_insert_with(|| self.span(member.span));
        }
        AstKind::CallExpression(outer) => {
          if callee_is_identifier(outer, node_id, self.semantic) {
            match self.indexes.exec_kind(self.semantic, parent) {
              ExecKind::Never => continue,
              ExecKind::Maybe => return None,
              ExecKind::Always => {}
            }
            if !self.alias_use_on_path(
              node_id,
              get,
              map_origin,
              parent,
              self.span(outer.span).offset,
            ) {
              return None;
            }
            demand.get_or_insert_with(|| self.span(outer.span));
          } else {
            return None;
          }
        }
        AstKind::LogicalExpression(_)
        | AstKind::IfStatement(_)
        | AstKind::ConditionalExpression(_) => guarded = true,
        AstKind::UnaryExpression(_) | AstKind::ExpressionStatement(_) => {}
        _ => return None,
      }
    }
    if guarded {
      return None;
    }
    demand
  }

  fn alias_use_on_path(
    &self,
    node_id: NodeId,
    get: MemberCallSite,
    map_origin: usize,
    demand_node: NodeId,
    demand_offset: usize,
  ) -> bool {
    let owner = self.indexes.owner(node_id);
    owner.callable == get.callable
      && owner.block == Some(get.block)
      && self.demand_reachable(get, map_origin, demand_node, demand_offset)
  }

  fn enclosing_tracking_fn(&self, node_id: NodeId) -> Option<(&'static str, &Expression<'_>)> {
    let mut current = node_id;
    for _ in 0..32 {
      let parent = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent) {
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
          let (_, call) = enclosing_call(self.semantic, parent, self.indexes.work_counter())?;
          let info = self.indexes.calls.get(&span_key(call.span)).copied()?;
          if !matches!(info.api, Some("computed" | "watchEffect")) || info.has_spread {
            return None;
          }
          let getter = call.arguments.first().and_then(Argument::as_expression)?;
          if getter.get_inner_expression().span() != self.semantic.nodes().kind(parent).span()
            && getter.span() != self.semantic.nodes().kind(parent).span()
          {
            return None;
          }
          return Some((info.api?, getter));
        }
        AstKind::Program(_) => return None,
        _ => current = parent,
      }
    }
    None
  }
}

enum ApplyResult {
  Keep,
  Unknown,
}

fn function_body<'a>(expression: &'a Expression<'a>) -> Option<&'a FunctionBody<'a>> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) if !arrow.expression => Some(&arrow.body),
    Expression::FunctionExpression(function) => function.body.as_deref(),
    _ => None,
  }
}

fn foreach_callback<'a>(
  expression: &'a Expression<'a>,
) -> Option<(SymbolId, SymbolId, &'a FunctionBody<'a>)> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) if !arrow.expression && !arrow.r#async => {
      let (value, key) = two_params(&arrow.params)?;
      Some((value, key, &arrow.body))
    }
    Expression::FunctionExpression(function) if !function.generator && !function.r#async => {
      let (value, key) = two_params(&function.params)?;
      Some((value, key, function.body.as_deref()?))
    }
    _ => None,
  }
}

fn two_params(params: &FormalParameters<'_>) -> Option<(SymbolId, SymbolId)> {
  if params.rest.is_some() || params.items.len() != 2 {
    return None;
  }
  let value = param_symbol(&params.items.first()?.pattern)?;
  let key = param_symbol(&params.items.get(1)?.pattern)?;
  Some((value, key))
}

fn param_symbol(pattern: &BindingPattern<'_>) -> Option<SymbolId> {
  let BindingPattern::BindingIdentifier(binding) = pattern else {
    return None;
  };
  binding.symbol_id.get()
}

fn literal_select_scratch(
  semantic: &oxc_semantic::Semantic<'_>,
  body: &FunctionBody<'_>,
  callback: &FunctionBody<'_>,
  value_sym: SymbolId,
  key_sym: SymbolId,
  api: &str,
  for_each_span: Span,
) -> Option<(SymbolId, String, Span)> {
  if body.statements.len() != 3 {
    return None;
  }
  let scratch = let_undefined_scratch(semantic, body.statements.first()?)?;
  let for_each = as_expression_stmt(body.statements.get(1)?)?;
  let Expression::CallExpression(call) = for_each.get_inner_expression() else {
    return None;
  };
  if call.span != for_each_span {
    return None;
  }
  let key = single_select_assignment(semantic, callback, scratch, value_sym, key_sym)?;
  let result_span = match api {
    "computed" => {
      let Statement::ReturnStatement(ret) = body.statements.get(2)? else {
        return None;
      };
      let argument = ret.argument.as_ref()?;
      let ident = argument.get_inner_expression().get_identifier_reference()?;
      let symbol = reference_symbol(semantic, ident)?;
      (symbol == scratch).then_some(argument.span())?
    }
    "watchEffect" => use_scratch_span(semantic, body.statements.get(2)?, scratch)?,
    _ => return None,
  };
  Some((scratch, key, result_span))
}

fn let_undefined_scratch(
  semantic: &oxc_semantic::Semantic<'_>,
  statement: &Statement<'_>,
) -> Option<SymbolId> {
  let Statement::VariableDeclaration(declaration) = statement else {
    return None;
  };
  if declaration.kind != VariableDeclarationKind::Let || declaration.declarations.len() != 1 {
    return None;
  }
  let declarator = declaration.declarations.first()?;
  let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
    return None;
  };
  match &declarator.init {
    None => {}
    Some(init) => {
      let ident = init.get_inner_expression().get_identifier_reference()?;
      if ident.name.as_str() != "undefined" || reference_symbol(semantic, ident).is_some() {
        return None;
      }
    }
  }
  binding.symbol_id.get()
}

fn single_select_assignment(
  semantic: &oxc_semantic::Semantic<'_>,
  body: &FunctionBody<'_>,
  scratch: SymbolId,
  value_sym: SymbolId,
  key_sym: SymbolId,
) -> Option<String> {
  if body.statements.len() != 1 {
    return None;
  }
  let statement = match body.statements.first()? {
    Statement::IfStatement(if_stmt) if if_stmt.alternate.is_none() => if_stmt,
    _ => return None,
  };
  let key = strict_eq_key_literal(semantic, &statement.test, key_sym)?;
  let assign = match &statement.consequent {
    Statement::ExpressionStatement(expr) => &expr.expression,
    Statement::BlockStatement(block) if block.body.len() == 1 => {
      let Statement::ExpressionStatement(expr) = block.body.first()? else {
        return None;
      };
      &expr.expression
    }
    _ => return None,
  };
  let Expression::AssignmentExpression(assignment) = assign.get_inner_expression() else {
    return None;
  };
  if assignment.operator != AssignmentOperator::Assign {
    return None;
  }
  let AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left else {
    return None;
  };
  let target_sym = reference_symbol(semantic, target)?;
  if target_sym != scratch {
    return None;
  }
  let value = assignment.right.get_inner_expression().get_identifier_reference()?;
  let value_id = reference_symbol(semantic, value)?;
  (value_id == value_sym).then_some(key)
}

fn strict_eq_key_literal(
  semantic: &oxc_semantic::Semantic<'_>,
  test: &Expression<'_>,
  key_sym: SymbolId,
) -> Option<String> {
  let Expression::BinaryExpression(binary) = test.get_inner_expression() else {
    return None;
  };
  if binary.operator != BinaryOperator::StrictEquality {
    return None;
  }
  let (ident, literal) =
    if let Some(ident) = binary.left.get_inner_expression().get_identifier_reference() {
      (ident, binary.right.get_inner_expression())
    } else {
      let ident = binary.right.get_inner_expression().get_identifier_reference()?;
      (ident, binary.left.get_inner_expression())
    };
  if reference_symbol(semantic, ident)? != key_sym {
    return None;
  }
  let Expression::StringLiteral(string) = literal else {
    return None;
  };
  Some(string.value.to_string())
}

fn use_scratch_span(
  semantic: &oxc_semantic::Semantic<'_>,
  statement: &Statement<'_>,
  scratch: SymbolId,
) -> Option<Span> {
  match statement {
    Statement::ExpressionStatement(expr) => identifier_span_if(semantic, &expr.expression, scratch)
      .or_else(|| call_arg_scratch(semantic, &expr.expression, scratch)),
    Statement::ReturnStatement(ret) => {
      identifier_span_if(semantic, ret.argument.as_ref()?, scratch)
    }
    _ => None,
  }
}

fn identifier_span_if(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  scratch: SymbolId,
) -> Option<Span> {
  let inner = expression.get_inner_expression();
  if let Some(ident) = inner.get_identifier_reference() {
    return (reference_symbol(semantic, ident)? == scratch).then_some(ident.span);
  }
  if let Expression::UnaryExpression(unary) = inner {
    return identifier_span_if(semantic, &unary.argument, scratch);
  }
  None
}

fn call_arg_scratch(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  scratch: SymbolId,
) -> Option<Span> {
  let Expression::CallExpression(call) = expression.get_inner_expression() else {
    return None;
  };
  if call.arguments.len() != 1 {
    return None;
  }
  let arg = call.arguments.first().and_then(Argument::as_expression)?;
  identifier_span_if(semantic, arg, scratch)
}

fn as_expression_stmt<'a>(statement: &'a Statement<'a>) -> Option<&'a Expression<'a>> {
  match statement {
    Statement::ExpressionStatement(expr) => Some(&expr.expression),
    _ => None,
  }
}

fn chain_parent_optional(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  work: &WorkCounter,
) -> bool {
  matches!(
    semantic.nodes().kind(skip_ts_parent(semantic, node_id, work)),
    AstKind::ChainExpression(_)
  ) || matches!(semantic.nodes().parent_kind(node_id), AstKind::ChainExpression(_))
}

fn expression_is_callee(callee: &Expression<'_>, call: &CallExpression<'_>) -> bool {
  callee.get_inner_expression().span() == call.span
}

fn callee_is_identifier(
  call: &CallExpression<'_>,
  ident_id: NodeId,
  semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
  let Some(ident) = call.callee.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  ident.span == semantic.nodes().kind(ident_id).span()
}

fn reference_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}
