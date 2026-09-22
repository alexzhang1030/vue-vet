//! One-pass scan that fills `Indexes`. Query methods stay in `queries.rs`.

use super::{
  ArgUse, Argument, AssignmentOperator, AssignmentTarget, AssignmentTargetMaybeDefault,
  AssignmentTargetProperty, AstKind, AwaitClosedSource, AwaitEscapeSite, AwaitPositionSite,
  AwaitSite, BindingPattern, CallExpression, CallInfo, CallUse, DemandRole, DirectMemberWrite,
  DisposeSite, EffectCallback, Expression, ExtractedMethod, FormalParameters, FunctionInfo,
  GetSpan, HashMap, HashSet, IdentCall, IdentifierReference, Indexes, InjectionSite,
  MAX_ALIAS_DEPTH, MAX_ESCAPE_DEPTH, MAX_ROLE_ANCESTORS, MemberCall, MemberInfo, MemberUse,
  MemberWrite, NamedUse, NativeSymbol, NestedWrite, NodeId, ObjectEntry, ObjectPropertyKind,
  PathCall, PathRead, PathWrite, PrimitiveAtom, ReferenceFlags, ResultDemand, ScriptKind,
  ShapePrimitiveAtom, SimpleAssignmentTarget, Span, StaticMemberExpression, StmtSite, SymbolFlags,
  SymbolId, TOREF_CAPABILITY_KEY, UnaryOperator, ValueDemand, ValueRead, ValueWrite,
  VariableDeclarator, WatchConsumer, WriteLiteral, analyze_class, array_is_controlled_source,
  array_is_map_entry, array_is_map_iterable, assigned_const_symbol,
  assignment_poisons_clone_intrinsic, assignment_poisons_map_intrinsic, atom_of_expression,
  block_is_straight_line, boolean_literal, callee_has_actual_proxy_origin, callee_is_pause,
  callee_span_matches, capability_mutating_callee, chain_optional, class_binding_symbol,
  class_new_info, classify_reach, classify_reach_except_chain, classify_role,
  closed_key_use_is_known, computed_literal_key, enclosing_call,
  expression_poisons_clone_intrinsic, expression_poisons_map_intrinsic, expression_static_key,
  function_node, hint_of, intern_extractable_method, intern_native_ctor, intern_scope_method,
  intern_unresolved_global_name, is_capability_key, is_custom_prototype_key, is_fresh_allocation,
  is_known_constructor_or_watch, is_known_receiver_method, is_native_structured_clone,
  is_object_define_property, is_proxy_allocating_api, is_retained_vueuse_source_arg, is_ts_wrapper,
  is_unresolved_date, known_class_ctor_role, known_const_alias_role, known_injection_key_role,
  known_map_constructor_key_role, known_static_member_object_role, literal_nullish, literal_of,
  literal_truthy, mapped, member_call_object_key, member_is_write_context, nth_call_expr,
  object_entries, object_has_skip_marker, parse_watch_options, peel_member_chain,
  peel_static_chain, poisons_date_tojson, poisons_json, poisons_native_date,
  poisons_string_capability, primitive_atom, prototype_receiver_is_native_ctor, reference_symbol,
  region_of, resolve_vue_api, resolve_vueuse_api, scalar_of, simple_binding_symbol,
  simple_target_poisons_clone_intrinsic, simple_target_poisons_map_intrinsic, skip_ts_parent,
  span_key, static_string_key, stopped_child_symbol, summarize_object_props,
  unresolved_collection_kind, vue_wrapper_skips_capability, watch_option_flags, write_literal,
};

impl Indexes {
  #[expect(clippy::too_many_lines, reason = "one-pass statement kind dispatch")]
  pub(super) fn scan(
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
        AstKind::Class(class) => self.record_class(semantic, node_id, class),
        AstKind::StaticMemberExpression(member) => {
          self.record_static_member(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            member,
          );
          let scheduling_read = self.member_value_is_read(semantic, node_id, member.span);
          self.record_lane_value_reads(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            member,
            scheduling_read,
          );
          self.index_static_member(semantic, kind, member);
        }
        AstKind::VariableDeclarator(declarator) => {
          if matches!(
            semantic.nodes().parent_kind(semantic.nodes().parent_id(node_id)),
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
          ) && let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
            && let Some(symbol_id) = binding.symbol_id.get()
          {
            let root = self.root_of(symbol_id);
            self.escape_root(root, true);
            self.closed_key_unknown.insert(root);
            self.capability_poisoned.insert(root);
          }
          if let Some(init) = &declarator.init {
            self.record_expr(semantic, kind, init);
            if let Some(ident) = init.get_inner_expression().get_identifier_reference()
              && let Some(target) = reference_symbol(semantic, ident)
            {
              match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(binding) => {
                  if let Some(local) = binding.symbol_id.get() {
                    let root = self.root_of(target);
                    if semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
                      self.alias_root.insert(local, root);
                    } else {
                      self.escape_root(root, true);
                      self.capability_poisoned.insert(root);
                    }
                  }
                }
                BindingPattern::ObjectPattern(_) => {
                  self.escape_root(self.root_of(target), true);
                }
                _ => {
                  let root = self.root_of(target);
                  self.escape_root(root, true);
                  self.capability_poisoned.insert(root);
                }
              }
            }
          }
          if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
            && let Some(symbol_id) = binding.symbol_id.get()
            && let Some(init) = &declarator.init
          {
            self.init_span.insert(symbol_id, init.span());
            self.record_native_symbol(
              semantic,
              line_index,
              sfc_source,
              script_offset,
              node_id,
              symbol_id,
              init,
            );
            self
              .init_offset
              .insert(symbol_id, mapped(line_index, sfc_source, script_offset, init.span()).offset);
            match init.get_inner_expression() {
              Expression::CallExpression(call) => {
                self.call_results.insert(span_key(call.span), symbol_id);
              }
              Expression::AwaitExpression(awaited) => {
                self.await_index.results_by_await.insert(span_key(awaited.span), symbol_id);
              }
              _ => {}
            }
            if semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable)
              && let Some((ident, keys)) = peel_static_chain(init, &self.work)
              && keys.as_slice() == ["value"]
              && let Some(target) = reference_symbol(semantic, ident)
            {
              self.value_object_alias.insert(symbol_id, self.root_of(target));
            }
          }
          if let Some(init) = &declarator.init {
            self.record_destructure(semantic, &declarator.id, init);
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
        AstKind::AssignmentExpression(assignment) => {
          self.record_expr(semantic, kind, &assignment.right);
          self.mark_escape_expr(semantic, &assignment.right, true);
          self.poison_expr(semantic, &assignment.right);
          self.taint_assignment_target(semantic, &assignment.left);
          if assignment_poisons_map_intrinsic(semantic, &assignment.left, &self.work) {
            self.map_intrinsic_poisoned = true;
          }
          let offset = mapped(line_index, sfc_source, script_offset, assignment.span).offset;
          self.index_assignment(
            semantic,
            node_id,
            offset,
            assignment.operator,
            &assignment.left,
            &assignment.right,
          );
          self.record_wrapper_assignment(
            semantic,
            kind,
            node_id,
            offset,
            assignment.operator,
            &assignment.left,
            &assignment.right,
          );
          self.record_stmt_site(semantic, line_index, sfc_source, script_offset, node_id);
        }
        AstKind::ForInStatement(statement) => {
          self.note_loop_assignment_poison(semantic, &statement.left);
          self.note_loop_map_poison(semantic, &statement.left);
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ForOfStatement(statement) => {
          self.note_loop_assignment_poison(semantic, &statement.left);
          self.note_loop_map_poison(semantic, &statement.left);
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::UpdateExpression(update) => {
          self.poison_simple_target(semantic, &update.argument);
          if simple_target_poisons_map_intrinsic(semantic, &update.argument, &self.work) {
            self.map_intrinsic_poisoned = true;
          }
          if simple_target_poisons_clone_intrinsic(semantic, &update.argument, &self.work) {
            self.clone_intrinsic_poisoned = true;
          }
          match &update.argument {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
              if let Some(symbol_id) = reference_symbol(semantic, identifier) {
                self.reassigned.insert(self.root_of(symbol_id));
              }
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
              if member.property.name.as_str() == "value"
                && let Some(object) =
                  member.object.get_inner_expression().get_identifier_reference()
                && let Some(symbol_id) = reference_symbol(semantic, object)
              {
                let root = self.root_of(symbol_id);
                let owner = self.owner(node_id);
                let offset = mapped(line_index, sfc_source, script_offset, update.span).offset;
                self.value_write_roots.insert(root);
                let event = ValueWrite {
                  offset,
                  callable: owner.callable,
                  block: owner.block.unwrap_or(node_id),
                  span: update.span,
                  rhs: update.span,
                  literal: WriteLiteral::Other,
                  simple_assign: false,
                  fresh_alloc: false,
                  node_id,
                };
                self.value_writes.entry(root).or_default().push(event);
                self.value_write_events.entry(root).or_default().push(event);
              }
            }
            _ => {}
          }
        }
        AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
          self.poison_member_expression(semantic, &unary.argument);
          self.note_delete(semantic, &unary.argument);
          if expression_poisons_map_intrinsic(semantic, &unary.argument, &self.work) {
            self.map_intrinsic_poisoned = true;
          }
        }
        AstKind::Function(function) => {
          self.record_function(node_id, function.span, &function.params, false);
          if function.r#async {
            self.async_callables.insert(node_id);
          }
        }
        AstKind::ArrowFunctionExpression(arrow) => {
          self.record_function(node_id, arrow.span, &arrow.params, arrow.expression);
          if arrow.r#async {
            self.async_callables.insert(node_id);
          }
        }
        AstKind::CallExpression(call) => {
          self.record_call(semantic, kind, call);
          self.call_nodes.insert(span_key(call.span), node_id);
          self.record_map_member_call(
            semantic,
            kind,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            call,
          );
          self.record_call_uses(semantic, line_index, sfc_source, script_offset, node_id, call);
          let info = self.calls.get(&span_key(call.span)).copied();
          if info.is_some_and(|call_info| call_info.vueuse.is_some()) {
            self
              .producer_call_offsets
              .push(mapped(line_index, sfc_source, script_offset, call.span).offset);
          }
          let api = info.and_then(|call_info| call_info.api);
          for (index, argument) in call.arguments.iter().enumerate() {
            if let Some(expression) = argument.as_expression() {
              self.record_expr(semantic, kind, expression);
              let until_borrow = index == 0
                && info.is_some_and(|call_info| {
                  call_info.vueuse == Some("until") && !call_info.has_spread
                });
              if !is_retained_vueuse_source_arg(info, index) {
                self.await_index.pending_borrow = until_borrow;
                let skip_injection_key = matches!(api, Some("provide" | "inject")) && index == 0;
                if !skip_injection_key {
                  self.mark_escape_expr(
                    semantic,
                    expression,
                    !(api == Some("toRef") && index == 0),
                  );
                }
                self.await_index.pending_borrow = false;
              }
              if capability_mutating_callee(semantic, &call.callee) {
                self.poison_expr(semantic, expression);
                self.mark_helper_escape_expr(semantic, expression);
              } else if api.is_none() {
                if let Some(pending) =
                  self.pending_native_map_key_arg(semantic, call, index, expression)
                {
                  self.pending_map_key_args.push(pending);
                } else if !is_retained_vueuse_source_arg(info, index) {
                  self.poison_expr(semantic, expression);
                  self.mark_helper_escape_expr(semantic, expression);
                }
              } else if !(vue_wrapper_skips_capability(call, index, &self.calls)
                || matches!(api, Some("provide" | "inject")) && index == 0)
              {
                self.mark_helper_escape_expr(semantic, expression);
              }
            }
          }
          if matches!(api, Some("provide" | "inject")) {
            self.record_injection(semantic, line_index, sfc_source, script_offset, node_id, call);
          }
          self.note_receiver_use(semantic, &call.callee);
          self.record_stmt_site(semantic, line_index, sfc_source, script_offset, node_id);
          self.record_event(semantic, line_index, sfc_source, script_offset, node_id, call.span);
          self.record_member_call(semantic, line_index, sfc_source, script_offset, node_id, call);
          self.record_identifier_call(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            call,
          );
          self.record_result_demand(semantic, line_index, sfc_source, script_offset, node_id, call);
          if callee_is_pause(call) {
            self.record_pause_event(node_id, line_index, sfc_source, script_offset, call.span);
          }
          self.record_scheduling_member_call(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            call,
          );
        }
        AstKind::NewExpression(expression) => {
          self.record_expr(semantic, kind, &expression.callee);
          self.note_receiver_use(semantic, &expression.callee);
          self.record_map_new(semantic, kind, expression);
          let skip_map_args =
            self.news.get(&span_key(expression.span)).is_some_and(|info| info.ctor == Some("Map"));
          for argument in &expression.arguments {
            if let Some(arg) = argument.as_expression() {
              self.record_expr(semantic, kind, arg);
              if !skip_map_args {
                self.mark_escape_expr(semantic, arg, true);
                self.poison_expr(semantic, arg);
              }
            }
          }
          if let Some(ctor) = unresolved_collection_kind(&expression.callee, |ident| {
            reference_symbol(semantic, ident)
          }) {
            self.collections.insert(span_key(expression.span), ctor);
          }
          if is_unresolved_date(&expression.callee, |ident| reference_symbol(semantic, ident)) {
            self.dates.insert(span_key(expression.span));
          }
          self.record_class_new(semantic, expression);
        }
        AstKind::TaggedTemplateExpression(tagged) => {
          self.note_receiver_use(semantic, &tagged.tag);
          for expression in &tagged.quasi.expressions {
            self.mark_escape_expr(semantic, expression, true);
            self.poison_expr(semantic, expression);
          }
        }
        AstKind::ObjectExpression(object) => {
          self.literal_span.insert(span_key(object.span), object.span);
          self.object_literals.insert(span_key(object.span));
          if object_has_skip_marker(object) {
            self.skip_marker_objects.insert(span_key(object.span));
          }
          let entries = object_entries(object);
          let props = summarize_object_props(&entries, &self.work);
          self.object_props.insert(span_key(object.span), props);
          self.objects.insert(span_key(object.span), entries);
          for property in &object.properties {
            if let ObjectPropertyKind::ObjectProperty(property) = property {
              self.record_expr(semantic, kind, &property.value);
              self.mark_escape_expr(semantic, &property.value, true);
              self.poison_expr(semantic, &property.value);
            }
          }
        }
        AstKind::ArrayExpression(array) => {
          self.index_array_expression(semantic, kind, node_id, array);
        }
        AstKind::ReturnStatement(statement) => {
          if let Some(argument) = &statement.argument {
            self.record_expr(semantic, kind, argument);
            self.mark_escape_expr(semantic, argument, true);
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
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier_end(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ThrowStatement(statement) => {
          self.record_expr(semantic, kind, &statement.argument);
          self.mark_escape_expr(semantic, &statement.argument, true);
          self.poison_expr(semantic, &statement.argument);
          self.record_termination(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier_end(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::SpreadElement(spread) => {
          self.mark_escape_expr(semantic, &spread.argument, true);
          self.poison_expr(semantic, &spread.argument);
        }
        AstKind::IfStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ForStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::WhileStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::SwitchStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::TryStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::StringLiteral(literal) => {
          self.store_interned_atom(literal.span, literal.value.as_str(), false);
        }
        AstKind::BigIntLiteral(literal) => {
          if let Some(raw) = literal.raw.as_deref() {
            self.store_interned_atom(literal.span, raw, true);
          }
        }
        AstKind::TemplateLiteral(literal) if literal.expressions.is_empty() => {
          if let Some(cooked) =
            literal.quasis.first().and_then(|quasi| quasi.value.cooked.as_deref())
          {
            self.store_interned_atom(literal.span, cooked, false);
          }
        }
        AstKind::BooleanLiteral(literal) => {
          self.store_atom(literal.span, ShapePrimitiveAtom::Bool(literal.value));
        }
        AstKind::NumericLiteral(literal) => {
          self.store_atom(
            literal.span,
            ShapePrimitiveAtom::Number {
              bits: literal.value.to_bits(),
              nan: literal.value.is_nan(),
            },
          );
        }
        AstKind::NullLiteral(literal) => self.store_atom(literal.span, ShapePrimitiveAtom::Null),
        AstKind::IdentifierReference(identifier)
          if identifier.name.as_str() == "undefined"
            && reference_symbol(semantic, identifier).is_none() =>
        {
          self.store_atom(identifier.span, ShapePrimitiveAtom::Undefined);
        }
        AstKind::UnaryExpression(unary)
          if matches!(unary.operator, UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus) =>
        {
          match unary.operator {
            UnaryOperator::UnaryNegation => {
              if let Some(ShapePrimitiveAtom::Number { bits, nan: false }) =
                primitive_atom(&unary.argument)
              {
                self.store_atom(
                  unary.span,
                  ShapePrimitiveAtom::Number {
                    bits: (-f64::from_bits(bits)).to_bits(),
                    nan: false,
                  },
                );
              }
            }
            UnaryOperator::UnaryPlus => {
              if let Some(atom) = primitive_atom(&unary.argument) {
                self.store_atom(unary.span, atom);
              }
            }
            _ => {}
          }
        }
        AstKind::AwaitExpression(expression) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, expression.span);
          self.record_await(
            semantic,
            kind,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            expression.span,
            &expression.argument,
          );
        }
        AstKind::YieldExpression(expression) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, expression.span);
        }
        AstKind::BreakStatement(statement) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ContinueStatement(statement) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
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

  pub(super) fn finish_aliases_and_roles(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
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
        let node_id = reference.node_id();
        let member_payload = flags.intersects(ReferenceFlags::MemberWriteTarget)
          || known_static_member_object_role(semantic, node_id, &self.work);
        if !closed_key_use_is_known(
          semantic,
          node_id,
          flags,
          member_payload,
          &self.calls,
          &self.work,
        ) {
          self.closed_key_unknown.insert(root);
        }
        if member_payload {
          if flags.is_write() && !self.has_simple_value_write(root) && !self.has_member_write(root)
          {
            self.uncertain.insert(root);
          }
          if known_static_member_object_role(semantic, reference.node_id(), &self.work)
            && self.static_member_chain_receiver_uncertain(semantic, reference.node_id())
          {
            self.toref_identity_uncertain.insert(root);
            self.capability_uncertain.insert(root);
            self.closed_key_unknown.insert(root);
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
        if known_injection_key_role(semantic, reference.node_id(), &self.calls, &self.work) {
          continue;
        }
        if known_class_ctor_role(semantic, reference.node_id(), &self.classes, root) {
          continue;
        }
        if known_map_constructor_key_role(semantic, reference.node_id(), &self.work) {
          continue;
        }
        self.uncertain.insert(root);
        if !self.known_toref_source_argument_role(semantic, reference.node_id()) {
          self.toref_identity_uncertain.insert(root);
        }
        if !self.known_vue_source_argument_role(semantic, reference.node_id()) {
          self.capability_uncertain.insert(root);
        }
      }
    }
    if !leftover.is_empty() {
      self.apply_unresolved_global_escape_coverage(&leftover);
    }
  }

  /// First argument of a proven `toRef` call. Other identifier uses do not prove `__v_isRef`.
  pub(super) fn known_toref_source_argument_role(
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
        wrapper if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::ChainExpression(_)) => {
          current = parent_id;
        }
        AstKind::CallExpression(call) => {
          let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
            return false;
          };
          if info.api != Some("toRef") {
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

  /// Receiver call / `new` / tagged template / unknown member-chain of a static
  /// member object, including TypeScript instantiation wrappers. Ordinary
  /// assigned-property reads and writes stay outside this set. Import sources,
  /// JSX member tags, and decorator expressions stay on the default (proven)
  /// branch; those positions bind no JS `this` receiver. Exhausted ancestor
  /// budgets stay unproven.
  pub(super) fn static_member_chain_receiver_uncertain(
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
        wrapper if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::ChainExpression(_)) => {
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

  pub(super) fn known_vue_source_argument_role(
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
        wrapper if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::ChainExpression(_)) => {
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

  pub(super) fn has_simple_value_write(&self, root: SymbolId) -> bool {
    self.value_write_roots.contains(&root)
  }

  pub(super) fn has_member_write(&self, root: SymbolId) -> bool {
    self.member_write_roots.contains(&root)
  }

  pub(super) fn index_static_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    member: &StaticMemberExpression<'_>,
  ) {
    self.record_expr(semantic, kind, &member.object);
    self.members.insert(
      span_key(member.span),
      MemberInfo {
        object: member.object.get_inner_expression().span(),
        property: member.property.name.to_string(),
        span: member.span,
      },
    );
  }

  pub(super) fn record_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    expression: &Expression<'_>,
  ) {
    let inner = expression.get_inner_expression();
    self
      .hints
      .insert(span_key(inner.span()), hint_of(inner, |ident| reference_symbol(semantic, ident)));
    if let Some(atom) = atom_of_expression(inner, &self.work, super::MAX_DEPTH) {
      self.atoms.insert(span_key(inner.span()), atom);
    } else if let Expression::Identifier(identifier) = inner
      && reference_symbol(semantic, identifier).is_none()
      && let Some(atom) = PrimitiveAtom::unresolved_global(identifier.name.as_str())
    {
      self.work.add_queries(1);
      self.atoms.insert(span_key(inner.span()), atom);
    }
    self.hints.insert(
      span_key(expression.span()),
      hint_of(inner, |ident| reference_symbol(semantic, ident)),
    );
    if matches!(inner, Expression::ObjectExpression(_) | Expression::ArrayExpression(_)) {
      self.literal_span.insert(span_key(expression.span()), inner.span());
      self.literal_span.insert(span_key(inner.span()), inner.span());
    }
    if let Some(truthy) = literal_truthy(semantic, inner) {
      self.known_truthy.insert(span_key(inner.span()), truthy);
      self.known_truthy.insert(span_key(expression.span()), truthy);
    }
    if let Some(nullish) = literal_nullish(semantic, inner) {
      self.known_nullish.insert(span_key(inner.span()), nullish);
      self.known_nullish.insert(span_key(expression.span()), nullish);
    }
    if matches!(inner, Expression::ObjectExpression(_)) {
      self.object_literals.insert(span_key(inner.span()));
      self.object_literals.insert(span_key(expression.span()));
    }
    if let Some(atom) = primitive_atom(inner) {
      self.store_atom(inner.span(), atom);
      self.store_atom(expression.span(), atom);
    }
    if let Some(scalar) = scalar_of(inner) {
      self.await_index.scalars.insert(span_key(inner.span()), scalar);
      self.await_index.scalars.insert(span_key(expression.span()), scalar);
    }
    if let Some(literal) = literal_of(inner) {
      self.literals.insert(span_key(inner.span()), literal);
      self.literals.insert(span_key(expression.span()), literal);
    }
    self.intern_expr(semantic, inner);
    if let Expression::CallExpression(call) = inner {
      self.record_call(semantic, kind, call);
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "one static-member visit fills custom-ref, derivation, and scheduling lanes"
  )]
  pub(super) fn record_lane_value_reads(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    member: &oxc_ast::ast::StaticMemberExpression<'_>,
    scheduling_read: bool,
  ) {
    if member.property.name.as_str() != "value" {
      return;
    }
    let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let owner = self.owner(node_id);
    let read = ValueRead {
      node_id,
      offset: mapped(line_index, sfc_source, script_offset, member.span).offset,
      span: member.span,
      callable: owner.callable,
      block: owner.block.unwrap_or(node_id),
    };
    // customRef keeps the pre-alias symbol; remap_roots canonicalizes later.
    // A write context is not a customRef read. Derivation skips every assignment
    // and update parent, including a read on the right-hand side. Scheduling uses
    // the caller-computed `member_value_is_read` predicate.
    if !member_is_write_context(semantic, node_id, member.span) {
      self.reads.record_custom_ref(symbol_id, read);
    }
    match semantic.nodes().parent_kind(node_id) {
      AstKind::AssignmentExpression(_) | AstKind::UpdateExpression(_) => {}
      _ => {
        self.work.add_writes(1);
        let root = self.root_of(symbol_id);
        self.reads.record_derivation(root, read);
      }
    }
    if scheduling_read {
      self.work.add_writes(1);
      let root = self.root_of(symbol_id);
      self.reads.record_scheduling(root, read);
    }
  }

  pub(super) fn intern(&mut self, value: &str) -> u32 {
    self.work.add_queries(1);
    if let Some(index) = self.interned.iter().position(|existing| {
      self.work.add_queries(1);
      existing == value
    }) {
      return u32::try_from(index).unwrap_or(u32::MAX);
    }
    let id = u32::try_from(self.interned.len()).unwrap_or(u32::MAX);
    self.interned.push(value.to_string());
    self.work.add_queries(1);
    id
  }

  pub(super) fn store_atom(&mut self, span: Span, atom: ShapePrimitiveAtom) {
    self.work.add_queries(1);
    self.primitives.insert(span_key(span), atom);
  }

  pub(super) fn store_interned_atom(&mut self, span: Span, value: &str, bigint: bool) {
    let id = self.intern(value);
    self.store_atom(
      span,
      if bigint { ShapePrimitiveAtom::BigInt(id) } else { ShapePrimitiveAtom::Str(id) },
    );
  }

  pub(super) fn intern_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    self.work.add_queries(1);
    match expression.get_inner_expression() {
      Expression::StringLiteral(literal) => {
        self.store_interned_atom(literal.span, literal.value.as_str(), false);
        self.store_interned_atom(expression.span(), literal.value.as_str(), false);
      }
      Expression::BigIntLiteral(literal) => {
        if let Some(raw) = literal.raw.as_deref() {
          self.store_interned_atom(literal.span, raw, true);
          self.store_interned_atom(expression.span(), raw, true);
        }
      }
      Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
        if let Some(cooked) = literal.quasis.first().and_then(|quasi| quasi.value.cooked.as_deref())
        {
          self.store_interned_atom(literal.span, cooked, false);
          self.store_interned_atom(expression.span(), cooked, false);
        }
      }
      Expression::Identifier(identifier)
        if identifier.name.as_str() == "undefined"
          && reference_symbol(semantic, identifier).is_none() =>
      {
        self.store_atom(identifier.span, ShapePrimitiveAtom::Undefined);
        self.store_atom(expression.span(), ShapePrimitiveAtom::Undefined);
      }
      Expression::UnaryExpression(unary)
        if matches!(unary.operator, UnaryOperator::UnaryNegation) =>
      {
        self.intern_expr(semantic, &unary.argument);
        if let Some(ShapePrimitiveAtom::Number { bits, nan: false }) =
          self.primitives.get(&span_key(unary.argument.span())).copied()
        {
          self.store_atom(
            unary.span,
            ShapePrimitiveAtom::Number { bits: (-f64::from_bits(bits)).to_bits(), nan: false },
          );
        }
      }
      Expression::UnaryExpression(unary) if matches!(unary.operator, UnaryOperator::UnaryPlus) => {
        self.intern_expr(semantic, &unary.argument);
        if let Some(atom) = self.primitives.get(&span_key(unary.argument.span())).copied() {
          self.store_atom(unary.span, atom);
          self.store_atom(expression.span(), atom);
        }
      }
      _ => {}
    }
  }

  pub(super) fn member_value_is_read(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    member_span: Span,
  ) -> bool {
    self.work.add_queries(1);
    match semantic.nodes().parent_kind(node_id) {
      AstKind::UpdateExpression(_) => false,
      AstKind::AssignmentExpression(assignment) => {
        let right = assignment.right.span();
        if right.start <= member_span.start && member_span.end <= right.end {
          return true;
        }
        assignment.operator != AssignmentOperator::Assign
          && assignment.left.span().start <= member_span.start
          && member_span.end <= assignment.left.span().end
      }
      _ => true,
    }
  }

  pub(super) fn record_scheduling_member_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return;
    };
    let Some(method) = intern_scope_method(member.property.name.as_str()) else {
      return;
    };
    let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let owner = self.owner(node_id);
    self.work.add_writes(1);
    let root = self.root_of(symbol_id);
    self.reads.record_scheduling_call(
      root,
      MemberCall {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        method,
        span: call.span,
        callable: owner.callable,
        block: owner.block.unwrap_or(node_id),
      },
    );
  }

  #[expect(clippy::too_many_arguments, reason = "await indexing needs owner, span, and callee")]
  pub(super) fn record_await(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
    argument: &Expression<'_>,
  ) {
    self.record_event(semantic, line_index, sfc_source, script_offset, node_id, span);
    let inner = argument.get_inner_expression();
    let callee_api = match inner {
      Expression::CallExpression(call) => resolve_vue_api(
        &call.callee,
        &self.vue_imports,
        |ident| reference_symbol(semantic, ident),
        kind,
      ),
      _ => None,
    };
    let owner = self.owner(node_id);
    self.work.add_writes(1);
    let site = AwaitSite {
      offset: mapped(line_index, sfc_source, script_offset, span).offset,
      callee_api,
      callable: owner.callable,
      block: owner.block.unwrap_or(node_id),
    };
    self.awaits.push(site);
    self.work.add_writes(1);
    self.awaits_by_callable.entry(owner.callable).or_default().push(site);
    let await_span = mapped(line_index, sfc_source, script_offset, span);
    let argument = argument.get_inner_expression();
    let bound =
      argument.get_identifier_reference().and_then(|ident| reference_symbol(semantic, ident));
    let region = region_of(owner, node_id);
    self.await_index.awaits.push(AwaitPositionSite {
      offset: await_span.offset,
      end: await_span.offset.saturating_add(await_span.length),
      span,
      argument: argument.span(),
      bound,
      callable: owner.callable,
      region,
      reach: classify_reach(semantic, node_id, &self.work),
    });
  }

  pub(super) fn finish_practice_indexes(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
  ) {
    let node_ids: Vec<NodeId> = self.call_nodes.values().copied().collect();
    self.work.add_queries(node_ids.len() as u64);
    for node_id in node_ids {
      let AstKind::CallExpression(call) = semantic.nodes().kind(node_id) else {
        continue;
      };
      self.work.add_queries(1);
      let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
        continue;
      };
      self.bind_practice_call(
        semantic,
        line_index,
        sfc_source,
        script_offset,
        kind,
        node_id,
        call,
        info,
      );
    }
  }

  #[expect(clippy::too_many_arguments, reason = "practice index bind needs owner, span, and call")]
  pub(super) fn bind_practice_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    let owner = self.owner(node_id);
    let offset = mapped(line_index, sfc_source, script_offset, call.span).offset;
    match info.api {
      Some("onScopeDispose") => {
        let child = nth_call_expr(call, 0).and_then(|expression| {
          stopped_child_symbol(semantic, expression, |ident| reference_symbol(semantic, ident))
        });
        let site = DisposeSite { offset, span: call.span, child, callable: owner.callable };
        self.work.add_writes(1);
        self.disposals_by_callable.entry(owner.callable).or_default().push(site);
      }
      Some("watch") => {
        let Some(source_expr) = nth_call_expr(call, 0) else {
          return;
        };
        let Some(ident) = source_expr.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(symbol_id) = reference_symbol(semantic, ident) else {
          return;
        };
        let (once, immediate, options_unknown) = watch_option_flags(nth_call_expr(call, 2));
        let handle = assigned_const_symbol(semantic, node_id);
        let consumer = WatchConsumer {
          offset,
          span: call.span,
          callable: owner.callable,
          source: self.root_of(symbol_id),
          once,
          immediate,
          options_unknown,
          handle,
        };
        self.work.add_writes(1);
        self.watches_by_source.entry(consumer.source).or_default().push(consumer);
        if let Some(callback) = nth_call_expr(call, 1)
          && let Some(callback_id) = function_node(callback, |span| {
            self.work.add_queries(1);
            self.callables.get(&span_key(span)).copied()
          })
        {
          self.work.add_writes(1);
          self.effect_callbacks.insert(callback_id, EffectCallback { offset, api: "watch" });
        }
      }
      Some("watchEffect" | "watchPostEffect" | "watchSyncEffect") => {
        let Some(api) = info.api else {
          return;
        };
        if let Some(callback) = nth_call_expr(call, 0)
          && let Some(callback_id) = function_node(callback, |span| {
            self.work.add_queries(1);
            self.callables.get(&span_key(span)).copied()
          })
        {
          self.work.add_writes(1);
          self.effect_callbacks.insert(callback_id, EffectCallback { offset, api });
        }
      }
      _ => {}
    }
    let _ = kind;
    if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
      && member.property.name.as_str() == "run"
      && let Some(object) = member.object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, object)
      && let Some(callback) = nth_call_expr(call, 0)
      && let Some(callback_id) = function_node(callback, |span| {
        self.work.add_queries(1);
        self.callables.get(&span_key(span)).copied()
      })
    {
      self.work.add_writes(1);
      self.run_callback_scope.insert(callback_id, self.root_of(symbol_id));
    }
  }

  pub(super) fn record_call(
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
    let vueuse = resolve_vueuse_api(&call.callee, &self.vueuse_imports, |ident| {
      reference_symbol(semantic, ident)
    });
    let first_arg = if has_spread {
      None
    } else {
      call.arguments.first().and_then(Argument::as_expression).map(GetSpan::span)
    };
    let second_arg = if has_spread {
      None
    } else {
      call.arguments.get(1).and_then(Argument::as_expression).map(GetSpan::span)
    };
    let native_structured_clone =
      !call.optional && is_native_structured_clone(&call.callee, semantic);
    let actual_proxy_origin = api.is_some_and(is_proxy_allocating_api)
      && callee_has_actual_proxy_origin(&call.callee, &self.vue_imports, semantic, &self.work);
    let arg_count = u8::try_from(call.arguments.len()).unwrap_or(u8::MAX);
    let third_arg = if has_spread {
      None
    } else {
      call.arguments.get(2).and_then(Argument::as_expression).map(GetSpan::span)
    };
    self.calls.insert(
      span_key(call.span),
      CallInfo {
        span: call.span,
        api,
        vueuse,
        first_arg,
        second_arg,
        has_spread,
        native_structured_clone,
        actual_proxy_origin,
        arg_count,
        third_arg,
      },
    );
    if is_object_define_property(&call.callee) {
      self.prototype_mutated = true;
    }
  }

  pub(super) fn note_delete(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    argument: &Expression<'_>,
  ) {
    if expression_poisons_clone_intrinsic(semantic, argument, &self.work) {
      self.clone_intrinsic_poisoned = true;
    }
    self.mark_delete_target(semantic, argument);
  }

  pub(super) fn index_array_expression(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    node_id: NodeId,
    array: &oxc_ast::ast::ArrayExpression<'_>,
  ) {
    self.arrays.insert(span_key(array.span));
    self.literal_span.insert(span_key(array.span), array.span);
    if array
      .elements
      .iter()
      .any(|element| matches!(element, oxc_ast::ast::ArrayExpressionElement::SpreadElement(_)))
    {
      self.array_spread.insert(span_key(array.span));
    }
    let map_entry = array_is_map_entry(semantic, node_id, &self.work);
    let map_iterable = array_is_map_iterable(semantic, node_id, &self.work);
    let retain = array_is_controlled_source(semantic, node_id, &self.calls, &self.work);
    let mut elements = Vec::new();
    let mut closed = true;
    for (index, element) in array.elements.iter().enumerate() {
      self.work.add_object_entries(1);
      let Some(expression) = element.as_expression() else {
        closed = false;
        continue;
      };
      self.record_expr(semantic, kind, expression);
      if closed {
        elements.push(expression.get_inner_expression().span());
      }
      if map_entry && index == 0 {
        continue;
      }
      if map_iterable || retain {
        continue;
      }
      self.mark_escape_expr(semantic, expression, true);
      self.poison_expr(semantic, expression);
    }
    if closed {
      self.array_elements.insert(span_key(array.span), elements);
    }
  }

  pub(super) fn mark_delete_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    argument: &Expression<'_>,
  ) {
    match argument.get_inner_expression() {
      Expression::StaticMemberExpression(member) => {
        let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(symbol_id) = reference_symbol(semantic, object) else {
          return;
        };
        if member.property.name.as_str() == TOREF_CAPABILITY_KEY {
          self.toref_identity_uncertain.insert(self.root_of(symbol_id));
        }
      }
      Expression::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          self.toref_identity_uncertain.insert(self.root_of(symbol_id));
        }
      }
      _ => {}
    }
  }

  pub(super) fn record_function(
    &mut self,
    node_id: NodeId,
    span: Span,
    params: &FormalParameters<'_>,
    expression_arrow: bool,
  ) {
    let mut simple = Vec::with_capacity(params.items.len());
    let mut has_pattern = false;
    for item in &params.items {
      if let Some(symbol_id) = simple_binding_symbol(&item.pattern) {
        simple.push(Some(symbol_id));
      } else {
        has_pattern = true;
        simple.push(None);
      }
    }
    self.callables.insert(span_key(span), node_id);
    self.function_by_node.insert(node_id, span);
    self.functions.insert(
      span_key(span),
      FunctionInfo {
        node_id,
        params: simple,
        has_rest: params.rest.is_some() || has_pattern,
        expression_arrow,
      },
    );
  }

  pub(super) fn record_call_uses(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let owner = self.owner(node_id);
    let block = owner.block.unwrap_or(node_id);
    let offset = mapped(line_index, sfc_source, script_offset, call.span).offset;
    let api = self.calls.get(&span_key(call.span)).and_then(|info| info.api);
    if super::shape::is_watch_effect_api(api.unwrap_or(""))
      && let Some(first) = call.arguments.first().and_then(Argument::as_expression)
    {
      let inner = first.get_inner_expression();
      self.effect_calls.insert(
        span_key(inner.span()),
        ArgUse {
          api,
          index: 0,
          call_span: call.span,
          offset,
          node_id,
          callable: owner.callable,
          block,
        },
      );
    }
    if api == Some("watch")
      && let Some(first) = call.arguments.first().and_then(Argument::as_expression)
    {
      let inner = first.get_inner_expression();
      if matches!(inner, Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
      {
        self.watch_getters.insert(
          span_key(inner.span()),
          ArgUse {
            api,
            index: 0,
            call_span: call.span,
            offset,
            node_id,
            callable: owner.callable,
            block,
          },
        );
      }
    }
    if api.is_some_and(|name| {
      matches!(name, "watch" | "watchEffect" | "watchPostEffect" | "watchSyncEffect" | "effect")
    }) {
      self.watch_options.insert(span_key(call.span), parse_watch_options(call, api));
    }
    if let Some(identifier) = call.callee.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.ident_calls.entry(symbol_id).or_default().push(IdentCall {
        span: call.span,
        offset,
        callable: owner.callable,
        block,
      });
    }
    if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
      && let Some(object) = member.object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, object)
    {
      self
        .handle_member_calls
        .entry((symbol_id, member.property.name.to_string()))
        .or_default()
        .push(IdentCall { span: call.span, offset, callable: owner.callable, block });
    }
    for (index, argument) in call.arguments.iter().enumerate() {
      let Some(expression) = argument.as_expression() else {
        continue;
      };
      self.record_arg_ident(
        semantic,
        line_index,
        sfc_source,
        script_offset,
        expression,
        api,
        index,
        call.span,
        offset,
        node_id,
        owner.callable,
        block,
      );
      if index == 0
        && api == Some("watch")
        && let Expression::ArrayExpression(array) = expression.get_inner_expression()
      {
        for element in &array.elements {
          let Some(inner) = element.as_expression() else {
            continue;
          };
          self.record_arg_ident(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            inner,
            api,
            0,
            call.span,
            offset,
            node_id,
            owner.callable,
            block,
          );
        }
      }
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "call-arg indexing needs the mapped call site plus argument identity"
  )]
  pub(super) fn record_arg_ident(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    expression: &Expression<'_>,
    api: Option<&'static str>,
    index: usize,
    call_span: Span,
    call_offset: usize,
    node_id: NodeId,
    callable: Option<NodeId>,
    block: NodeId,
  ) {
    let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, identifier) else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, identifier.span).offset;
    self.arg_uses.entry(symbol_id).or_default().push(ArgUse {
      api,
      index: u32::try_from(index).unwrap_or(u32::MAX),
      call_span,
      offset: if offset == 0 { call_offset } else { offset },
      node_id,
      callable,
      block,
    });
  }

  pub(super) fn mark_escape_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    for_toref: bool,
  ) {
    if let Some(identifier) = expression.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.escape_root(self.root_of(symbol_id), for_toref);
    }
  }

  pub(super) fn escape_root(&mut self, root: SymbolId, for_toref: bool) {
    self.escaped.insert(root);
    if for_toref {
      self.toref_helper_escape.insert(root);
    }
    self
      .await_index
      .escapes
      .entry(root)
      .or_default()
      .push(AwaitEscapeSite { offset: 0, until_borrow: self.await_index.pending_borrow });
  }

  pub(super) fn index_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    offset: usize,
    operator: AssignmentOperator,
    left: &AssignmentTarget<'_>,
    right: &Expression<'_>,
  ) {
    if assignment_poisons_clone_intrinsic(semantic, left, &self.work) {
      self.clone_intrinsic_poisoned = true;
    }
    self.note_native_prototype_assignment(semantic, left);
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
          DirectMemberWrite {
            offset,
            callable,
            block,
            right,
            simple,
            fresh,
            span: member.span,
            node_id,
          },
        );
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          let root = self.root_of(symbol_id);
          self.unknown_member_touch.insert(root);
          self.capability_touch.insert(root);
          self.capability_poisoned.insert(root);
        }
      }
      AssignmentTarget::ObjectAssignmentTarget(_) | AssignmentTarget::ArrayAssignmentTarget(_) => {
        self.mark_pattern_uncertain(semantic, left);
      }
      _ => {}
    }
    self.record_snapshot_assignment(semantic, node_id, offset, simple, left, right);
  }

  pub(super) fn record_static_member_assignment(
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
    self.capability_poisoned.insert(root);
    if property == TOREF_CAPABILITY_KEY {
      self.toref_identity_uncertain.insert(root);
    }
    if property != "value" {
      self.capability_touch.insert(root);
    }
    if property == "value" {
      let event = ValueWrite {
        offset: write.offset,
        callable: write.callable,
        block: write.block,
        span: write.span,
        rhs: write.right.span(),
        literal: write_literal(semantic, write.right),
        simple_assign: write.simple,
        fresh_alloc: write.fresh,
        node_id: write.node_id,
      };
      self.value_write_events.entry(root).or_default().push(event);
      if write.simple {
        self.value_write_roots.insert(root);
        self.value_writes.entry(root).or_default().push(event);
      } else {
        self.uncertain.insert(root);
        self.await_index.uncertain_value_writes.insert(root);
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

  pub(super) fn record_snapshot_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    offset: usize,
    simple: bool,
    left: &AssignmentTarget<'_>,
    right: &Expression<'_>,
  ) {
    let owner = self.owner(node_id);
    let callable = owner.callable;
    match left {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => match identifier.name.as_str() {
        "Date" => self.date_poisoned = true,
        "JSON" => self.json_poisoned = true,
        "String" => self.string_capability_poisoned = true,
        _ => {}
      },
      AssignmentTarget::StaticMemberExpression(member) => {
        let property = member.property.name.as_str();
        if poisons_native_date(&member.object, property, |ident| reference_symbol(semantic, ident))
        {
          self.date_poisoned = true;
        }
        if poisons_date_tojson(&member.object, property, |ident| reference_symbol(semantic, ident))
        {
          self.date_poisoned = true;
        }
        if poisons_json(&member.object, property) {
          self.json_poisoned = true;
        }
        if poisons_string_capability(&member.object, property, |ident| {
          reference_symbol(semantic, ident)
        }) {
          self.string_capability_poisoned = true;
        }
        if property == "value"
          && let Some((ident, keys)) = peel_member_chain(&member.object, &self.work)
          && let Some(symbol_id) = reference_symbol(semantic, ident)
          && !keys.is_empty()
        {
          let region = region_of(owner, node_id);
          self
            .path_value_writes
            .entry(self.root_of(symbol_id))
            .or_default()
            .push((keys, PathWrite { offset, callable, region, simple_assign: simple }));
        }
        if let Some((ident, keys)) = peel_static_chain(&member.object, &self.work)
          && keys.as_slice() == ["value"]
          && let Some(symbol_id) = reference_symbol(semantic, ident)
        {
          self.push_nested_write(
            self.root_of(symbol_id),
            member.property.name.as_str().to_string(),
            NestedWrite {
              offset,
              span: member.span,
              callable,
              region: region_of(owner, node_id),
              rhs: right.span(),
              simple_assign: simple,
            },
          );
        }
        if let Some(ident) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, ident)
          && let Some(root) = self.value_object_alias.get(&self.root_of(symbol_id)).copied()
        {
          self.push_nested_write(
            root,
            member.property.name.as_str().to_string(),
            NestedWrite {
              offset,
              span: member.span,
              callable,
              region: region_of(owner, node_id),
              rhs: right.span(),
              simple_assign: simple,
            },
          );
        }
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if let Some(key) = computed_literal_key(&member.expression) {
          if poisons_json(&member.object, &key) {
            self.json_poisoned = true;
          }
          if poisons_string_capability(&member.object, &key, |ident| {
            reference_symbol(semantic, ident)
          }) {
            self.string_capability_poisoned = true;
          }
          if poisons_date_tojson(&member.object, &key, |ident| reference_symbol(semantic, ident)) {
            self.date_poisoned = true;
          }
        }
        if let Some((ident, keys)) = peel_static_chain(&member.object, &self.work)
          && keys.as_slice() == ["value"]
          && let Some(symbol_id) = reference_symbol(semantic, ident)
          && let Some(key) = computed_literal_key(&member.expression)
        {
          self.push_nested_write(
            self.root_of(symbol_id),
            key,
            NestedWrite {
              offset,
              span: member.span,
              callable,
              region: region_of(owner, node_id),
              rhs: right.span(),
              simple_assign: simple,
            },
          );
        }
      }
      _ => {}
    }
  }

  pub(super) fn push_nested_write(&mut self, root: SymbolId, property: String, write: NestedWrite) {
    self.work.add_writes(1);
    self.nested_writes.entry(root).or_default().push((property, write));
  }

  pub(super) fn mark_pattern_uncertain(
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
  /// `state.n` keeps generic uncertainty only; `__v_isRef` and dynamic targets
  /// also mark toRef identity, and capability keys mark the dedicated
  /// capability set.
  pub(super) fn record_pattern_static_member(
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
    if property == "value" {
      self.await_index.uncertain_value_writes.insert(root);
    }
    self.capability_poisoned.insert(root);
    if property == TOREF_CAPABILITY_KEY {
      self.toref_identity_uncertain.insert(root);
    }
    if is_capability_key(property) {
      self.capability_uncertain.insert(root);
    }
  }

  pub(super) fn record_pattern_dynamic_member(
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
    self.toref_identity_uncertain.insert(root);
    self.capability_uncertain.insert(root);
    self.capability_touch.insert(root);
    self.capability_poisoned.insert(root);
  }

  pub(super) fn mark_expression_target_uncertain(
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

  pub(super) fn mark_maybe_default_uncertain(
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

  pub(super) fn record_static_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    member: &StaticMemberExpression<'_>,
  ) {
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let use_site = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, member.span).offset,
      span: member.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, node_id, &self.work),
      role: classify_role(semantic, node_id, &self.work),
    };
    let property = member.property.name.as_str();
    let object = member.object.get_inner_expression();
    if let Some(ident) = object.get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      let root = self.root_of(symbol_id);
      if property == "prototype" {
        self.prototype_touch.insert(root);
      }
      if property == "value" {
        if use_site.role.needs_get() || use_site.role.needs_set() {
          self.value_reads.entry(root).or_default().push(use_site);
        }
      } else {
        self
          .member_reads_by_root
          .entry(root)
          .or_default()
          .push(NamedUse { key: property.to_string(), site: use_site });
      }
    }
    if property == "value"
      && let Expression::StaticMemberExpression(inner) = object
      && let Some(ident) = inner.object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      let root = self.root_of(symbol_id);
      let key = inner.property.name.as_str().to_string();
      self.chained_value_by_root.entry(root).or_default().push(NamedUse { key, site: use_site });
    }
    if !use_site.optional
      && !member.optional
      && let Some((ident, mut keys)) = peel_member_chain(&member.object, &self.work)
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      self.work.add_queries(1);
      keys.push(property.to_string());
      self
        .path_reads
        .entry(self.root_of(symbol_id))
        .or_default()
        .push(PathRead { keys, site: use_site });
    }
  }

  pub(super) fn record_member_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Some((object, key)) = member_call_object_key(call.callee.get_inner_expression()) else {
      return;
    };
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let object = object.get_inner_expression();
    if let Some(ident) = object.get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      let use_site = MemberUse {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
        optional,
        call_optional: call.optional,
        reach: classify_reach(semantic, node_id, &self.work),
        role: DemandRole::Other,
      };
      let named = NamedUse { key: key.to_string(), site: use_site };
      let root = self.root_of(symbol_id);
      if named.key == "stop" && self.demand_ok(&use_site) {
        self.stops_by_region.entry((root, use_site.callable, region)).or_default().push(use_site);
        self.stop_offsets.push(use_site.offset);
      }
      self.member_call_by_span.insert(span_key(call.span), named.clone());
      self.await_index.result_method_calls.entry(root).or_default().push(NamedUse {
        key: named.key.clone(),
        site: MemberUse {
          reach: classify_reach_except_chain(semantic, node_id, &self.work),
          ..use_site
        },
      });
      self.member_calls_by_root.entry(root).or_default().push(NamedUse {
        key: named.key,
        site: MemberUse {
          reach: classify_reach_except_chain(semantic, node_id, &self.work),
          call_optional: call.optional,
          ..use_site
        },
      });
      return;
    }
    if let Expression::AwaitExpression(awaited) = object {
      let use_site = MemberUse {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
        optional,
        call_optional: call.optional,
        reach: classify_reach_except_chain(semantic, node_id, &self.work),
        role: DemandRole::Other,
      };
      let named = NamedUse { key: key.to_string(), site: use_site };
      self.member_call_by_span.insert(span_key(call.span), named.clone());
      self.await_index.await_method_calls.entry(span_key(awaited.span)).or_default().push(named);
    }
    if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() {
      let use_site = MemberUse {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
        optional,
        call_optional: call.optional,
        reach: classify_reach(semantic, node_id, &self.work),
        role: DemandRole::Other,
      };
      self.record_path_call(semantic, member, use_site);
    }
  }

  pub(super) fn record_path_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    member: &StaticMemberExpression<'_>,
    site: MemberUse,
  ) {
    if site.optional {
      return;
    }
    let Some((ident, keys)) = peel_static_chain(&member.object, &self.work) else {
      return;
    };
    if keys.is_empty() {
      return;
    }
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let root = self.root_of(symbol_id);
    let path = keys.into_iter().map(str::to_string).collect();
    self.path_calls.entry(root).or_default().push(PathCall {
      keys: path,
      method: member.property.name.as_str().to_string(),
      site,
    });
  }

  pub(super) fn record_flow(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    self.record_event(semantic, line_index, sfc_source, script_offset, node_id, span);
    self.record_control_event(node_id, line_index, sfc_source, script_offset, span);
  }

  pub(super) fn record_control_event(
    &mut self,
    node_id: NodeId,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    span: Span,
  ) {
    let Some(block) = self.owner(node_id).block else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.control_events_by_block.entry(block).or_default().push(offset);
  }

  pub(super) fn record_pause_event(
    &mut self,
    node_id: NodeId,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    span: Span,
  ) {
    let Some(block) = self.owner(node_id).block else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.pause_events_by_block.entry(block).or_default().push(offset);
  }

  pub(super) fn record_identifier_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Some(ident) = call.callee.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let block = owner.block.unwrap_or(node_id);
    let has_spread = call.arguments.iter().any(Argument::is_spread);
    let argc = u8::try_from(call.arguments.len()).unwrap_or(u8::MAX);
    let use_site = CallUse {
      offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
      span: call.span,
      callable: owner.callable,
      region,
      block,
      optional,
      reach: classify_reach(semantic, node_id, &self.work),
      argc,
      has_spread,
    };
    let root = self.root_of(symbol_id);
    if self.symbol_is_vueuse_producer(root) {
      self.producer_call_offsets.push(use_site.offset);
    }
    self.identifier_calls.entry(root).or_default().push(use_site);
    self.record_wrapped_result_demand(
      semantic,
      line_index,
      sfc_source,
      script_offset,
      node_id,
      root,
      use_site,
    );
  }

  #[expect(clippy::too_many_arguments, reason = "span mapping matches other record helpers")]
  pub(super) fn record_wrapped_result_demand(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    root: SymbolId,
    inner: CallUse,
  ) {
    let parent = skip_ts_parent(semantic, node_id, &self.work);
    let AstKind::StaticMemberExpression(member) = semantic.nodes().kind(parent) else {
      return;
    };
    let Some((grand, outer)) = enclosing_call(semantic, parent, &self.work) else {
      return;
    };
    let optional = chain_optional(semantic, grand, &self.work);
    let owner = self.owner(grand);
    let region = region_of(owner, grand);
    let block = owner.block.unwrap_or(grand);
    let site = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, outer.span).offset,
      span: outer.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, grand, &self.work),
      role: DemandRole::Other,
    };
    self.result_demands.entry(root).or_default().push(ResultDemand {
      member: member.property.name.as_str().to_string(),
      site,
      inner,
      block,
    });
  }

  pub(super) fn symbol_is_vueuse_producer(&self, root: SymbolId) -> bool {
    let Some(init) = self.init_span.get(&root).copied() else {
      return false;
    };
    let info = self.calls.get(&span_key(init)).copied().or_else(|| {
      let super::shape::ShapeHint::Call(span) = self.hints.get(&span_key(init)).copied()? else {
        return None;
      };
      self.calls.get(&span_key(span)).copied()
    });
    info.is_some_and(|call| call.vueuse.is_some() && !call.has_spread)
  }

  pub(super) fn record_result_demand(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return;
    };
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let block = owner.block.unwrap_or(node_id);
    let site = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
      span: call.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, node_id, &self.work),
      role: DemandRole::Other,
    };
    let object = &member.object.get_inner_expression();
    let Expression::StaticMemberExpression(inner) = object else {
      return;
    };
    if inner.property.name.as_str() != "value" {
      return;
    }
    let Some(ident) = inner.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let value_read = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, inner.span).offset,
      span: inner.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, node_id, &self.work),
      role: DemandRole::Read,
    };
    self.value_demands.entry(self.root_of(symbol_id)).or_default().push(ValueDemand {
      member: member.property.name.as_str().to_string(),
      site,
      value_read,
      block,
    });
  }

  pub(super) fn note_ctor_shadows(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    for symbol_id in semantic.scoping().symbol_ids() {
      self.work.add_references(1);
      match semantic.scoping().symbol_name(symbol_id) {
        "String" => {
          self.shadowed_ctors.insert("String");
        }
        "Number" => {
          self.shadowed_ctors.insert("Number");
        }
        "Boolean" => {
          self.shadowed_ctors.insert("Boolean");
        }
        "BigInt" => {
          self.shadowed_ctors.insert("BigInt");
        }
        "Object" => {
          self.shadowed_ctors.insert("Object");
        }
        "Symbol" => {
          self.shadowed_ctors.insert("Symbol");
        }
        "Date" => {
          self.shadowed_ctors.insert("Date");
        }
        "JSON" => {
          self.shadowed_ctors.insert("JSON");
        }
        _ => {}
      }
    }
  }

  #[expect(clippy::too_many_arguments, reason = "span mapping matches other record helpers")]
  pub(super) fn record_native_symbol(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    symbol_id: SymbolId,
    init: &Expression<'_>,
  ) {
    if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
      return;
    }
    let Expression::CallExpression(call) = init.get_inner_expression() else {
      return;
    };
    if call.arguments.iter().any(Argument::is_spread) || call.arguments.len() > 1 {
      return;
    }
    let Some(identifier) = call.callee.get_inner_expression().get_identifier_reference() else {
      return;
    };
    if identifier.name.as_str() != "Symbol" || reference_symbol(semantic, identifier).is_some() {
      return;
    }
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    self.native_symbols.insert(
      symbol_id,
      NativeSymbol {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
      },
    );
  }

  pub(super) fn record_injection(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
      return;
    };
    let Some(key_expr) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Some(ident) = key_expr.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let root = self.root_of(symbol_id);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let site = InjectionSite {
      offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
      span: call.span,
      callable: owner.callable,
      region,
      block: owner.block.unwrap_or(node_id),
      reach: classify_reach(semantic, node_id, &self.work),
      optional: chain_optional(semantic, node_id, &self.work),
      argc: info.arg_count,
      has_spread: info.has_spread,
      payload: info.second_arg,
      factory: call.arguments.get(2).and_then(Argument::as_expression).and_then(boolean_literal),
      node_id,
    };
    match info.api {
      Some("provide") => self.provides_by_key.entry(root).or_default().push(site),
      Some("inject") => self.injects_by_key.entry(root).or_default().push(site),
      _ => {}
    }
  }

  pub(super) fn note_native_prototype_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    left: &AssignmentTarget<'_>,
  ) {
    let (object, last_key) = match left {
      AssignmentTarget::StaticMemberExpression(member) => {
        (&member.object, Some(member.property.name.as_str()))
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        (&member.object, expression_static_key(&member.expression))
      }
      _ => return,
    };
    if prototype_receiver_is_native_ctor(semantic, object, last_key) {
      self.prototype_mutated = true;
    }
  }

  pub(super) fn record_destructure(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    pattern: &BindingPattern<'_>,
    init: &Expression<'_>,
  ) {
    let Some(ident) = init.get_inner_expression().get_identifier_reference() else {
      if let BindingPattern::ObjectPattern(object) = pattern
        && let Expression::CallExpression(call) = init.get_inner_expression()
      {
        self.record_object_destructure_from_call(semantic, object, call.span);
      }
      return;
    };
    let Some(object_symbol) = reference_symbol(semantic, ident) else {
      return;
    };
    let root = self.root_of(object_symbol);
    let BindingPattern::ObjectPattern(object) = pattern else {
      return;
    };
    if object.rest.is_some() {
      self.uncertain.insert(root);
      return;
    }
    for property in &object.properties {
      let Some(key) = property.key.static_name() else {
        self.uncertain.insert(root);
        return;
      };
      if let BindingPattern::BindingIdentifier(binding) = &property.value
        && let Some(local) = binding.symbol_id.get()
      {
        self.destructure_by_object.entry(root).or_default().push((local, key.to_string()));
      }
    }
  }

  pub(super) fn record_object_destructure_from_call(
    &mut self,
    _semantic: &oxc_semantic::Semantic<'_>,
    object: &oxc_ast::ast::ObjectPattern<'_>,
    call_span: Span,
  ) {
    if object.rest.is_some() {
      return;
    }
    let mut bindings = Vec::new();
    for property in &object.properties {
      let Some(key) = property.key.static_name() else {
        return;
      };
      if let BindingPattern::BindingIdentifier(binding) = &property.value
        && let Some(local) = binding.symbol_id.get()
      {
        self.work.add_key_copies(1);
        let name = key.to_string();
        self.destructure_by_object.entry(local).or_default().push((local, name.clone()));
        bindings.push((local, name));
      }
    }
    if !bindings.is_empty() {
      self.call_destructure.insert(span_key(call_span), bindings);
    }
  }

  pub(super) fn index_vueuse_value_writes(&mut self) {
    let mut value_writes_by_callable: HashMap<(SymbolId, Option<NodeId>), Vec<ValueWrite>> =
      HashMap::new();
    for (root, writes) in &self.value_write_events {
      for write in writes {
        value_writes_by_callable.entry((*root, write.callable)).or_default().push(*write);
      }
    }
    self.value_writes_by_callable = value_writes_by_callable;
  }

  pub(super) fn build_until_await_lookups(&mut self) {
    for site in &self.await_index.awaits {
      self.await_index.await_by_argument.insert(span_key(site.argument), *site);
      self
        .await_index
        .awaits_by_region
        .entry((site.callable, site.region))
        .or_default()
        .push(*site);
      if let Some(symbol_id) = site.bound {
        let root = self.root_of(symbol_id);
        self.await_index.await_by_bound.entry(root).or_default().push(*site);
      }
    }
  }

  pub(super) fn summarize_until_closed_sources(&mut self) {
    let roots: Vec<_> = self.init_span.keys().copied().collect();
    for root in roots {
      self.work.add_queries(1);
      let foreign_escape = self.await_index.escapes.get(&root).is_some_and(|sites| {
        sites.iter().any(|site| {
          self.work.add_queries(1);
          !site.until_borrow
        })
      });
      let Some(init_span) = self.init_span.get(&root).copied() else {
        continue;
      };
      let closed = !foreign_escape
        && !self.mixed_value_owners.contains(&root)
        && !self.reassigned.contains(&root)
        && !self.await_index.uncertain_value_writes.contains(&root)
        && !self.unknown_member_touch.contains(&root)
        && !self.capability_touch.contains(&root);
      self.await_index.closed_sources.insert(root, AwaitClosedSource { init_span, closed });
    }
  }

  pub(super) fn summarize_closed_keys(&mut self) {
    for (key, entries) in &self.objects {
      let mut keys = HashSet::new();
      let mut closed = true;
      for entry in entries {
        self.work.add_object_entries(1);
        match entry {
          ObjectEntry::Data { name, .. } => {
            if is_custom_prototype_key(name) {
              closed = false;
              break;
            }
            self.work.add_key_copies(1);
            keys.insert(name.clone());
          }
          ObjectEntry::Spread
          | ObjectEntry::Computed
          | ObjectEntry::Accessor { .. }
          | ObjectEntry::Method { .. } => {
            closed = false;
            break;
          }
        }
      }
      if closed {
        self.closed_keys.insert(*key, keys);
      }
    }
  }

  pub(super) fn record_barrier(
    &mut self,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let Some(region) = self.owner(node_id).region else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.barriers_by_region.entry(region).or_default().push(offset);
  }

  pub(super) fn record_barrier_end(
    &mut self,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let Some(region) = self.owner(node_id).region else {
      return;
    };
    let mapped_span = mapped(line_index, sfc_source, script_offset, span);
    let offset = mapped_span.offset.saturating_add(mapped_span.length);
    self.barriers_by_region.entry(region).or_default().push(offset);
  }

  pub(super) fn record_class(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    class: &oxc_ast::ast::Class<'_>,
  ) {
    let Some(symbol_id) = class_binding_symbol(semantic, node_id, class) else {
      return;
    };
    let record = analyze_class(class, &self.work);
    self.classes.insert(symbol_id, record);
  }

  pub(super) fn record_class_new(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &oxc_ast::ast::NewExpression<'_>,
  ) {
    let callee = expression
      .callee
      .get_inner_expression()
      .get_identifier_reference()
      .and_then(|ident| reference_symbol(semantic, ident));
    self.class_news.insert(span_key(expression.span), class_new_info(expression, callee));
  }

  pub(super) fn record_stmt_site(
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

  pub(super) fn record_event(
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

  pub(super) fn has_callable_termination_between(
    &self,
    callable: Option<NodeId>,
    start: usize,
    end: usize,
  ) -> bool {
    self.timeline_has_between(&self.terminations_by_callable, &callable, start, end)
  }

  pub(super) fn call_reach_is_straight(
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

  pub(super) fn precompute_aliases(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
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

  pub(super) fn note_prototype_alias(
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

  pub(super) fn resolve_prototype_aliases(&mut self, pending: Vec<(SymbolId, SymbolId)>) {
    for (local, object) in pending {
      self.work.add_queries(1);
      if let Some(ctor) = self.native_ctor_kind(self.root_of(object)) {
        self.record_native_ctor_alias(local, ctor);
      }
    }
  }

  pub(super) fn record_native_ctor_alias(&mut self, local: SymbolId, ctor: &'static str) {
    self.work.add_queries(1);
    self.native_ctor_aliases.insert(local, ctor);
  }

  pub(super) fn native_ctor_kind(&self, root: SymbolId) -> Option<&'static str> {
    self.work.add_queries(1);
    self.native_ctor_aliases.get(&root).copied()
  }

  pub(super) fn native_ctor_of_identifier(
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

  pub(super) fn canonicalize_aliases(&mut self) {
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

  pub(super) fn poison_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    self.poison_expr_bounded(semantic, expression, MAX_ESCAPE_DEPTH);
  }

  pub(super) fn poison_expr_bounded(
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

  pub(super) fn poison_unresolved_escape(
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

  pub(super) fn merged_unresolved_escape_ranges(&self) -> Vec<(u32, u32)> {
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

  pub(super) fn unresolved_escape_covers(&self, ranges: &[(u32, u32)], span: Span) -> bool {
    let index = self.work.partition_point(ranges, |range| range.0 <= span.start);
    self.work.add_queries(1);
    index
      .checked_sub(1)
      .and_then(|index| ranges.get(index))
      .is_some_and(|range| span.end <= range.1)
  }

  pub(super) fn taint_native_ctor_identifier(
    &mut self,
    identifier: &IdentifierReference<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
  ) {
    if let Some(ctor) = self.native_ctor_of_identifier(identifier, semantic) {
      self.tainted_ctors.insert(ctor);
    }
  }

  pub(super) fn taint_if_global_object(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    identifier: &IdentifierReference<'_>,
  ) {
    if self.is_global_this_ref(semantic, identifier) {
      self.taint_all_native_ctors();
    }
  }

  pub(super) fn taint_all_native_ctors(&mut self) {
    self.work.add_queries(1);
    for name in ["Map", "Set", "Array"] {
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      }
    }
  }

  pub(super) fn index_unresolved_global_ref(
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

  pub(super) fn apply_unresolved_global_escape_coverage(&mut self, leftover: &[(u32, u32)]) {
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

  pub(super) fn taint_native_prototype(
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

  pub(super) fn taint_assignment_target(
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

  pub(super) fn taint_maybe_default(
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

  pub(super) fn taint_expression_target(
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

  pub(super) fn taint_global_ctor_member(
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

  pub(super) fn is_global_this_ref(
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

  pub(super) fn note_receiver_use(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    callee: &Expression<'_>,
  ) {
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

  pub(super) fn record_method_extraction(
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

  pub(super) fn poison_simple_target(
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

  pub(super) fn poison_member_expression(
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

  pub(super) fn poison_member_object(
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

  pub(super) fn record_termination(
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
}
