//! Factory-storage lattice for customRef get/set. The join stays in `mod.rs`.

use super::{
  Argument, ArrayExpressionElement, AssignmentOperator, AssignmentTarget,
  AssignmentTargetMaybeDefault, AssignmentTargetProperty, AstKind, BindingPattern, Collector,
  Expression, FACTORY_BIND_BUDGET, FunctionBody, HashMap, KnownPrim, ObjectPropertyKind,
  PropertyKind, SimpleAssignmentTarget, Span, Statement, SymbolFlags, SymbolId, TakenBranch,
  TakenLogical, assignment_target_writes_symbol, computed_key_expression, known_prim,
  mentions_storage, taken_conditional_branch, taken_logical_side, update_target_ident,
};

#[expect(
  clippy::too_many_lines,
  reason = "one factory storage inventory covers assignment, update, sequence, and nested ownership"
)]
pub(super) fn apply_factory_storage_expr(
  collector: &mut Collector<'_>,
  expression: &Expression<'_>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  if *unknown {
    return;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::AssignmentExpression(assignment) => {
      apply_factory_storage_expr(
        collector,
        &assignment.right,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_assignment_target(
        collector,
        &assignment.left,
        Some(&assignment.right),
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      if assignment_target_writes_symbol(&assignment.left, Some(storage), collector) {
        if assignment.operator == AssignmentOperator::Assign
          && let Some(prim) =
            known_prim(&assignment.right, |ident| collector.reference_symbol(ident))
        {
          *current = Some(prim);
        } else {
          *unknown = true;
        }
      }
    }
    Expression::UpdateExpression(update) => {
      apply_factory_simple_assignment_target(
        collector,
        &update.argument,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      if let Some(ident) = update_target_ident(update)
        && collector.reference_symbol(ident) == Some(storage)
      {
        *unknown = true;
      }
    }
    Expression::SequenceExpression(sequence) => {
      for item in &sequence.expressions {
        apply_factory_storage_expr(collector, item, storage, get_span, set_span, current, unknown);
      }
    }
    Expression::UnaryExpression(unary) => {
      apply_factory_storage_expr(
        collector,
        &unary.argument,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::BinaryExpression(binary) => {
      apply_factory_storage_expr(
        collector,
        &binary.left,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &binary.right,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::LogicalExpression(logical) => {
      apply_factory_storage_expr(
        collector,
        &logical.left,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      match taken_logical_side(
        logical.operator,
        &logical.left,
        collector.semantic,
        collector.indexes.work_counter(),
      ) {
        TakenLogical::Both => apply_factory_storage_expr(
          collector,
          &logical.right,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        ),
        TakenLogical::LeftOnly => {}
        TakenLogical::Unknown => *unknown = true,
      }
    }
    Expression::ConditionalExpression(conditional) => {
      apply_factory_storage_expr(
        collector,
        &conditional.test,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      match taken_conditional_branch(&conditional.test, collector.indexes.work_counter()) {
        TakenBranch::Consequent => apply_factory_storage_expr(
          collector,
          &conditional.consequent,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        ),
        TakenBranch::Alternate => apply_factory_storage_expr(
          collector,
          &conditional.alternate,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        ),
        TakenBranch::Unknown => *unknown = true,
      }
    }
    Expression::CallExpression(call) => {
      apply_factory_storage_expr(
        collector,
        &call.callee,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_call_args(
        collector,
        &call.arguments,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::NewExpression(new_expr) => {
      apply_factory_storage_expr(
        collector,
        &new_expr.callee,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_call_args(
        collector,
        &new_expr.arguments,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ObjectExpression(object) => {
      for property in &object.properties {
        match property {
          ObjectPropertyKind::ObjectProperty(property) => {
            match computed_key_expression(property) {
              Some(key) => apply_factory_storage_expr(
                collector, key, storage, get_span, set_span, current, unknown,
              ),
              None if property.computed => *unknown = true,
              None => {}
            }
            apply_factory_storage_expr(
              collector,
              &property.value,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
          ObjectPropertyKind::SpreadProperty(spread) => {
            apply_factory_storage_expr(
              collector,
              &spread.argument,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
        }
      }
    }
    Expression::ArrayExpression(array) => {
      for element in &array.elements {
        match element {
          ArrayExpressionElement::SpreadElement(spread) => {
            apply_factory_storage_expr(
              collector,
              &spread.argument,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
          ArrayExpressionElement::Elision(_) => {}
          other => {
            if let Some(expr) = other.as_expression() {
              apply_factory_storage_expr(
                collector, expr, storage, get_span, set_span, current, unknown,
              );
            } else {
              *unknown = true;
            }
          }
        }
      }
    }
    Expression::StaticMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ComputedMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &member.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ParenthesizedExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.span == get_span || arrow.span == set_span {
        return;
      }
      if function_mentions_storage(Some(&arrow.body), storage, collector) {
        *unknown = true;
      }
    }
    Expression::FunctionExpression(function) => {
      if function.span == get_span || function.span == set_span {
        return;
      }
      if function_mentions_storage(function.body.as_deref(), storage, collector) {
        *unknown = true;
      }
    }
    Expression::TemplateLiteral(template) => {
      for item in &template.expressions {
        apply_factory_storage_expr(collector, item, storage, get_span, set_span, current, unknown);
      }
    }
    Expression::PrivateFieldExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::Identifier(_)
    | Expression::ThisExpression(_)
    | Expression::Super(_)
    | Expression::ImportMeta(_)
    | Expression::NewTarget(_) => {}
    _ => *unknown = true,
  }
}

fn apply_factory_call_args(
  collector: &mut Collector<'_>,
  arguments: &[Argument<'_>],
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  for argument in arguments {
    match argument {
      Argument::SpreadElement(spread) => {
        apply_factory_storage_expr(
          collector,
          &spread.argument,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        );
      }
      other => {
        if let Some(arg) = other.as_expression() {
          apply_factory_storage_expr(collector, arg, storage, get_span, set_span, current, unknown);
        } else {
          *unknown = true;
        }
      }
    }
  }
}

enum Retrieved<'a> {
  Missing,
  Value(&'a Expression<'a>),
  Unknown,
}

enum ProtoKind<'a> {
  Standard,
  Null,
  Unknown,
  Inherited(Box<SourceProps<'a>>),
}

struct SourceProps<'a> {
  last: HashMap<String, &'a Expression<'a>>,
  proto: ProtoKind<'a>,
}

fn is_object_proto_key(name: &str) -> bool {
  matches!(
    name,
    "constructor"
      | "hasOwnProperty"
      | "isPrototypeOf"
      | "propertyIsEnumerable"
      | "toLocaleString"
      | "toString"
      | "valueOf"
  )
}

fn empty_source_props<'a>() -> SourceProps<'a> {
  SourceProps { last: HashMap::new(), proto: ProtoKind::Standard }
}

fn is_proto_setter(property: &oxc_ast::ast::ObjectProperty<'_>) -> bool {
  !property.computed
    && !property.shorthand
    && !property.method
    && property.kind == PropertyKind::Init
    && property.key.static_name().as_deref() == Some("__proto__")
}

const fn consume_bind(budget: &mut u32) -> bool {
  if *budget == 0 {
    return false;
  }
  *budget = budget.saturating_sub(1);
  true
}

#[expect(
  clippy::too_many_arguments,
  reason = "factory binding walk threads storage inventory, initializer, and the bind budget"
)]
pub(super) fn apply_factory_binding_pattern(
  collector: &mut Collector<'_>,
  pattern: &BindingPattern<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
  budget: &mut u32,
) {
  if *unknown {
    return;
  }
  if !consume_bind(budget) {
    *unknown = true;
    return;
  }
  collector.indexes.note_node();
  match pattern {
    BindingPattern::BindingIdentifier(_) => {}
    BindingPattern::AssignmentPattern(_) | BindingPattern::ArrayPattern(_) => *unknown = true,
    BindingPattern::ObjectPattern(object) => {
      if object.rest.is_some() {
        *unknown = true;
        return;
      }
      let summary = source.map_or_else(
        || Some(empty_source_props()),
        |expr| summarize_source_object(expr, collector, budget),
      );
      if source.is_some() && summary.is_none() {
        *unknown = true;
        return;
      }
      for property in &object.properties {
        if *unknown {
          return;
        }
        if !consume_bind(budget) {
          *unknown = true;
          return;
        }
        collector.indexes.note_node();
        if property.computed {
          match property.key.as_expression() {
            Some(key) => apply_factory_storage_expr(
              collector, key, storage, get_span, set_span, current, unknown,
            ),
            None => *unknown = true,
          }
        }
        if *unknown {
          return;
        }
        let retrieved = if property.computed {
          Retrieved::Unknown
        } else if let Some(name) = property.key.static_name() {
          lookup_source_prop(summary.as_ref(), name.as_ref(), collector, budget)
        } else {
          Retrieved::Unknown
        };
        match &property.value {
          BindingPattern::AssignmentPattern(inner) => {
            let activate = match retrieved {
              Retrieved::Missing => Some(true),
              Retrieved::Value(expr) => proven_retrieved_undefined(expr, collector, budget),
              Retrieved::Unknown => None,
            };
            match activate {
              Some(true) => apply_factory_storage_expr(
                collector,
                &inner.right,
                storage,
                get_span,
                set_span,
                current,
                unknown,
              ),
              Some(false) => {}
              None => *unknown = true,
            }
            let nested = match activate {
              Some(true) => Some(&inner.right),
              Some(false) => match retrieved {
                Retrieved::Value(expr) => Some(expr),
                Retrieved::Missing | Retrieved::Unknown => None,
              },
              None => None,
            };
            apply_factory_binding_pattern(
              collector,
              &inner.left,
              nested,
              storage,
              get_span,
              set_span,
              current,
              unknown,
              budget,
            );
          }
          BindingPattern::BindingIdentifier(_) => {}
          other => {
            let nested = match retrieved {
              Retrieved::Value(expr) => Some(expr),
              Retrieved::Missing | Retrieved::Unknown => {
                *unknown = true;
                None
              }
            };
            apply_factory_binding_pattern(
              collector, other, nested, storage, get_span, set_span, current, unknown, budget,
            );
          }
        }
      }
    }
  }
}

fn summarize_source_object<'a>(
  expression: &'a Expression<'a>,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> Option<SourceProps<'a>> {
  if !consume_bind(budget) {
    return None;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::SequenceExpression(sequence) => {
      sequence.expressions.last().and_then(|item| summarize_source_object(item, collector, budget))
    }
    Expression::ObjectExpression(object) => {
      let mut last = HashMap::new();
      let mut proto = ProtoKind::Standard;
      for property in &object.properties {
        if !consume_bind(budget) {
          return None;
        }
        collector.indexes.work_counter().add_object_entries(1);
        match property {
          ObjectPropertyKind::SpreadProperty(_) => return None,
          ObjectPropertyKind::ObjectProperty(property) => {
            if is_proto_setter(property) {
              proto = classify_proto_value(&property.value, collector, budget);
              continue;
            }
            if property.computed || property.method || property.kind != PropertyKind::Init {
              return None;
            }
            let name = property.key.static_name()?;
            collector.indexes.work_counter().add_writes(1);
            last.insert(name.into_owned(), &property.value);
          }
        }
      }
      Some(SourceProps { last, proto })
    }
    _ => None,
  }
}

fn classify_proto_value<'a>(
  expression: &'a Expression<'a>,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> ProtoKind<'a> {
  if !consume_bind(budget) {
    return ProtoKind::Unknown;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::NullLiteral(_) => ProtoKind::Null,
    Expression::ObjectExpression(_) => summarize_source_object(expression, collector, budget)
      .map_or(ProtoKind::Unknown, |nested| ProtoKind::Inherited(Box::new(nested))),
    Expression::SequenceExpression(sequence) => sequence
      .expressions
      .last()
      .map_or(ProtoKind::Unknown, |item| classify_proto_value(item, collector, budget)),
    _ => ProtoKind::Unknown,
  }
}

fn lookup_source_prop<'a>(
  summary: Option<&SourceProps<'a>>,
  name: &str,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> Retrieved<'a> {
  collector.indexes.note_query();
  let Some(summary) = summary else {
    return Retrieved::Unknown;
  };
  if !consume_bind(budget) {
    return Retrieved::Unknown;
  }
  if let Some(expr) = summary.last.get(name) {
    return Retrieved::Value(expr);
  }
  match &summary.proto {
    ProtoKind::Null => Retrieved::Missing,
    ProtoKind::Standard => {
      if is_object_proto_key(name) {
        Retrieved::Unknown
      } else {
        Retrieved::Missing
      }
    }
    ProtoKind::Inherited(inner) => {
      lookup_source_prop(Some(inner.as_ref()), name, collector, budget)
    }
    ProtoKind::Unknown => Retrieved::Unknown,
  }
}

fn proven_retrieved_undefined(
  expression: &Expression<'_>,
  collector: &Collector<'_>,
  budget: &mut u32,
) -> Option<bool> {
  if !consume_bind(budget) {
    return None;
  }
  collector.indexes.note_node();
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) if identifier.name.as_str() == "undefined" => {
      match collector.reference_symbol(identifier) {
        None => Some(true),
        Some(symbol) => {
          let prim = const_known_prim(collector, symbol, budget)?;
          Some(matches!(prim, KnownPrim::Undefined))
        }
      }
    }
    Expression::Identifier(identifier) => {
      let symbol = collector.reference_symbol(identifier)?;
      let prim = const_known_prim(collector, symbol, budget)?;
      Some(matches!(prim, KnownPrim::Undefined))
    }
    Expression::NullLiteral(_)
    | Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::ObjectExpression(_)
    | Expression::ArrayExpression(_)
    | Expression::FunctionExpression(_)
    | Expression::ArrowFunctionExpression(_)
    | Expression::ClassExpression(_) => Some(false),
    Expression::SequenceExpression(sequence) => sequence
      .expressions
      .last()
      .and_then(|item| proven_retrieved_undefined(item, collector, budget)),
    _ => None,
  }
}

fn const_known_prim(
  collector: &Collector<'_>,
  symbol: SymbolId,
  budget: &mut u32,
) -> Option<KnownPrim> {
  if !consume_bind(budget) {
    return None;
  }
  collector.indexes.note_query();
  if !collector.semantic.scoping().symbol_flags(symbol).contains(SymbolFlags::ConstVariable) {
    return None;
  }
  let mut node_id = collector.semantic.scoping().symbol_declaration(symbol);
  for _ in 0..8 {
    if !consume_bind(budget) {
      return None;
    }
    collector.indexes.note_node();
    match collector.semantic.nodes().kind(node_id) {
      AstKind::VariableDeclarator(declarator) => {
        let init = declarator.init.as_ref()?;
        return known_prim(init, |ident| collector.reference_symbol(ident));
      }
      AstKind::Program(_) => return None,
      _ => node_id = collector.semantic.nodes().parent_id(node_id),
    }
  }
  None
}

#[expect(
  clippy::too_many_arguments,
  reason = "object assignment uses the same factory inventory plus a cached source summary"
)]
fn apply_factory_object_assignment(
  collector: &mut Collector<'_>,
  object: &oxc_ast::ast::ObjectAssignmentTarget<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  if object.rest.is_some() {
    *unknown = true;
    return;
  }
  let mut budget = FACTORY_BIND_BUDGET;
  let summary = source.map_or_else(
    || Some(empty_source_props()),
    |expr| summarize_source_object(expr, collector, &mut budget),
  );
  if source.is_some() && summary.is_none() {
    *unknown = true;
    return;
  }
  for property in &object.properties {
    if *unknown {
      return;
    }
    if !consume_bind(&mut budget) {
      *unknown = true;
      return;
    }
    collector.indexes.note_node();
    match property {
      AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
        let retrieved = lookup_source_prop(
          summary.as_ref(),
          property.binding.name.as_str(),
          collector,
          &mut budget,
        );
        if let Some(default) = &property.init {
          let activate = match retrieved {
            Retrieved::Missing => Some(true),
            Retrieved::Value(expr) => proven_retrieved_undefined(expr, collector, &mut budget),
            Retrieved::Unknown => None,
          };
          match activate {
            Some(true) => apply_factory_storage_expr(
              collector, default, storage, get_span, set_span, current, unknown,
            ),
            Some(false) => {}
            None => *unknown = true,
          }
        }
      }
      AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
        if property.computed {
          match property.name.as_expression() {
            Some(key) => apply_factory_storage_expr(
              collector, key, storage, get_span, set_span, current, unknown,
            ),
            None => *unknown = true,
          }
        }
        if *unknown {
          return;
        }
        let retrieved = if property.computed {
          Retrieved::Unknown
        } else if let Some(name) = property.name.static_name() {
          lookup_source_prop(summary.as_ref(), name.as_ref(), collector, &mut budget)
        } else {
          Retrieved::Unknown
        };
        match &property.binding {
          AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(inner) => {
            let activate = match retrieved {
              Retrieved::Missing => Some(true),
              Retrieved::Value(expr) => proven_retrieved_undefined(expr, collector, &mut budget),
              Retrieved::Unknown => None,
            };
            match activate {
              Some(true) => apply_factory_storage_expr(
                collector,
                &inner.init,
                storage,
                get_span,
                set_span,
                current,
                unknown,
              ),
              Some(false) => {}
              None => *unknown = true,
            }
            let nested = match activate {
              Some(true) => Some(&inner.init),
              Some(false) => match retrieved {
                Retrieved::Value(expr) => Some(expr),
                Retrieved::Missing | Retrieved::Unknown => None,
              },
              None => None,
            };
            apply_factory_assignment_target(
              collector,
              &inner.binding,
              nested,
              storage,
              get_span,
              set_span,
              current,
              unknown,
            );
          }
          other => {
            let nested = match retrieved {
              Retrieved::Value(expr) => Some(expr),
              Retrieved::Missing | Retrieved::Unknown => {
                if other.as_assignment_target().is_some_and(|target| {
                  !matches!(target, AssignmentTarget::AssignmentTargetIdentifier(_))
                }) {
                  *unknown = true;
                }
                None
              }
            };
            apply_factory_assignment_maybe_default(
              collector, other, nested, storage, get_span, set_span, current, unknown,
            );
          }
        }
      }
    }
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "factory assignment-target walk threads storage inventory plus the rhs used for default activation"
)]
fn apply_factory_assignment_target(
  collector: &mut Collector<'_>,
  target: &AssignmentTarget<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(_) => {}
    AssignmentTarget::TSAsExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::StaticMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &member.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::PrivateFieldExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    AssignmentTarget::ArrayAssignmentTarget(_) => *unknown = true,
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      apply_factory_object_assignment(
        collector, object, source, storage, get_span, set_span, current, unknown,
      );
    }
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "factory assignment defaults thread storage inventory plus the extracted source value"
)]
fn apply_factory_assignment_maybe_default(
  collector: &mut Collector<'_>,
  target: &AssignmentTargetMaybeDefault<'_>,
  source: Option<&Expression<'_>>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(inner) => {
      if source.is_some() {
        apply_factory_assignment_target(
          collector,
          &inner.binding,
          source,
          storage,
          get_span,
          set_span,
          current,
          unknown,
        );
      } else {
        *unknown = true;
      }
    }
    other => {
      if let Some(target) = other.as_assignment_target() {
        apply_factory_assignment_target(
          collector, target, source, storage, get_span, set_span, current, unknown,
        );
      } else {
        *unknown = true;
      }
    }
  }
}

fn apply_factory_simple_assignment_target(
  collector: &mut Collector<'_>,
  target: &SimpleAssignmentTarget<'_>,
  storage: SymbolId,
  get_span: Span,
  set_span: Span,
  current: &mut Option<KnownPrim>,
  unknown: &mut bool,
) {
  match target {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(_) => {}
    SimpleAssignmentTarget::TSAsExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::TSNonNullExpression(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::TSTypeAssertion(inner) => {
      apply_factory_storage_expr(
        collector,
        &inner.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::StaticMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
      apply_factory_storage_expr(
        collector,
        &member.expression,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
    SimpleAssignmentTarget::PrivateFieldExpression(member) => {
      apply_factory_storage_expr(
        collector,
        &member.object,
        storage,
        get_span,
        set_span,
        current,
        unknown,
      );
    }
  }
}

pub(super) fn function_mentions_storage(
  body: Option<&FunctionBody<'_>>,
  storage: SymbolId,
  collector: &Collector<'_>,
) -> bool {
  let Some(body) = body else {
    return false;
  };
  body.statements.iter().any(|statement| match statement {
    Statement::ExpressionStatement(statement) => {
      mentions_storage(&statement.expression, Some(storage), collector)
    }
    Statement::ReturnStatement(ret) => ret
      .argument
      .as_ref()
      .is_some_and(|argument| mentions_storage(argument, Some(storage), collector)),
    Statement::VariableDeclaration(declaration) => {
      declaration.declarations.iter().any(|declarator| {
        declarator
          .init
          .as_ref()
          .is_some_and(|init| mentions_storage(init, Some(storage), collector))
      })
    }
    _ => false,
  })
}
