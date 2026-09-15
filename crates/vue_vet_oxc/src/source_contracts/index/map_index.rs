//! Native `Map` key-identity indexing: constructor/`set`/`get` key roles,
//! intrinsic-poison detection, and wrapper (`reactive` / `shallowReactive`)
//! origin proofs consumed by `super::super::map_lookup`.

use super::{
  Argument, ArrayExpression, ArrayExpressionElement, AssignmentOperator, AssignmentTarget,
  AssignmentTargetMaybeDefault, AssignmentTargetProperty, AstKind, CallExpression, CallInfo,
  ChainPlace, CtorKeyState, ExecKind, Expression, ForStatementLeft, GetSpan, HashMap,
  IdentifierReference, Indexes, LogicalOperator, MapKeyRef, MapOp, MemberCallSite, MemberWrite,
  NewExpression, NewInfo, NodeId, ObjectPropertyKind, Owner, PendingMapKeyArg, ProxyFlavor,
  ScriptKind, ShapeHint, SimpleAssignmentTarget, Span, SymbolFlags, SymbolId, VueImport,
  WorkCounter, WrapperOrigin, callee_has_actual_proxy_origin, chain_optional, is_fresh_allocation,
  is_proxy_allocating_api, mapped, reference_symbol, resolve_vue_api, span_key,
};

impl Indexes {
  pub(in crate::source_contracts) fn record_region_starts(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
  ) {
    for (node_id, node) in semantic.nodes().iter_enumerated() {
      match node.kind() {
        AstKind::Program(_) | AstKind::FunctionBody(_) => {
          self.work.add_queries(1);
          self.region_start.insert(
            node_id,
            mapped(line_index, sfc_source, script_offset, node.kind().span()).offset,
          );
        }
        _ => {}
      }
    }
  }

  pub(in crate::source_contracts) fn map_has_member_write(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.member_write_roots.contains(&root)
  }

  pub(in crate::source_contracts) fn map_method_overridden(
    &self,
    root: SymbolId,
    method: &str,
  ) -> bool {
    self.work.add_queries(1);
    self.member_writes.contains_key(&(root, method.to_string()))
  }

  pub(in crate::source_contracts) fn map_capability_intact(&self, root: SymbolId) -> bool {
    !self.reassigned.contains(&root)
      && !self.escaped(root)
      && !self.uncertain_root(root)
      && !self.map_has_member_write(root)
  }

  pub(in crate::source_contracts) fn wrapped_map_capability_intact(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    if self.unresolved_origin_touch {
      return false;
    }
    if let Some(intact) = self.alloc_capability.get(&root).copied() {
      return intact;
    }
    self.receiver_capability_intact(root)
  }

  pub(in crate::source_contracts) fn receiver_capability_intact(&self, root: SymbolId) -> bool {
    !self.reassigned.contains(&root)
      && !self.helper_escaped(root)
      && !self.capability_poisoned(root)
      && !self.unknown_member_touch.contains(&root)
      && !self.map_has_member_write(root)
      && !self.map_method_overridden(root, "forEach")
      && !self.map_method_overridden(root, "get")
  }

  pub(in crate::source_contracts) fn helper_escaped(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.helper_escaped.contains(&root)
  }

  pub(in crate::source_contracts) fn escaped(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.escaped.contains(&root)
  }

  pub(in crate::source_contracts) fn uncertain_root(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.uncertain.contains(&root) || self.unknown_member_touch.contains(&root)
  }

  pub(in crate::source_contracts) fn declaration_owner(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    symbol_id: SymbolId,
  ) -> Owner {
    self.owner(semantic.scoping().symbol_declaration(symbol_id))
  }

  pub(in crate::source_contracts) fn member_site(&self, node_id: NodeId) -> Option<MemberCallSite> {
    self.work.add_queries(1);
    self.member_call_by_node.get(&node_id).copied()
  }

  pub(in crate::source_contracts) fn map_get_count(&self, root: SymbolId) -> usize {
    self.work.add_queries(1);
    self.map_gets.get(&root).map_or(0, Vec::len)
  }

  pub(in crate::source_contracts) fn map_get_at(
    &self,
    root: SymbolId,
    index: usize,
  ) -> Option<MemberCallSite> {
    self.work.add_queries(1);
    self.map_gets.get(&root).and_then(|gets| gets.get(index)).copied()
  }

  pub(in crate::source_contracts) fn map_mutation_count(&self, root: SymbolId) -> usize {
    self.work.add_queries(1);
    self.map_ops.get(&root).map_or(0, Vec::len)
  }

  pub(in crate::source_contracts) fn map_mutation_at(
    &self,
    root: SymbolId,
    index: usize,
  ) -> Option<MapOp> {
    self.work.add_queries(1);
    self.map_ops.get(&root).and_then(|ops| ops.get(index)).copied()
  }

  pub(in crate::source_contracts) fn map_get_roots(&self) -> Vec<SymbolId> {
    self.work.add_queries(1);
    let mut roots: Vec<SymbolId> = self.map_gets.keys().copied().collect();
    roots.sort_by(|left, right| {
      self.work.add_queries(1);
      self
        .init_offset
        .get(left)
        .copied()
        .unwrap_or(0)
        .cmp(&self.init_offset.get(right).copied().unwrap_or(0))
    });
    roots
  }

  pub(in crate::source_contracts) fn ctor_key_count(&self, new_span: Span) -> Option<CtorKeyState> {
    self.work.add_queries(1);
    Some(
      self
        .map_init_keys
        .get(&span_key(new_span))?
        .as_ref()
        .map_or(CtorKeyState::Unknown, |keys| CtorKeyState::Known(keys.len())),
    )
  }

  pub(in crate::source_contracts) fn ctor_key_at(
    &self,
    new_span: Span,
    index: usize,
  ) -> Option<MapKeyRef> {
    self.work.add_queries(1);
    self.map_init_keys.get(&span_key(new_span))?.as_ref()?.get(index).copied()
  }

  pub(in crate::source_contracts) fn init_mapped_offset(&self, root: SymbolId) -> Option<usize> {
    self.work.add_queries(1);
    self.init_offset.get(&root).copied()
  }

  pub(in crate::source_contracts) fn region_entry(&self, region: NodeId) -> Option<usize> {
    self.work.add_queries(1);
    self.region_start.get(&region).copied()
  }

  pub(in crate::source_contracts) fn wrapper_span(&self, proxy: SymbolId) -> Option<Span> {
    self.work.add_queries(1);
    self.proxy_wrapper.get(&proxy).copied()
  }

  pub(in crate::source_contracts) fn object_has_skip_marker(&self, span: Span) -> bool {
    self.work.add_queries(1);
    self.skip_marker_objects.contains(&span_key(span))
  }

  pub(in crate::source_contracts) fn mark_helper_escape_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    if let Some(identifier) = expression.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.helper_escaped.insert(self.root_of(symbol_id));
    }
  }

  pub(in crate::source_contracts) fn finish_map_indexes(&mut self) {
    let mut member_calls = std::mem::take(&mut self.member_calls);
    for calls in member_calls.values_mut() {
      calls.sort_by(|left, right| {
        self.work.add_queries(1);
        left.offset.cmp(&right.offset)
      });
      for site in calls.iter() {
        self.member_call_by_node.insert(site.node_id, *site);
      }
    }
    let mut map_ops: HashMap<SymbolId, Vec<MapOp>> = HashMap::new();
    let mut map_gets: HashMap<SymbolId, Vec<MemberCallSite>> = HashMap::new();
    for ((root, method), calls) in &member_calls {
      let interned = intern_map_method(method);
      if interned == Some("get") {
        map_gets.entry(*root).or_default().extend(calls.iter().copied());
        continue;
      }
      if matches!(interned, Some("has" | "forEach" | "keys" | "values" | "entries")) {
        continue;
      }
      for site in calls {
        map_ops.entry(*root).or_default().push(MapOp { method: interned, site: *site });
      }
    }
    for ops in map_ops.values_mut() {
      ops.sort_by(|left, right| {
        self.work.add_queries(1);
        left.site.offset.cmp(&right.site.offset)
      });
    }
    self.member_calls = member_calls;
    self.map_ops = map_ops;
    self.map_gets = map_gets;
    let inits: Vec<(SymbolId, Span)> =
      self.init_span.iter().map(|(symbol, span)| (*symbol, *span)).collect();
    for (symbol, init) in inits {
      let info = if let Some(info) = self.calls.get(&span_key(init)).copied() {
        info
      } else if let Some(ShapeHint::Call(span)) = self.hints.get(&span_key(init)).copied() {
        let Some(info) = self.calls.get(&span_key(span)).copied() else {
          continue;
        };
        info
      } else {
        continue;
      };
      if !info.actual_proxy_origin || !matches!(info.api, Some("reactive" | "shallowReactive")) {
        continue;
      }
      let Some(arg) = info.first_arg else {
        continue;
      };
      let Some(ShapeHint::Identifier(Some(raw), false)) = self.hints.get(&span_key(arg)).copied()
      else {
        continue;
      };
      let origin = self.root_of(raw);
      self.work.add_queries(1);
      self.proxy_wrapper.insert(symbol, info.span);
      self.work.add_queries(1);
      self.wrapper_origin.insert(symbol, origin);
    }
    self.build_canonical_origins();
    let origins: Vec<(SymbolId, SymbolId)> =
      self.wrapper_origin.iter().map(|(w, o)| (*w, *o)).collect();
    self.work.add_queries(origins.len() as u64);
    for (wrapper, origin) in origins {
      match self.canonical_alloc(origin) {
        Some(alloc) => {
          self.work.add_queries(1);
          self.wrappers_of_alloc.entry(alloc).or_default().push(wrapper);
        }
        None => self.unresolved_origin_touch = true,
      }
    }
    self.sort_wrappers_of_alloc();
    self.apply_pending_inline_touches();
    self.summarize_alloc_capability();
  }

  pub(in crate::source_contracts) fn build_canonical_origins(&mut self) {
    let wrappers: Vec<SymbolId> = self.wrapper_origin.keys().copied().collect();
    self.work.add_queries(wrappers.len() as u64);
    for wrapper in wrappers {
      let mut stack = Vec::new();
      self.resolve_canonical(wrapper, &mut stack);
    }
  }

  pub(in crate::source_contracts) fn resolve_canonical(
    &mut self,
    symbol: SymbolId,
    stack: &mut Vec<SymbolId>,
  ) -> Option<SymbolId> {
    if let Some(cached) = self.canonical_of.get(&symbol).copied() {
      self.work.add_queries(1);
      return cached;
    }
    self.work.add_queries(1);
    if stack.contains(&symbol) {
      self.canonical_of.insert(symbol, None);
      return None;
    }
    let Some(origin) = self.wrapper_origin.get(&symbol).copied() else {
      self.canonical_of.insert(symbol, Some(symbol));
      return Some(symbol);
    };
    if origin == symbol {
      self.canonical_of.insert(symbol, Some(symbol));
      return Some(symbol);
    }
    stack.push(symbol);
    let resolved = self.resolve_canonical(origin, stack);
    stack.pop();
    self.canonical_of.insert(symbol, resolved);
    resolved
  }

  pub(in crate::source_contracts) fn canonical_alloc(&self, symbol: SymbolId) -> Option<SymbolId> {
    self.work.add_queries(1);
    if let Some(cached) = self.canonical_of.get(&symbol).copied() {
      return cached;
    }
    if self.wrapper_origin.contains_key(&symbol) {
      return None;
    }
    Some(symbol)
  }

  pub(in crate::source_contracts) fn apply_pending_inline_touches(&mut self) {
    let writes = std::mem::take(&mut self.pending_inline_writes);
    for (origin, property, write) in writes {
      let Some(alloc) = self.canonical_alloc(origin) else {
        self.unresolved_origin_touch = true;
        continue;
      };
      self.work.add_queries(1);
      self.member_write_roots.insert(alloc);
      self.member_writes.entry((alloc, property)).or_default().push(write);
    }
    let unknown = std::mem::take(&mut self.pending_inline_unknown);
    for origin in unknown {
      let Some(alloc) = self.canonical_alloc(origin) else {
        self.unresolved_origin_touch = true;
        continue;
      };
      self.unknown_member_touch.insert(alloc);
    }
  }

  pub(in crate::source_contracts) fn summarize_alloc_capability(&mut self) {
    let allocs: Vec<SymbolId> = self.wrappers_of_alloc.keys().copied().collect();
    for alloc in allocs {
      let intact = self.compute_alloc_intact(alloc);
      self.alloc_capability.insert(alloc, intact);
    }
  }

  pub(in crate::source_contracts) fn compute_alloc_intact(&self, alloc: SymbolId) -> bool {
    self.work.add_queries(1);
    if !self.receiver_capability_intact(alloc) {
      return false;
    }
    let Some(wrappers) = self.wrappers_of_alloc.get(&alloc) else {
      return true;
    };
    wrappers.iter().all(|wrapper| {
      self.work.add_queries(1);
      self.receiver_capability_intact(*wrapper)
    })
  }

  pub(in crate::source_contracts) fn sort_wrappers_of_alloc(&mut self) {
    let mut wrappers = std::mem::take(&mut self.wrappers_of_alloc);
    for list in wrappers.values_mut() {
      list.sort_by(|left, right| {
        self.work.add_queries(1);
        self
          .init_offset
          .get(left)
          .copied()
          .unwrap_or(0)
          .cmp(&self.init_offset.get(right).copied().unwrap_or(0))
      });
    }
    self.wrappers_of_alloc = wrappers;
  }

  pub(in crate::source_contracts) fn record_map_new(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    expression: &NewExpression<'_>,
  ) {
    let ctor =
      unresolved_collection_name(&expression.callee, |ident| reference_symbol(semantic, ident));
    let has_spread = expression.arguments.iter().any(Argument::is_spread);
    let first_arg = if has_spread {
      None
    } else {
      expression.arguments.first().and_then(Argument::as_expression).map(GetSpan::span)
    };
    let second_arg = if has_spread {
      None
    } else {
      expression.arguments.get(1).and_then(Argument::as_expression).map(GetSpan::span)
    };
    let arg_count = u8::try_from(expression.arguments.len()).unwrap_or(u8::MAX);
    self.news.insert(
      span_key(expression.span),
      NewInfo { ctor, first_arg, has_spread, second_arg, arg_count },
    );
    if ctor == Some("Map") {
      self.map_init_keys.insert(
        span_key(expression.span),
        map_constructor_keys(semantic, expression, &self.vue_imports, kind, &self.work),
      );
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "member-call indexing needs the same span mapping as the scan"
  )]
  pub(in crate::source_contracts) fn record_map_member_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return;
    };
    let Some(ident) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let has_spread = call.arguments.iter().any(Argument::is_spread);
    let first = call.arguments.first().and_then(Argument::as_expression);
    let (first_key, first_key_unknown) = match first {
      Some(expression) => {
        extract_map_key_ref(semantic, expression, &self.vue_imports, kind, &self.work)
          .map_or((None, true), |key| (Some(key), false))
      }
      None => (None, has_spread),
    };
    let owner = self.owner(node_id);
    let site = MemberCallSite {
      span: call.span,
      node_id,
      block: owner.block.unwrap_or(node_id),
      callable: owner.callable,
      region: owner.region.or(owner.block).unwrap_or(node_id),
      offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
      optional: chain_optional(semantic, node_id, &self.work),
      has_spread,
      arg_count: u8::try_from(call.arguments.len()).unwrap_or(u8::MAX),
      first_key,
      first_key_unknown,
      exec: self.exec_kind(semantic, node_id),
    };
    self
      .member_calls
      .entry((self.root_of(symbol_id), member.property.name.to_string()))
      .or_default()
      .push(site);
  }

  pub(super) fn pending_native_map_key_arg(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    call: &CallExpression<'_>,
    index: usize,
    expression: &Expression<'_>,
  ) -> Option<PendingMapKeyArg> {
    if index != 0 || call.arguments.iter().any(Argument::is_spread) {
      return None;
    }
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return None;
    };
    let method = intern_map_method(member.property.name.as_str())?;
    if !matches!(method, "get" | "set" | "has" | "delete") {
      return None;
    }
    let ident = member.object.get_inner_expression().get_identifier_reference()?;
    let receiver = reference_symbol(semantic, ident)?;
    let arg_ident = expression.get_inner_expression().get_identifier_reference()?;
    let arg = reference_symbol(semantic, arg_ident)?;
    Some(PendingMapKeyArg { receiver: self.root_of(receiver), method, arg: self.root_of(arg) })
  }

  pub(in crate::source_contracts) fn finish_pending_map_key_args(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
  ) {
    let pending = std::mem::take(&mut self.pending_map_key_args);
    for pending in pending {
      if self.native_map_key_role_holds(semantic, pending.receiver, pending.method) {
        continue;
      }
      self.capability_poisoned.insert(pending.arg);
      self.helper_escaped.insert(pending.arg);
    }
  }

  pub(in crate::source_contracts) fn native_map_key_role_holds(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    root: SymbolId,
    method: &str,
  ) -> bool {
    self.receiver_is_native_map(semantic, root)
      && self.map_capability_intact(root)
      && !self.map_method_overridden(root, method)
  }

  pub(in crate::source_contracts) fn receiver_is_native_map(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    root: SymbolId,
  ) -> bool {
    self.work.add_queries(1);
    if self.reassigned.contains(&root) {
      return false;
    }
    let Some(init) = self.init_span.get(&root).copied() else {
      return false;
    };
    let new_span = if self.news.contains_key(&span_key(init)) {
      init
    } else if let Some(ShapeHint::New(span)) = self.hints.get(&span_key(init)).copied() {
      span
    } else {
      return false;
    };
    self.work.add_queries(1);
    self.news.get(&span_key(new_span)).is_some_and(|info| info.ctor == Some("Map"))
      && semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable)
  }

  pub(in crate::source_contracts) fn note_loop_map_poison(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    left: &ForStatementLeft<'_>,
  ) {
    self.work.add_queries(1);
    let Some(target) = left.as_assignment_target() else {
      return;
    };
    if assignment_poisons_map_intrinsic(semantic, target, &self.work) {
      self.map_intrinsic_poisoned = true;
    }
  }

  pub(super) fn wrapper_expr_origin(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    expression: &Expression<'_>,
  ) -> WrapperOrigin {
    self.wrapper_expr_origin_from(semantic, kind, expression, 0)
  }

  pub(super) fn wrapper_expr_origin_from(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    expression: &Expression<'_>,
    depth: u8,
  ) -> WrapperOrigin {
    self.work.add_queries(1);
    if depth >= 32 {
      return WrapperOrigin::Unknown;
    }
    let inner = expression.get_inner_expression();
    if let Some(ident) = inner.get_identifier_reference() {
      return reference_symbol(semantic, ident)
        .map_or(WrapperOrigin::Unknown, |symbol_id| WrapperOrigin::Known(self.root_of(symbol_id)));
    }
    let Expression::CallExpression(call) = inner else {
      return WrapperOrigin::None;
    };
    let Some(api) = resolve_vue_api(
      &call.callee,
      &self.vue_imports,
      |ident| reference_symbol(semantic, ident),
      kind,
    ) else {
      return WrapperOrigin::None;
    };
    if !is_proxy_allocating_api(api)
      || !callee_has_actual_proxy_origin(&call.callee, &self.vue_imports, semantic, &self.work)
    {
      return WrapperOrigin::None;
    }
    if call.arguments.iter().any(Argument::is_spread) {
      return WrapperOrigin::Unknown;
    }
    let Some(arg) = call.arguments.first().and_then(Argument::as_expression) else {
      return WrapperOrigin::Unknown;
    };
    match self.wrapper_expr_origin_from(semantic, kind, arg, depth.saturating_add(1)) {
      WrapperOrigin::None => WrapperOrigin::Unknown,
      other => other,
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "assignment indexing needs owner, operator, and wrapper-origin kind together"
  )]
  pub(in crate::source_contracts) fn record_wrapper_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    node_id: NodeId,
    offset: usize,
    operator: AssignmentOperator,
    left: &AssignmentTarget<'_>,
    right: &Expression<'_>,
  ) {
    let simple = operator == AssignmentOperator::Assign;
    let fresh = simple && is_fresh_allocation(right, |ident| reference_symbol(semantic, ident));
    let owner = self.owner(node_id);
    let write = MemberWrite {
      offset,
      callable: owner.callable,
      block: owner.block.unwrap_or(node_id),
      rhs: right.span(),
      simple_assign: simple,
      fresh_alloc: fresh,
    };
    match left {
      AssignmentTarget::StaticMemberExpression(member) => {
        if member.object.get_inner_expression().get_identifier_reference().is_some() {
          return;
        }
        let property = member.property.name.as_str();
        match self.wrapper_expr_origin(semantic, kind, &member.object) {
          WrapperOrigin::Known(origin) => {
            self.pending_inline_writes.push((origin, property.to_string(), write));
          }
          WrapperOrigin::Unknown => self.unresolved_origin_touch = true,
          WrapperOrigin::None => {}
        }
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if member.object.get_inner_expression().get_identifier_reference().is_some() {
          return;
        }
        match self.wrapper_expr_origin(semantic, kind, &member.object) {
          WrapperOrigin::Known(origin) => self.pending_inline_unknown.push(origin),
          WrapperOrigin::Unknown => self.unresolved_origin_touch = true,
          WrapperOrigin::None => {}
        }
      }
      _ => {}
    }
  }
}

fn intern_map_method(name: &str) -> Option<&'static str> {
  match name {
    "get" => Some("get"),
    "set" => Some("set"),
    "has" => Some("has"),
    "delete" => Some("delete"),
    "clear" => Some("clear"),
    "forEach" => Some("forEach"),
    "keys" => Some("keys"),
    "values" => Some("values"),
    "entries" => Some("entries"),
    _ => None,
  }
}

pub(super) fn known_map_constructor_key_role(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
) -> bool {
  let parent = skip_ts_parent(semantic, node_id);
  let AstKind::ArrayExpression(array) = semantic.nodes().kind(parent) else {
    return false;
  };
  let Some(index) = array_element_index(array, semantic, node_id) else {
    return false;
  };
  index == 0 && array_is_map_entry(semantic, parent)
}

fn array_element_index(
  array: &ArrayExpression<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
) -> Option<usize> {
  let span = semantic.nodes().kind(node_id).span();
  array.elements.iter().enumerate().find_map(|(index, element)| {
    let expression = element.as_expression()?;
    (expression.span() == span || expression.get_inner_expression().span() == span).then_some(index)
  })
}

pub(super) fn array_is_map_entry(semantic: &oxc_semantic::Semantic<'_>, array_id: NodeId) -> bool {
  let parent = skip_ts_parent(semantic, array_id);
  let AstKind::ArrayExpression(_) = semantic.nodes().kind(parent) else {
    return false;
  };
  array_is_map_iterable(semantic, parent)
}

pub(super) fn array_is_map_iterable(
  semantic: &oxc_semantic::Semantic<'_>,
  array_id: NodeId,
) -> bool {
  let parent = skip_ts_parent(semantic, array_id);
  let AstKind::NewExpression(expression) = semantic.nodes().kind(parent) else {
    return false;
  };
  if unresolved_collection_name(&expression.callee, |ident| reference_symbol(semantic, ident))
    != Some("Map")
  {
    return false;
  }
  let Some(first) = expression.arguments.first().and_then(Argument::as_expression) else {
    return false;
  };
  let array_span = semantic.nodes().kind(array_id).span();
  first.span() == array_span || first.get_inner_expression().span() == array_span
}

fn unresolved_collection_name<'a>(
  callee: &'a Expression<'a>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> Option<&'static str> {
  let identifier = callee.get_inner_expression().get_identifier_reference()?;
  if symbol_of(identifier).is_some() {
    return None;
  }
  match identifier.name.as_str() {
    "Map" => Some("Map"),
    "Set" => Some("Set"),
    "WeakMap" => Some("WeakMap"),
    "WeakSet" => Some("WeakSet"),
    _ => None,
  }
}

fn map_constructor_keys(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &NewExpression<'_>,
  vue_imports: &HashMap<SymbolId, VueImport>,
  kind: ScriptKind,
  work: &WorkCounter,
) -> Option<Vec<MapKeyRef>> {
  if expression.arguments.iter().any(Argument::is_spread) {
    return None;
  }
  let Some(first) = expression.arguments.first() else {
    return Some(Vec::new());
  };
  let arg = first.as_expression()?;
  let Expression::ArrayExpression(array) = arg.get_inner_expression() else {
    return None;
  };
  let mut keys = Vec::new();
  for element in &array.elements {
    if element.is_elision() || matches!(element, ArrayExpressionElement::SpreadElement(_)) {
      return None;
    }
    let entry_expr = element.as_expression()?;
    let Expression::ArrayExpression(entry) = entry_expr.get_inner_expression() else {
      return None;
    };
    if entry
      .elements
      .iter()
      .any(|item| item.is_elision() || matches!(item, ArrayExpressionElement::SpreadElement(_)))
    {
      return None;
    }
    let key_expr = entry.elements.first().and_then(ArrayExpressionElement::as_expression)?;
    keys.push(extract_map_key_ref(semantic, key_expr, vue_imports, kind, work)?);
  }
  Some(keys)
}

fn extract_map_key_ref(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  vue_imports: &HashMap<SymbolId, VueImport>,
  kind: ScriptKind,
  work: &WorkCounter,
) -> Option<MapKeyRef> {
  let inner = expression.get_inner_expression();
  if let Some(identifier) = inner.get_identifier_reference() {
    return Some(MapKeyRef {
      span: identifier.span,
      symbol: reference_symbol(semantic, identifier),
      proxy_of: None,
      actual_proxy: false,
      proxy_flavor: None,
      is_literal: false,
    });
  }
  if let Expression::CallExpression(call) = inner {
    let api =
      resolve_vue_api(&call.callee, vue_imports, |ident| reference_symbol(semantic, ident), kind)?;
    let flavor = proxy_flavor(api);
    if flavor.is_none() || call.arguments.iter().any(Argument::is_spread) {
      return None;
    }
    let arg = call.arguments.first().and_then(Argument::as_expression)?;
    let ident = arg.get_inner_expression().get_identifier_reference()?;
    return Some(MapKeyRef {
      span: call.span,
      symbol: None,
      proxy_of: reference_symbol(semantic, ident),
      actual_proxy: callee_has_actual_proxy_origin(&call.callee, vue_imports, semantic, work),
      proxy_flavor: flavor,
      is_literal: false,
    });
  }
  match inner {
    Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::NullLiteral(_) => Some(MapKeyRef {
      span: inner.span(),
      symbol: None,
      proxy_of: None,
      actual_proxy: false,
      proxy_flavor: None,
      is_literal: true,
    }),
    _ => None,
  }
}

pub(in crate::source_contracts) fn proxy_flavor(api: &str) -> Option<ProxyFlavor> {
  match api {
    "reactive" => Some(ProxyFlavor::Deep),
    "shallowReactive" => Some(ProxyFlavor::Shallow),
    _ => None,
  }
}

pub(in crate::source_contracts) fn skip_ts_parent(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
) -> NodeId {
  let mut parent = semantic.nodes().parent_id(node_id);
  for _ in 0..8 {
    match semantic.nodes().kind(parent) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => parent = semantic.nodes().parent_id(parent),
      _ => return parent,
    }
  }
  parent
}

pub(super) fn assignment_poisons_map_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  left: &AssignmentTarget<'_>,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  match left {
    AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
      identifier_is_unresolved_map(semantic, identifier)
    }
    AssignmentTarget::StaticMemberExpression(member) => {
      global_map_member(semantic, &member.object, Some(member.property.name.as_str()), None)
        || map_prototype_assignment(semantic, &member.object, Some(member.property.name.as_str()))
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      global_map_member(semantic, &member.object, None, Some(&member.expression))
        || map_prototype_assignment(semantic, &member.object, None)
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      let properties = object.properties.iter().any(|property| match property {
        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
          identifier_is_unresolved_map(semantic, &property.binding)
        }
        AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
          maybe_default_poisons_map(semantic, &property.binding, work)
        }
      });
      let rest = object
        .rest
        .as_ref()
        .is_some_and(|rest| assignment_poisons_map_intrinsic(semantic, &rest.target, work));
      properties || rest
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      let elements = array
        .elements
        .iter()
        .flatten()
        .any(|element| maybe_default_poisons_map(semantic, element, work));
      let rest = array
        .rest
        .as_ref()
        .is_some_and(|rest| assignment_poisons_map_intrinsic(semantic, &rest.target, work));
      elements || rest
    }
    AssignmentTarget::TSAsExpression(inner) => {
      expression_poisons_map_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      expression_poisons_map_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      expression_poisons_map_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      expression_poisons_map_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::PrivateFieldExpression(_) => false,
  }
}

fn maybe_default_poisons_map(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTargetMaybeDefault<'_>,
  work: &WorkCounter,
) -> bool {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
      assignment_poisons_map_intrinsic(semantic, &with_default.binding, work)
    }
    other => other
      .as_assignment_target()
      .is_some_and(|inner| assignment_poisons_map_intrinsic(semantic, inner, work)),
  }
}

pub(super) fn simple_target_poisons_map_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &SimpleAssignmentTarget<'_>,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  match target {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
      identifier_is_unresolved_map(semantic, identifier)
    }
    SimpleAssignmentTarget::StaticMemberExpression(member) => {
      global_map_member(semantic, &member.object, Some(member.property.name.as_str()), None)
        || map_prototype_assignment(semantic, &member.object, Some(member.property.name.as_str()))
    }
    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
      global_map_member(semantic, &member.object, None, Some(&member.expression))
        || map_prototype_assignment(semantic, &member.object, None)
    }
    _ => false,
  }
}

pub(super) fn expression_poisons_map_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) => identifier_is_unresolved_map(semantic, identifier),
    Expression::StaticMemberExpression(member) => {
      global_map_member(semantic, &member.object, Some(member.property.name.as_str()), None)
        || map_prototype_assignment(semantic, &member.object, Some(member.property.name.as_str()))
    }
    Expression::ComputedMemberExpression(member) => {
      global_map_member(semantic, &member.object, None, Some(&member.expression))
        || map_prototype_assignment(semantic, &member.object, None)
    }
    _ => false,
  }
}

fn identifier_is_unresolved_map(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> bool {
  identifier.name.as_str() == "Map" && reference_symbol(semantic, identifier).is_none()
}

fn global_map_member(
  semantic: &oxc_semantic::Semantic<'_>,
  object: &Expression<'_>,
  static_key: Option<&str>,
  computed: Option<&Expression<'_>>,
) -> bool {
  let Some(ident) = object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  if reference_symbol(semantic, ident).is_some() {
    return false;
  }
  if !matches!(ident.name.as_str(), "globalThis" | "global" | "window") {
    return false;
  }
  if let Some(key) = static_key {
    return key == "Map";
  }
  let Some(computed) = computed else {
    return false;
  };
  match computed.get_inner_expression() {
    Expression::StringLiteral(literal) => literal.value.as_str() == "Map",
    _ => true,
  }
}

pub(super) fn vue_wrapper_skips_capability(
  call: &CallExpression<'_>,
  index: usize,
  calls: &HashMap<u64, CallInfo>,
) -> bool {
  if index != 0 {
    return false;
  }
  let Some(info) = calls.get(&span_key(call.span)) else {
    return false;
  };
  info.actual_proxy_origin
    && matches!(info.api, Some("reactive" | "readonly" | "shallowReactive" | "shallowReadonly"))
}

pub(super) fn capability_mutating_callee(
  semantic: &oxc_semantic::Semantic<'_>,
  callee: &Expression<'_>,
) -> bool {
  let Expression::StaticMemberExpression(member) = callee.get_inner_expression() else {
    return false;
  };
  let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  if reference_symbol(semantic, object).is_some() {
    return false;
  }
  matches!(
    (object.name.as_str(), member.property.name.as_str()),
    ("Object", "preventExtensions" | "seal" | "freeze" | "setPrototypeOf")
      | ("Reflect", "preventExtensions" | "setPrototypeOf")
  )
}

pub(super) fn object_has_skip_marker(object: &oxc_ast::ast::ObjectExpression<'_>) -> bool {
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return true,
      ObjectPropertyKind::ObjectProperty(property) => {
        let Some(name) = property.key.static_name() else {
          continue;
        };
        if matches!(&*name, "__v_skip" | "__v_raw") && !proven_falsy(&property.value) {
          return true;
        }
      }
    }
  }
  false
}

fn proven_falsy(expression: &Expression<'_>) -> bool {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => !literal.value,
    Expression::NullLiteral(_) => true,
    Expression::NumericLiteral(literal) => literal.value == 0.0,
    Expression::StringLiteral(literal) => literal.value.is_empty(),
    Expression::Identifier(identifier) => identifier.name.as_str() == "undefined",
    _ => false,
  }
}

fn is_map_ctor_expr(semantic: &oxc_semantic::Semantic<'_>, expression: &Expression<'_>) -> bool {
  let inner = expression.get_inner_expression();
  if let Some(identifier) = inner.get_identifier_reference() {
    return identifier_is_unresolved_map(semantic, identifier);
  }
  if let Expression::StaticMemberExpression(member) = inner {
    return global_map_member(semantic, &member.object, Some(member.property.name.as_str()), None);
  }
  if let Expression::ComputedMemberExpression(member) = inner {
    return global_map_member(semantic, &member.object, None, Some(&member.expression));
  }
  false
}

fn map_prototype_assignment(
  semantic: &oxc_semantic::Semantic<'_>,
  object: &Expression<'_>,
  static_key: Option<&str>,
) -> bool {
  let inner = object.get_inner_expression();
  if is_map_ctor_expr(semantic, inner) {
    return static_key.is_none_or(|key| key == "prototype" || intern_map_method(key).is_some());
  }
  let Expression::StaticMemberExpression(member) = inner else {
    return false;
  };
  member.property.name.as_str() == "prototype" && is_map_ctor_expr(semantic, &member.object)
}

fn node_matches_expr(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  expression: &Expression<'_>,
) -> bool {
  let span = semantic.nodes().kind(node_id).span();
  span == expression.span() || span == expression.get_inner_expression().span()
}

fn node_in_span(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId, span: Span) -> bool {
  let node_span = semantic.nodes().kind(node_id).span();
  node_span == span || (span.start <= node_span.start && node_span.end <= span.end)
}

pub(super) fn literal_truthy(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> Option<bool> {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(literal.value),
    Expression::NullLiteral(_) => Some(false),
    Expression::NumericLiteral(literal) => Some(literal.value != 0.0),
    Expression::StringLiteral(literal) => Some(!literal.value.is_empty()),
    Expression::ObjectExpression(_)
    | Expression::ArrayExpression(_)
    | Expression::FunctionExpression(_)
    | Expression::ArrowFunctionExpression(_)
    | Expression::NewExpression(_)
    | Expression::ClassExpression(_) => Some(true),
    Expression::Identifier(identifier)
      if identifier.name.as_str() == "undefined"
        && reference_symbol(semantic, identifier).is_none() =>
    {
      Some(false)
    }
    _ => None,
  }
}

pub(super) fn literal_nullish(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> Option<bool> {
  match expression.get_inner_expression() {
    Expression::NullLiteral(_) => Some(true),
    Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::ObjectExpression(_)
    | Expression::ArrayExpression(_)
    | Expression::FunctionExpression(_)
    | Expression::ArrowFunctionExpression(_)
    | Expression::NewExpression(_)
    | Expression::ClassExpression(_)
    | Expression::TemplateLiteral(_) => Some(false),
    Expression::Identifier(identifier)
      if identifier.name.as_str() == "undefined"
        && reference_symbol(semantic, identifier).is_none() =>
    {
      Some(true)
    }
    _ => None,
  }
}

const fn combine_exec(left: ExecKind, right: ExecKind) -> ExecKind {
  match (left, right) {
    (ExecKind::Never, _) | (_, ExecKind::Never) => ExecKind::Never,
    (ExecKind::Maybe, _) | (_, ExecKind::Maybe) => ExecKind::Maybe,
    (ExecKind::Always, ExecKind::Always) => ExecKind::Always,
  }
}

impl Indexes {
  pub(in crate::source_contracts) fn exec_kind(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut node_id: NodeId,
  ) -> ExecKind {
    let mut kind = ExecKind::Always;
    for _ in 0..32 {
      let parent = semantic.nodes().parent_id(node_id);
      match semantic.nodes().kind(parent) {
        AstKind::Program(_) | AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
          return kind;
        }
        AstKind::IfStatement(statement) => {
          if !node_matches_expr(semantic, node_id, &statement.test) {
            kind = combine_exec(kind, ExecKind::Maybe);
          }
          node_id = parent;
        }
        AstKind::LogicalExpression(logical) => {
          if node_matches_expr(semantic, node_id, &logical.left) {
            node_id = parent;
            continue;
          }
          match logical.operator {
            LogicalOperator::And | LogicalOperator::Or => {
              match (logical.operator, self.const_truthy(semantic, &logical.left)) {
                (LogicalOperator::And, Some(true)) | (LogicalOperator::Or, Some(false)) => {
                  node_id = parent;
                }
                (LogicalOperator::And, Some(false)) | (LogicalOperator::Or, Some(true)) => {
                  return ExecKind::Never;
                }
                _ => {
                  kind = combine_exec(kind, ExecKind::Maybe);
                  node_id = parent;
                }
              }
            }
            LogicalOperator::Coalesce => match self.const_nullish(semantic, &logical.left) {
              Some(true) => node_id = parent,
              Some(false) => return ExecKind::Never,
              None => {
                kind = combine_exec(kind, ExecKind::Maybe);
                node_id = parent;
              }
            },
          }
        }
        AstKind::ConditionalExpression(conditional) => {
          if node_matches_expr(semantic, node_id, &conditional.test) {
            node_id = parent;
            continue;
          }
          let taken_consequent = node_matches_expr(semantic, node_id, &conditional.consequent);
          match (self.const_truthy(semantic, &conditional.test), taken_consequent) {
            (Some(true), true) | (Some(false), false) => node_id = parent,
            (Some(true), false) | (Some(false), true) => return ExecKind::Never,
            _ => {
              kind = combine_exec(kind, ExecKind::Maybe);
              node_id = parent;
            }
          }
        }
        AstKind::AssignmentExpression(assignment) => {
          if node_in_span(semantic, node_id, assignment.left.span()) {
            node_id = parent;
            continue;
          }
          if !assignment.operator.is_logical() {
            node_id = parent;
            continue;
          }
          let Some(logical) = assignment.operator.to_logical_operator() else {
            return ExecKind::Maybe;
          };
          match self.assignment_target_logical_rhs(semantic, logical, &assignment.left) {
            ExecKind::Always => node_id = parent,
            ExecKind::Never => return ExecKind::Never,
            ExecKind::Maybe => {
              kind = combine_exec(kind, ExecKind::Maybe);
              node_id = parent;
            }
          }
        }
        AstKind::CallExpression(call) => {
          if node_matches_expr(semantic, node_id, &call.callee) {
            node_id = parent;
            continue;
          }
          match self.chain_child_exec(semantic, parent, &call.callee, call.optional) {
            ExecKind::Always => node_id = parent,
            ExecKind::Never => return ExecKind::Never,
            ExecKind::Maybe => {
              kind = combine_exec(kind, ExecKind::Maybe);
              node_id = parent;
            }
          }
        }
        AstKind::ComputedMemberExpression(member) => {
          if node_matches_expr(semantic, node_id, &member.object) {
            node_id = parent;
            continue;
          }
          match self.chain_child_exec(semantic, parent, &member.object, member.optional) {
            ExecKind::Always => node_id = parent,
            ExecKind::Never => return ExecKind::Never,
            ExecKind::Maybe => {
              kind = combine_exec(kind, ExecKind::Maybe);
              node_id = parent;
            }
          }
        }
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::ChainExpression(_)
        | AstKind::StaticMemberExpression(_)
        | AstKind::UnaryExpression(_)
        | AstKind::UpdateExpression(_)
        | AstKind::SequenceExpression(_)
        | AstKind::NewExpression(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::VariableDeclarator(_)
        | AstKind::VariableDeclaration(_)
        | AstKind::ReturnStatement(_)
        | AstKind::FunctionBody(_)
        | AstKind::BlockStatement(_)
        | AstKind::SpreadElement(_)
        | AstKind::ArrayExpression(_)
        | AstKind::ObjectExpression(_) => node_id = parent,
        _ => return ExecKind::Maybe,
      }
    }
    ExecKind::Maybe
  }

  pub(in crate::source_contracts) fn chain_child_exec(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    parent: NodeId,
    prefix: &Expression<'_>,
    immediate_optional: bool,
  ) -> ExecKind {
    if immediate_optional {
      return self.exec_if_not_nullish(semantic, prefix);
    }
    match self.containing_chain(semantic, parent) {
      ChainPlace::Inside => self.optional_prefix_exec(semantic, prefix),
      ChainPlace::Outside => ExecKind::Always,
      ChainPlace::Unknown => ExecKind::Maybe,
    }
  }

  pub(super) fn containing_chain(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut node_id: NodeId,
  ) -> ChainPlace {
    for _ in 0..16 {
      self.work.add_queries(1);
      let parent = semantic.nodes().parent_id(node_id);
      match semantic.nodes().kind(parent) {
        AstKind::ChainExpression(_) => return ChainPlace::Inside,
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_) => node_id = parent,
        AstKind::StaticMemberExpression(member)
          if node_matches_expr(semantic, node_id, &member.object) =>
        {
          node_id = parent;
        }
        AstKind::ComputedMemberExpression(member)
          if node_matches_expr(semantic, node_id, &member.object) =>
        {
          node_id = parent;
        }
        AstKind::PrivateFieldExpression(member)
          if node_matches_expr(semantic, node_id, &member.object) =>
        {
          node_id = parent;
        }
        AstKind::CallExpression(call) if node_matches_expr(semantic, node_id, &call.callee) => {
          node_id = parent;
        }
        AstKind::Program(_)
        | AstKind::Function(_)
        | AstKind::ArrowFunctionExpression(_)
        | AstKind::FunctionBody(_)
        | AstKind::BlockStatement(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::VariableDeclarator(_)
        | AstKind::VariableDeclaration(_)
        | AstKind::ReturnStatement(_)
        | AstKind::ThrowStatement(_)
        | AstKind::IfStatement(_)
        | AstKind::AssignmentExpression(_)
        | AstKind::SequenceExpression(_)
        | AstKind::UnaryExpression(_)
        | AstKind::UpdateExpression(_)
        | AstKind::LogicalExpression(_)
        | AstKind::ConditionalExpression(_)
        | AstKind::ArrayExpression(_)
        | AstKind::ObjectExpression(_)
        | AstKind::SpreadElement(_)
        | AstKind::NewExpression(_) => return ChainPlace::Outside,
        _ => return ChainPlace::Unknown,
      }
    }
    ChainPlace::Unknown
  }

  pub(in crate::source_contracts) fn optional_prefix_exec(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut current: &Expression<'_>,
  ) -> ExecKind {
    let mut kind = ExecKind::Always;
    for _ in 0..16 {
      self.work.add_queries(1);
      match current.get_inner_expression() {
        Expression::StaticMemberExpression(member) => {
          if member.optional {
            kind = combine_exec(kind, self.exec_if_not_nullish(semantic, &member.object));
            if kind == ExecKind::Never {
              return ExecKind::Never;
            }
          }
          current = &member.object;
        }
        Expression::ComputedMemberExpression(member) => {
          if member.optional {
            kind = combine_exec(kind, self.exec_if_not_nullish(semantic, &member.object));
            if kind == ExecKind::Never {
              return ExecKind::Never;
            }
          }
          current = &member.object;
        }
        Expression::PrivateFieldExpression(member) => {
          if member.optional {
            kind = combine_exec(kind, self.exec_if_not_nullish(semantic, &member.object));
            if kind == ExecKind::Never {
              return ExecKind::Never;
            }
          }
          current = &member.object;
        }
        Expression::CallExpression(call) => {
          if call.optional {
            kind = combine_exec(kind, self.exec_if_not_nullish(semantic, &call.callee));
            if kind == ExecKind::Never {
              return ExecKind::Never;
            }
          }
          current = &call.callee;
        }
        Expression::ChainExpression(_) | Expression::Identifier(_) => return kind,
        _ => return combine_exec(kind, ExecKind::Maybe),
      }
    }
    ExecKind::Maybe
  }

  pub(in crate::source_contracts) fn exec_if_not_nullish(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) -> ExecKind {
    match self.const_nullish(semantic, expression) {
      Some(true) => ExecKind::Never,
      Some(false) => ExecKind::Always,
      None => ExecKind::Maybe,
    }
  }

  pub(in crate::source_contracts) fn const_truthy(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) -> Option<bool> {
    if let Some(value) = literal_truthy(semantic, expression) {
      return Some(value);
    }
    if let Some(value) = self.ident_init_flag(semantic, expression, &self.known_truthy) {
      return Some(value);
    }
    self.known_flag(expression.span(), &self.known_truthy)
  }

  pub(in crate::source_contracts) fn const_nullish(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) -> Option<bool> {
    if let Some(value) = literal_nullish(semantic, expression) {
      return Some(value);
    }
    if let Some(value) = self.ident_init_flag(semantic, expression, &self.known_nullish) {
      return Some(value);
    }
    self.known_flag(expression.span(), &self.known_nullish)
  }

  pub(in crate::source_contracts) fn ident_init_flag(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    table: &HashMap<u64, bool>,
  ) -> Option<bool> {
    let identifier = expression.get_inner_expression().get_identifier_reference()?;
    let symbol_id = reference_symbol(semantic, identifier)?;
    let root = self.root_of(symbol_id);
    self.work.add_queries(1);
    if self.reassigned.contains(&root) {
      return None;
    }
    if !semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      return None;
    }
    let init = *self.init_span.get(&root)?;
    self.known_flag(init, table)
  }

  pub(in crate::source_contracts) fn assignment_target_logical_rhs(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    operator: LogicalOperator,
    target: &AssignmentTarget<'_>,
  ) -> ExecKind {
    let AssignmentTarget::AssignmentTargetIdentifier(identifier) = target else {
      return ExecKind::Maybe;
    };
    let Some(symbol_id) = reference_symbol(semantic, identifier) else {
      return ExecKind::Maybe;
    };
    let root = self.root_of(symbol_id);
    self.work.add_queries(1);
    if self.reassigned.contains(&root)
      || !semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable)
    {
      return ExecKind::Maybe;
    }
    let Some(init) = self.init_span.get(&root).copied() else {
      return ExecKind::Maybe;
    };
    match operator {
      LogicalOperator::And | LogicalOperator::Or => {
        match (operator, self.known_flag(init, &self.known_truthy)) {
          (LogicalOperator::And, Some(true)) | (LogicalOperator::Or, Some(false)) => {
            ExecKind::Always
          }
          (LogicalOperator::And, Some(false)) | (LogicalOperator::Or, Some(true)) => {
            ExecKind::Never
          }
          _ => ExecKind::Maybe,
        }
      }
      LogicalOperator::Coalesce => match self.known_flag(init, &self.known_nullish) {
        Some(true) => ExecKind::Always,
        Some(false) => ExecKind::Never,
        None => ExecKind::Maybe,
      },
    }
  }

  pub(in crate::source_contracts) fn known_flag(
    &self,
    mut span: Span,
    table: &HashMap<u64, bool>,
  ) -> Option<bool> {
    for _ in 0..8 {
      self.work.add_queries(1);
      if let Some(value) = table.get(&span_key(span)) {
        return Some(*value);
      }
      let hint = self.hints.get(&span_key(span)).copied()?;
      match hint {
        ShapeHint::Identifier(None, true) => {
          return table.get(&span_key(span)).copied();
        }
        ShapeHint::Identifier(Some(symbol_id), _) => {
          self.work.add_queries(1);
          span = *self.init_span.get(&self.root_of(symbol_id))?;
        }
        _ => return None,
      }
    }
    None
  }
}
