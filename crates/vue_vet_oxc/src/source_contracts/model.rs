//! Per-file `defineModel` / ref / mounted-demand / expose / instance-chain facts.

use std::collections::HashSet;

use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentTarget, BinaryOperator, BindingPattern, CallExpression, Expression,
    FunctionBody, ObjectExpression, ObjectPropertyKind, PropertyKey, Statement, UnaryOperator,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};

use super::Collector;
use super::index::CallInfo;
use super::shape::{ShapeHint, span_key};
use vue_vet_core::{
  DefineExposeFact, InstanceMemberDemandFact, InstancePathWriteFact, ModelDefaultFact,
  ModelDefaultOrigin, ModelPrimitiveKind, ModelValueWriteFact, MountedMemberDemandFact,
  OrdinaryRefInitFact, RefInitKind, ScriptKind, SharedObjectBindingFact,
};

const NATIVE_MEMBERS: &[&str] = &[
  "toFixed",
  "toExponential",
  "toPrecision",
  "toUpperCase",
  "toLowerCase",
  "slice",
  "charAt",
  "substring",
  "concat",
  "includes",
  "startsWith",
  "endsWith",
  "indexOf",
];

impl Collector<'_> {
  pub(super) fn collect_model_facts(&mut self) {
    let mut mounted = HashSet::new();
    for (node_id, node) in self.semantic.nodes().iter_enumerated() {
      self.indexes.note_node();
      let AstKind::CallExpression(call) = node.kind() else {
        continue;
      };
      let Some(info) = self.indexes.calls.get(&span_key(call.span)).copied() else {
        continue;
      };
      if info.api == Some("onMounted") {
        if let Some(callback) = self.callback_node(call) {
          mounted.insert(callback);
        }
        continue;
      }
      match info.api {
        Some("defineModel") if self.kind == ScriptKind::Setup => {
          self.collect_define_model(node_id, call, info);
        }
        Some("defineExpose") if self.kind == ScriptKind::Setup => {
          self.collect_define_expose(call);
        }
        Some("ref" | "shallowRef") => self.collect_ordinary_ref(node_id, call, info),
        _ => {}
      }
    }
    self.collect_shared_object_bindings();
    for (node_id, node) in self.semantic.nodes().iter_enumerated() {
      self.indexes.note_node();
      match node.kind() {
        AstKind::CallExpression(call) => {
          self.collect_native_demand(node_id, call, &mounted);
        }
        AstKind::AssignmentExpression(assignment) => {
          self.collect_chain_write(&assignment.left, &assignment.right);
        }
        _ => {}
      }
    }
  }

  fn callback_node(&self, call: &CallExpression<'_>) -> Option<NodeId> {
    let expression = call.arguments.first().and_then(Argument::as_expression)?;
    let span = expression.get_inner_expression().span();
    for (node_id, node) in self.semantic.nodes().iter_enumerated() {
      self.indexes.note_query();
      match node.kind() {
        AstKind::ArrowFunctionExpression(arrow) if arrow.span == span => return Some(node_id),
        AstKind::Function(function) if function.span == span => return Some(node_id),
        _ => {}
      }
    }
    None
  }

  fn collect_define_model(&mut self, node_id: NodeId, call: &CallExpression<'_>, info: CallInfo) {
    if info.has_spread {
      return;
    }
    let model_name = model_name_of(call);
    let options = model_options(call);
    let mut origin = ModelDefaultOrigin::Unknown;
    let mut primitive = None;
    let mut primitive_text = None;
    let mut shared_binding = None;
    let mut own_paths = Vec::new();
    let mut has_get_set = false;
    if let Some(object) = options {
      self.indexes.note_object_entries(u64::try_from(object.properties.len()).unwrap_or(u64::MAX));
      for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = property else {
          origin = ModelDefaultOrigin::Unknown;
          continue;
        };
        let Some(name) = static_key(&prop.key) else {
          origin = ModelDefaultOrigin::Unknown;
          continue;
        };
        match name {
          "get" | "set" => has_get_set = true,
          "default" => {
            let classified = classify_default(&prop.value);
            origin = classified.origin;
            primitive = classified.primitive;
            primitive_text = classified.primitive_text;
            shared_binding = classified.shared_binding;
            own_paths = classified.own_paths;
          }
          _ => {}
        }
      }
    }
    let binding = self.result_binding(node_id);
    let escaped = binding.is_some_and(|symbol| {
      let root = self.indexes.root_of(symbol);
      self.indexes.reassigned.contains(&root) || self.indexes.unknown_member_touch.contains(&root)
    });
    let binding_name = binding.map(|symbol| self.symbol_name(self.indexes.root_of(symbol)));
    self.facts.model_defaults.push(ModelDefaultFact {
      span: self.span(call.span),
      model_name,
      origin,
      binding: binding_name,
      primitive,
      primitive_text,
      shared_binding,
      own_paths,
      has_get_set,
      escaped,
    });
  }

  fn collect_define_expose(&mut self, call: &CallExpression<'_>) {
    let Some(expression) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Expression::ObjectExpression(object) = expression.get_inner_expression() else {
      return;
    };
    self.indexes.note_object_entries(u64::try_from(object.properties.len()).unwrap_or(u64::MAX));
    let mut names = Vec::new();
    for property in &object.properties {
      let ObjectPropertyKind::ObjectProperty(prop) = property else {
        return;
      };
      if prop.computed {
        return;
      }
      let Some(name) = static_key(&prop.key) else {
        return;
      };
      names.push(name.to_owned());
    }
    names.sort();
    names.dedup();
    self.facts.define_expose.push(DefineExposeFact { span: self.span(call.span), names });
  }

  fn collect_ordinary_ref(&mut self, node_id: NodeId, call: &CallExpression<'_>, info: CallInfo) {
    let Some(symbol) = self.result_binding(node_id) else {
      return;
    };
    let root = self.indexes.root_of(symbol);
    let kind = info.first_arg.map_or(RefInitKind::Undefined, |_| {
      let Some(argument) = call.arguments.first().and_then(Argument::as_expression) else {
        return RefInitKind::Undefined;
      };
      match argument.get_inner_expression() {
        Expression::Identifier(ident) if ident.name == "undefined" => RefInitKind::Undefined,
        Expression::NullLiteral(_) => RefInitKind::Null,
        other => match literal_primitive(other) {
          Some(ModelPrimitiveKind::Number) => RefInitKind::Number,
          Some(ModelPrimitiveKind::String) => RefInitKind::String,
          Some(ModelPrimitiveKind::Boolean) => RefInitKind::Boolean,
          None => match self.indexes.hints.get(&span_key(other.span())).copied() {
            Some(ShapeHint::PlainRecord) => RefInitKind::Object,
            Some(ShapeHint::Nullish) => RefInitKind::Null,
            _ => RefInitKind::Unknown,
          },
        },
      }
    });
    let name = self.symbol_name(root);
    let span = self.indexes.init_span.get(&root).copied().unwrap_or(Span::new(0, 0));
    self.facts.ordinary_ref_inits.push(OrdinaryRefInitFact {
      span: self.span(span),
      binding: name,
      kind,
      escaped: self.indexes.payload_uncertain(root),
    });
  }

  pub(super) fn collect_shared_object_bindings(&mut self) {
    let mut seen = HashSet::new();
    for (symbol, init) in &self.indexes.init_span {
      self.indexes.note_query();
      let root = self.indexes.root_of(*symbol);
      if !seen.insert(root) {
        continue;
      }
      let object_span = self.indexes.literal_span.get(&span_key(*init)).copied().unwrap_or(*init);
      let Some(hint) = self.indexes.hints.get(&span_key(object_span)).copied() else {
        continue;
      };
      if hint != ShapeHint::PlainRecord {
        continue;
      }
      if self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::Function) {
        continue;
      }
      let name = self.symbol_name(root);
      let Some(object) = self.indexes.closed_object_literal(object_span) else {
        continue;
      };
      if !object {
        continue;
      }
      let mut own_paths = Vec::new();
      if let Some(props) = self.indexes.object_props.get(&span_key(object_span)) {
        for (key, prop) in props {
          self.indexes.note_query();
          let super::index::ObjectProp::Value(value) = *prop else {
            continue;
          };
          if let Some(kind) = self.literal_kind_at(value) {
            own_paths.push((key.clone(), kind));
          }
        }
      }
      own_paths.sort_by(|left, right| left.0.cmp(&right.0));
      self.facts.shared_object_bindings.push(SharedObjectBindingFact {
        span: self.span(object_span),
        binding: name,
        own_paths,
        escaped: self.indexes.reassigned.contains(&root)
          || self.indexes.unknown_member_touch.contains(&root),
      });
    }
  }

  fn collect_native_demand(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    mounted: &HashSet<NodeId>,
  ) {
    if call.optional {
      return;
    }
    let Some((chain, member)) = callee_chain(&call.callee) else {
      return;
    };
    if !NATIVE_MEMBERS.contains(&member.as_str()) {
      return;
    }
    let optional = chain_optional(&call.callee);
    let guarded = self.demand_guarded(node_id);
    let owner = self.indexes.owner(node_id);
    let in_mounted = owner.callable.is_some_and(|callable| mounted.contains(&callable));
    if chain.len() == 2 && chain.get(1).map(String::as_str) == Some("value") && in_mounted {
      if let Some(receiver) = chain.first() {
        self.facts.mounted_member_demands.push(MountedMemberDemandFact {
          span: self.span(call.span),
          receiver: receiver.clone(),
          member,
          optional,
          guarded,
        });
      }
      return;
    }
    if chain.len() >= 3
      && chain.get(1).map(String::as_str) == Some("value")
      && let Some(instance) = chain.first()
    {
      self.facts.instance_member_demands.push(InstanceMemberDemandFact {
        span: self.span(call.span),
        instance: instance.clone(),
        path: chain.get(2..).unwrap_or(&[]).to_vec(),
        member,
        optional,
        guarded,
      });
    }
  }

  fn collect_chain_write(&mut self, left: &AssignmentTarget<'_>, right: &Expression<'_>) {
    let Some((chain, last)) = assignment_chain(left) else {
      return;
    };
    let rhs_kind = expression_init_kind(right);
    if chain.len() == 1 && last == "value" {
      let Some(binding) = chain.first() else {
        return;
      };
      let Some(model) = self
        .facts
        .model_defaults
        .iter()
        .find(|model| model.binding.as_deref() == Some(binding.as_str()))
      else {
        return;
      };
      let rhs_text = literal_primitive_text(right);
      let unchanged = model.origin == ModelDefaultOrigin::LiteralPrimitive
        && model.primitive_text.is_some()
        && rhs_text.as_ref() == model.primitive_text.as_ref();
      self.facts.model_value_writes.push(ModelValueWriteFact {
        span: self.span(left.span()),
        binding: binding.clone(),
        rhs_kind,
        rhs_text,
        unchanged_default: unchanged,
      });
      return;
    }
    if chain.len() >= 2
      && chain.get(1).map(String::as_str) == Some("value")
      && let Some(instance) = chain.first()
    {
      let mut path = chain.get(2..).unwrap_or(&[]).to_vec();
      path.push(last);
      self.facts.instance_path_writes.push(InstancePathWriteFact {
        span: self.span(left.span()),
        instance: instance.clone(),
        path,
        rhs_kind,
      });
    }
  }

  fn demand_guarded(&self, node_id: NodeId) -> bool {
    if self.ancestor_guarded(node_id) {
      return true;
    }
    self.preceding_early_return_guard(node_id)
  }

  fn ancestor_guarded(&self, node_id: NodeId) -> bool {
    let mut current = node_id;
    let owner = self.indexes.owner(node_id).callable;
    for _ in 0..12 {
      self.indexes.note_query();
      let parent = self.semantic.nodes().parent_id(current);
      if owner.is_some_and(|callable| parent == callable) {
        return false;
      }
      match self.semantic.nodes().kind(parent) {
        AstKind::IfStatement(_)
        | AstKind::ConditionalExpression(_)
        | AstKind::LogicalExpression(_)
        | AstKind::ChainExpression(_) => return true,
        AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Typeof => {
          return true;
        }
        AstKind::BinaryExpression(binary)
          if matches!(
            binary.operator,
            BinaryOperator::Equality
              | BinaryOperator::Inequality
              | BinaryOperator::StrictEquality
              | BinaryOperator::StrictInequality
          ) =>
        {
          return true;
        }
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
          return false;
        }
        _ => current = parent,
      }
    }
    true
  }

  fn preceding_early_return_guard(&self, node_id: NodeId) -> bool {
    let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
      return false;
    };
    let Some((chain, _)) = callee_chain(&call.callee) else {
      return false;
    };
    let Some(receiver) = chain.first() else {
      return false;
    };
    let Some(body) = self.enclosing_function_body(node_id) else {
      return false;
    };
    let demand_start = call.span.start;
    for statement in &body.statements {
      self.indexes.note_query();
      if statement.span().start >= demand_start {
        break;
      }
      if statement_exits_on_receiver(statement, receiver) {
        return true;
      }
    }
    false
  }

  fn enclosing_function_body(&self, node_id: NodeId) -> Option<&FunctionBody<'_>> {
    let mut current = node_id;
    for _ in 0..24 {
      self.indexes.note_query();
      let parent = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent) {
        AstKind::ArrowFunctionExpression(arrow) => return Some(&arrow.body),
        AstKind::Function(function) => return function.body.as_deref(),
        AstKind::Program(_) => return None,
        _ => current = parent,
      }
    }
    None
  }

  fn result_binding(&self, node_id: NodeId) -> Option<SymbolId> {
    let mut current = node_id;
    for _ in 0..6 {
      self.indexes.note_query();
      let parent = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent) {
        AstKind::VariableDeclarator(declarator) => {
          return match &declarator.id {
            BindingPattern::BindingIdentifier(ident) => ident.symbol_id.get(),
            _ => None,
          };
        }
        AstKind::TSAsExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::ParenthesizedExpression(_) => current = parent,
        _ => return None,
      }
    }
    None
  }

  fn literal_kind_at(&self, span: Span) -> Option<ModelPrimitiveKind> {
    self.indexes.note_query();
    let mapped = self.span(span);
    let text = self.sfc_source.get(mapped.offset..mapped.offset.saturating_add(mapped.length))?;
    let trimmed = text.trim();
    if trimmed.parse::<f64>().is_ok() {
      return Some(ModelPrimitiveKind::Number);
    }
    if trimmed == "true" || trimmed == "false" {
      return Some(ModelPrimitiveKind::Boolean);
    }
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
      || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
      || (trimmed.starts_with('`') && trimmed.ends_with('`') && !trimmed.contains("${"))
    {
      return Some(ModelPrimitiveKind::String);
    }
    None
  }
}

struct ClassifiedDefault {
  origin: ModelDefaultOrigin,
  primitive: Option<ModelPrimitiveKind>,
  primitive_text: Option<String>,
  shared_binding: Option<String>,
  own_paths: Vec<(String, ModelPrimitiveKind)>,
}

fn classify_default(expression: &Expression<'_>) -> ClassifiedDefault {
  let inner = expression.get_inner_expression();
  if let Some((kind, text)) = literal_primitive_parts(inner) {
    return ClassifiedDefault {
      origin: ModelDefaultOrigin::LiteralPrimitive,
      primitive: Some(kind),
      primitive_text: Some(text),
      shared_binding: None,
      own_paths: Vec::new(),
    };
  }
  match inner {
    Expression::ObjectExpression(object) => {
      return ClassifiedDefault {
        origin: ModelDefaultOrigin::SharedObjectLiteral,
        primitive: None,
        primitive_text: None,
        shared_binding: None,
        own_paths: object_literal_paths(object),
      };
    }
    Expression::ArrayExpression(_) => {
      return ClassifiedDefault {
        origin: ModelDefaultOrigin::SharedObjectLiteral,
        primitive: None,
        primitive_text: None,
        shared_binding: None,
        own_paths: Vec::new(),
      };
    }
    _ => {}
  }
  let Some(body) = factory_body(inner) else {
    return ClassifiedDefault {
      origin: ModelDefaultOrigin::Unknown,
      primitive: None,
      primitive_text: None,
      shared_binding: None,
      own_paths: Vec::new(),
    };
  };
  match body {
    FactoryBody::Object => ClassifiedDefault {
      origin: ModelDefaultOrigin::FreshObjectFactory,
      primitive: None,
      primitive_text: None,
      shared_binding: None,
      own_paths: Vec::new(),
    },
    FactoryBody::Identifier(name) => ClassifiedDefault {
      origin: ModelDefaultOrigin::SharedObjectFactory,
      primitive: None,
      primitive_text: None,
      shared_binding: Some(name),
      own_paths: Vec::new(),
    },
    FactoryBody::Unknown => ClassifiedDefault {
      origin: ModelDefaultOrigin::Unknown,
      primitive: None,
      primitive_text: None,
      shared_binding: None,
      own_paths: Vec::new(),
    },
  }
}

fn object_literal_paths(object: &ObjectExpression<'_>) -> Vec<(String, ModelPrimitiveKind)> {
  let mut own_paths = Vec::new();
  for property in &object.properties {
    let ObjectPropertyKind::ObjectProperty(prop) = property else {
      continue;
    };
    if prop.computed {
      continue;
    }
    let Some(name) = static_key(&prop.key) else {
      continue;
    };
    if let Some((kind, _)) = literal_primitive_parts(prop.value.get_inner_expression()) {
      own_paths.push((name.to_owned(), kind));
    }
  }
  own_paths.sort_by(|left, right| left.0.cmp(&right.0));
  own_paths
}

fn statement_exits_on_receiver(statement: &Statement<'_>, receiver: &str) -> bool {
  let Statement::IfStatement(if_statement) = statement else {
    return false;
  };
  if !test_guards_receiver(&if_statement.test, receiver) {
    return false;
  }
  statement_exits(&if_statement.consequent)
}

fn statement_exits(statement: &Statement<'_>) -> bool {
  match statement {
    Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
    Statement::BlockStatement(block) => block.body.iter().any(statement_exits),
    _ => false,
  }
}

fn test_guards_receiver(test: &Expression<'_>, receiver: &str) -> bool {
  match test.get_inner_expression() {
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
      reads_receiver(&unary.argument, receiver)
    }
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Typeof => {
      reads_receiver(&unary.argument, receiver)
    }
    Expression::BinaryExpression(binary)
      if matches!(
        binary.operator,
        BinaryOperator::Equality
          | BinaryOperator::Inequality
          | BinaryOperator::StrictEquality
          | BinaryOperator::StrictInequality
      ) =>
    {
      let left = binary.left.get_inner_expression();
      let right = binary.right.get_inner_expression();
      (reads_receiver(left, receiver) && is_nullish_or_undefined_string(right))
        || (reads_receiver(right, receiver) && is_nullish_or_undefined_string(left))
        || (typeof_receiver(left, receiver) && is_undefined_string(right))
        || (typeof_receiver(right, receiver) && is_undefined_string(left))
    }
    _ => false,
  }
}

fn typeof_receiver(expression: &Expression<'_>, receiver: &str) -> bool {
  match expression.get_inner_expression() {
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Typeof => {
      reads_receiver(&unary.argument, receiver)
    }
    _ => false,
  }
}

fn reads_receiver(expression: &Expression<'_>, receiver: &str) -> bool {
  match expression.get_inner_expression() {
    Expression::Identifier(ident) => ident.name == receiver,
    Expression::StaticMemberExpression(member)
      if member.property.name == "value"
        && let Expression::Identifier(ident) = member.object.get_inner_expression() =>
    {
      ident.name == receiver
    }
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Typeof => {
      reads_receiver(&unary.argument, receiver)
    }
    _ => false,
  }
}

fn is_nullish_or_undefined_string(expression: &Expression<'_>) -> bool {
  match expression.get_inner_expression() {
    Expression::Identifier(ident) if ident.name == "undefined" => true,
    Expression::NullLiteral(_) => true,
    Expression::StringLiteral(literal) if literal.value == "undefined" => true,
    _ => false,
  }
}

fn is_undefined_string(expression: &Expression<'_>) -> bool {
  matches!(
    expression.get_inner_expression(),
    Expression::StringLiteral(literal) if literal.value == "undefined"
  )
}

enum FactoryBody {
  Object,
  Identifier(String),
  Unknown,
}

fn factory_body(expression: &Expression<'_>) -> Option<FactoryBody> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.r#async || !arrow.params.items.is_empty() {
        return Some(FactoryBody::Unknown);
      }
      if arrow.expression {
        let Some(Statement::ExpressionStatement(statement)) = arrow.body.statements.first() else {
          return Some(FactoryBody::Unknown);
        };
        return Some(value_body(&statement.expression));
      }
      Some(block_return(&arrow.body))
    }
    Expression::FunctionExpression(function) => {
      if function.r#async || !function.params.items.is_empty() {
        return Some(FactoryBody::Unknown);
      }
      let body = function.body.as_ref()?;
      Some(block_return(body))
    }
    _ => None,
  }
}

fn block_return(body: &FunctionBody<'_>) -> FactoryBody {
  let mut returned = None;
  for statement in &body.statements {
    match statement {
      Statement::ReturnStatement(ret) => {
        let Some(argument) = &ret.argument else {
          return FactoryBody::Unknown;
        };
        if returned.is_some() {
          return FactoryBody::Unknown;
        }
        returned = Some(value_body(argument));
      }
      Statement::EmptyStatement(_) => {}
      _ => return FactoryBody::Unknown,
    }
  }
  returned.unwrap_or(FactoryBody::Unknown)
}

fn value_body(expression: &Expression<'_>) -> FactoryBody {
  match expression.get_inner_expression() {
    Expression::ObjectExpression(_) => FactoryBody::Object,
    Expression::Identifier(identifier) => FactoryBody::Identifier(identifier.name.to_string()),
    _ => FactoryBody::Unknown,
  }
}

fn model_name_of(call: &CallExpression<'_>) -> String {
  call
    .arguments
    .first()
    .and_then(Argument::as_expression)
    .and_then(|expression| match expression.get_inner_expression() {
      Expression::StringLiteral(literal) => Some(literal.value.to_string()),
      _ => None,
    })
    .unwrap_or_else(|| "modelValue".into())
}

fn model_options<'a>(
  call: &'a CallExpression<'a>,
) -> Option<&'a oxc_ast::ast::ObjectExpression<'a>> {
  let first = call.arguments.first().and_then(Argument::as_expression)?;
  match first.get_inner_expression() {
    Expression::ObjectExpression(object) => Some(object.as_ref()),
    Expression::StringLiteral(_) => {
      call.arguments.get(1).and_then(Argument::as_expression).and_then(
        |expression| match expression.get_inner_expression() {
          Expression::ObjectExpression(object) => Some(object.as_ref()),
          _ => None,
        },
      )
    }
    _ => None,
  }
}

fn static_key<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
  match key {
    PropertyKey::StaticIdentifier(ident) => Some(ident.name.as_str()),
    PropertyKey::StringLiteral(literal) => Some(literal.value.as_str()),
    _ => None,
  }
}

fn literal_primitive(expression: &Expression<'_>) -> Option<ModelPrimitiveKind> {
  literal_primitive_parts(expression).map(|(kind, _)| kind)
}

fn literal_primitive_parts(expression: &Expression<'_>) -> Option<(ModelPrimitiveKind, String)> {
  match expression.get_inner_expression() {
    Expression::NumericLiteral(literal) => {
      Some((ModelPrimitiveKind::Number, normalize_number(literal.value)))
    }
    Expression::StringLiteral(literal) => {
      Some((ModelPrimitiveKind::String, literal.value.to_string()))
    }
    Expression::BooleanLiteral(literal) => {
      Some((ModelPrimitiveKind::Boolean, literal.value.to_string()))
    }
    Expression::TemplateLiteral(literal)
      if literal.expressions.is_empty() && literal.quasis.len() == 1 =>
    {
      let text = literal
        .quasis
        .first()
        .and_then(|quasi| quasi.value.cooked.as_ref().map(std::string::ToString::to_string))?;
      Some((ModelPrimitiveKind::String, text))
    }
    _ => None,
  }
}

fn literal_primitive_text(expression: &Expression<'_>) -> Option<String> {
  literal_primitive_parts(expression).map(|(_, text)| text)
}

fn normalize_number(value: f64) -> String {
  if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
    #[expect(clippy::cast_possible_truncation, reason = "safe integer range is checked above")]
    return (value as i64).to_string();
  }
  value.to_string()
}

fn expression_init_kind(expression: &Expression<'_>) -> RefInitKind {
  match expression.get_inner_expression() {
    Expression::NumericLiteral(_) => RefInitKind::Number,
    Expression::StringLiteral(_) => RefInitKind::String,
    Expression::BooleanLiteral(_) => RefInitKind::Boolean,
    Expression::NullLiteral(_) => RefInitKind::Null,
    Expression::Identifier(ident) if ident.name == "undefined" => RefInitKind::Undefined,
    Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => RefInitKind::Object,
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => RefInitKind::String,
    _ => RefInitKind::Unknown,
  }
}

fn callee_chain(callee: &Expression<'_>) -> Option<(Vec<String>, String)> {
  let mut optional = false;
  let mut current = callee.get_inner_expression();
  let member = match current {
    Expression::StaticMemberExpression(member) => {
      optional |= member.optional;
      let name = member.property.name.to_string();
      current = member.object.get_inner_expression();
      name
    }
    _ => return None,
  };
  let mut chain = Vec::new();
  loop {
    match current {
      Expression::Identifier(ident) => {
        chain.push(ident.name.to_string());
        chain.reverse();
        let _ = optional;
        return Some((chain, member));
      }
      Expression::StaticMemberExpression(next) => {
        optional |= next.optional;
        chain.push(next.property.name.to_string());
        current = next.object.get_inner_expression();
      }
      Expression::ComputedMemberExpression(_) | Expression::PrivateFieldExpression(_) => {
        return None;
      }
      _ => return None,
    }
  }
}

fn chain_optional(callee: &Expression<'_>) -> bool {
  let mut current = callee.get_inner_expression();
  loop {
    match current {
      Expression::StaticMemberExpression(member) => {
        if member.optional {
          return true;
        }
        current = member.object.get_inner_expression();
      }
      Expression::ChainExpression(_) => return true,
      _ => return false,
    }
  }
}

fn assignment_chain(target: &AssignmentTarget<'_>) -> Option<(Vec<String>, String)> {
  let AssignmentTarget::StaticMemberExpression(member) = target else {
    return None;
  };
  let member_name = member.property.name.to_string();
  let mut chain = Vec::new();
  let mut current = member.object.get_inner_expression();
  loop {
    match current {
      Expression::Identifier(ident) => {
        chain.push(ident.name.to_string());
        chain.reverse();
        return Some((chain, member_name));
      }
      Expression::StaticMemberExpression(next) => {
        chain.push(next.property.name.to_string());
        current = next.object.get_inner_expression();
      }
      _ => return None,
    }
  }
}
