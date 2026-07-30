//! Template / macro Essential gap rules (shared directive harness).

use vue_vet_core::{
  Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, ScriptKind, Severity,
  TemplateElementFact,
};

struct MissingExprRule {
  meta: &'static RuleMeta,
  directive: &'static str,
}

impl Rule for MissingExprRule {
  fn meta(&self) -> &'static RuleMeta {
    self.meta
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    let Some(directive) = element.directive(self.directive) else {
      return;
    };
    if directive.expression.as_ref().is_some_and(|expression| !expression.trim().is_empty()) {
      return;
    }
    context.report(
      self.meta(),
      directive.span.clone(),
      format!("`v-{}` is missing an expression", self.directive),
      Some(format!("Provide an expression for `v-{}`.", self.directive)),
    );
  }
}

macro_rules! missing_expr {
  ($static_name:ident, $id:literal, $doc:literal, $directive:literal) => {
    static $static_name: MissingExprRule = MissingExprRule {
      meta: &RuleMeta {
        id: $id,
        category: "correctness",
        default_severity: Severity::Error,
        confidence: Confidence::High,
        documentation: $doc,
      },
      directive: $directive,
    };
  };
}

missing_expr!(VALID_V_IF, "vue-vet/correctness/valid-v-if", "rules/correctness/valid-v-if", "if");
missing_expr!(
  VALID_V_ELSE_IF,
  "vue-vet/correctness/valid-v-else-if",
  "rules/correctness/valid-v-else-if",
  "else-if"
);
missing_expr!(
  VALID_V_SHOW,
  "vue-vet/correctness/valid-v-show",
  "rules/correctness/valid-v-show",
  "show"
);
missing_expr!(
  VALID_V_MODEL,
  "vue-vet/correctness/valid-v-model",
  "rules/correctness/valid-v-model",
  "model"
);
missing_expr!(
  VALID_V_FOR,
  "vue-vet/correctness/valid-v-for",
  "rules/correctness/valid-v-for",
  "for"
);
missing_expr!(
  VALID_V_MEMO,
  "vue-vet/correctness/valid-v-memo",
  "rules/correctness/valid-v-memo",
  "memo"
);

const VALID_V_ON_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-on",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-on",
};

struct ValidVOn;
static VALID_V_ON: ValidVOn = ValidVOn;

impl Rule for ValidVOn {
  fn meta(&self) -> &'static RuleMeta {
    &VALID_V_ON_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    for directive in &element.directives {
      if directive.name != "on" {
        continue;
      }
      if directive.argument.as_ref().is_some_and(|argument| !argument.is_empty()) {
        continue;
      }
      // v-on="listeners" object form is valid without argument.
      if directive.expression.as_ref().is_some_and(|expression| !expression.trim().is_empty()) {
        continue;
      }
      context.report(
        self.meta(),
        directive.span.clone(),
        "`v-on` is missing an event name or listeners expression".into(),
        Some("Use `v-on:event` / `@event`, or `v-on=\"listeners\"`.".into()),
      );
    }
  }
}

const VALID_V_ELSE_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-else",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-else",
};

struct ValidVElse;
static VALID_V_ELSE: ValidVElse = ValidVElse;

impl Rule for ValidVElse {
  fn meta(&self) -> &'static RuleMeta {
    &VALID_V_ELSE_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    let Some(directive) = element.directive("else") else {
      return;
    };
    if directive.expression.as_ref().is_some_and(|expression| !expression.trim().is_empty()) {
      context.report(
        self.meta(),
        directive.span.clone(),
        "`v-else` does not accept an expression".into(),
        Some("Use `v-else-if=\"…\"` when a condition is required.".into()),
      );
    }
  }
}

const VALID_V_BIND_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-bind",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-bind",
};

struct ValidVBind;
static VALID_V_BIND: ValidVBind = ValidVBind;

impl Rule for ValidVBind {
  fn meta(&self) -> &'static RuleMeta {
    &VALID_V_BIND_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    for directive in &element.directives {
      if directive.name != "bind" {
        continue;
      }
      let has_arg = directive.argument.as_ref().is_some_and(|argument| !argument.is_empty());
      let has_expr =
        directive.expression.as_ref().is_some_and(|expression| !expression.trim().is_empty());
      if has_arg || has_expr {
        continue;
      }
      context.report(
        self.meta(),
        directive.span.clone(),
        "`v-bind` is missing an attribute name or object expression".into(),
        Some("Use `v-bind:attr` / `:attr`, or `v-bind=\"object\"`.".into()),
      );
    }
  }
}

const NO_CHILD_CONTENT_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-child-content",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-child-content",
};

struct NoChildContent;
static NO_CHILD_CONTENT: NoChildContent = NoChildContent;

impl Rule for NoChildContent {
  fn meta(&self) -> &'static RuleMeta {
    &NO_CHILD_CONTENT_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !element.has_children {
      return;
    }
    for name in ["html", "text"] {
      if let Some(directive) = element.directive(name) {
        context.report(
          self.meta(),
          directive.span.clone(),
          format!("`v-{name}` overwrites element children"),
          Some(format!("Remove the children, or drop `v-{name}`.")),
        );
      }
    }
  }
}

const NO_TEXTAREA_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-textarea-mustache",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-textarea-mustache",
};

struct NoTextareaMustache;
static NO_TEXTAREA_MUSTACHE: NoTextareaMustache = NoTextareaMustache;

impl Rule for NoTextareaMustache {
  fn meta(&self) -> &'static RuleMeta {
    &NO_TEXTAREA_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !element.tag.eq_ignore_ascii_case("textarea") {
      return;
    }
    if element.directive("model").is_some() {
      return;
    }
    if !element.has_children && !element.has_accessible_content {
      return;
    }
    context.report(
      self.meta(),
      element.span.clone(),
      "textarea content should use `v-model` instead of interpolation".into(),
      Some("Bind the value with `v-model` on `<textarea>`.".into()),
    );
  }
}

const NO_TEMPLATE_KEY_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-template-key",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-template-key",
};

struct NoTemplateKey;
static NO_TEMPLATE_KEY: NoTemplateKey = NoTemplateKey;

impl Rule for NoTemplateKey {
  fn meta(&self) -> &'static RuleMeta {
    &NO_TEMPLATE_KEY_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !element.tag.eq_ignore_ascii_case("template") {
      return;
    }
    // Vue 3: key on <template v-for> is OK; key on plain template is useless/wrong.
    if element.directive("for").is_some() {
      return;
    }
    if !element.has_key() {
      return;
    }
    context.report(
      self.meta(),
      element.span.clone(),
      "`<template>` should not have a `key` unless it also has `v-for`".into(),
      Some("Move `key` onto a real element, or add `v-for` on the template.".into()),
    );
  }
}

const NO_DUP_ATTR_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-attributes",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-attributes",
};

struct NoDuplicateAttributes;
static NO_DUPLICATE_ATTRIBUTES: NoDuplicateAttributes = NoDuplicateAttributes;

impl Rule for NoDuplicateAttributes {
  fn meta(&self) -> &'static RuleMeta {
    &NO_DUP_ATTR_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    let mut seen = std::collections::BTreeSet::new();
    for attribute in &element.attributes {
      let key = attribute.name.to_ascii_lowercase();
      if !seen.insert(key.clone()) {
        context.report(
          self.meta(),
          attribute.span.clone(),
          format!("duplicate attribute `{}`", attribute.name),
          Some("Remove the duplicate attribute.".into()),
        );
      }
    }
    for directive in &element.directives {
      if directive.name != "bind" {
        continue;
      }
      let Some(argument) = directive.argument.as_deref() else {
        continue;
      };
      let key = format!("bind:{}", argument.to_ascii_lowercase());
      if !seen.insert(key) {
        context.report(
          self.meta(),
          directive.span.clone(),
          format!("duplicate bound attribute `{argument}`"),
          Some("Remove the duplicate `v-bind` / `:` attribute.".into()),
        );
      }
    }
  }
}

const NO_SYNC_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-deprecated-v-bind-sync",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-deprecated-v-bind-sync",
};

struct NoDeprecatedVBindSync;
static NO_DEPRECATED_V_BIND_SYNC: NoDeprecatedVBindSync = NoDeprecatedVBindSync;

impl Rule for NoDeprecatedVBindSync {
  fn meta(&self) -> &'static RuleMeta {
    &NO_SYNC_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    for directive in &element.directives {
      let sync = directive.modifiers.iter().any(|modifier| modifier == "sync")
        || directive.raw_name.contains(".sync");
      if !sync {
        continue;
      }
      context.report(
        self.meta(),
        directive.span.clone(),
        "`.sync` is deprecated; use `v-model:prop` instead".into(),
        Some("Replace `v-bind:prop.sync` with `v-model:prop`.".into()),
      );
    }
  }
}

const NO_SLOT_ATTR_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-deprecated-slot-attribute",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-deprecated-slot-attribute",
};

struct NoDeprecatedSlotAttribute;
static NO_DEPRECATED_SLOT_ATTRIBUTE: NoDeprecatedSlotAttribute = NoDeprecatedSlotAttribute;

impl Rule for NoDeprecatedSlotAttribute {
  fn meta(&self) -> &'static RuleMeta {
    &NO_SLOT_ATTR_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if let Some(attribute) = element.attribute("slot") {
      context.report(
        self.meta(),
        attribute.span.clone(),
        "`slot` attribute is deprecated; use `v-slot` / `#`".into(),
        Some("Replace `slot=\"name\"` with `v-slot:name` or `#name`.".into()),
      );
    }
  }
}

const NO_VHTML_COMPONENT_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-v-text-v-html-on-component",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-v-text-v-html-on-component",
};

struct NoVTextVHtmlOnComponent;
static NO_V_TEXT_V_HTML_ON_COMPONENT: NoVTextVHtmlOnComponent = NoVTextVHtmlOnComponent;

impl Rule for NoVTextVHtmlOnComponent {
  fn meta(&self) -> &'static RuleMeta {
    &NO_VHTML_COMPONENT_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !looks_like_component(element) {
      return;
    }
    for name in ["html", "text"] {
      if let Some(directive) = element.directive(name) {
        context.report(
          self.meta(),
          directive.span.clone(),
          format!("`v-{name}` should not be used on components"),
          Some("Pass content via slots or props instead.".into()),
        );
      }
    }
  }
}

const NO_IMPORT_MACROS_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-import-compiler-macros",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-import-compiler-macros",
};

struct NoImportCompilerMacros;
static NO_IMPORT_COMPILER_MACROS: NoImportCompilerMacros = NoImportCompilerMacros;

impl Rule for NoImportCompilerMacros {
  fn meta(&self) -> &'static RuleMeta {
    &NO_IMPORT_MACROS_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    const MACROS: &[&str] = &[
      "defineProps",
      "defineEmits",
      "defineExpose",
      "defineSlots",
      "defineOptions",
      "defineModel",
      "withDefaults",
    ];
    let mut findings = Vec::new();
    for block in &context.script().blocks {
      for import in &block.imports {
        if import.source != "vue" {
          continue;
        }
        if MACROS.contains(&import.imported.as_str()) {
          findings.push((import.span.clone(), import.imported.clone(), block.kind));
        }
      }
    }
    for (span, name, kind) in findings {
      let help = if kind == ScriptKind::Setup {
        "Remove the import; compiler macros are globally available in `<script setup>`."
      } else {
        "Remove the import; these names are `<script setup>` compiler macros, not runtime exports from `vue`."
      };
      context.report(
        self.meta(),
        span,
        format!("`{name}` is a compiler macro and must not be imported"),
        Some(help.into()),
      );
    }
  }
}

const NO_DUP_MODEL_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-model",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-model",
};

struct NoDuplicateDefineModel;
static NO_DUPLICATE_DEFINE_MODEL: NoDuplicateDefineModel = NoDuplicateDefineModel;

impl Rule for NoDuplicateDefineModel {
  fn meta(&self) -> &'static RuleMeta {
    &NO_DUP_MODEL_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    let mut findings = Vec::new();
    for block in &context.script().blocks {
      if block.kind != ScriptKind::Setup {
        continue;
      }
      let mut seen = std::collections::BTreeSet::new();
      for call in &block.calls {
        if call.callee != "defineModel" {
          continue;
        }
        // Under-approx identity: assignee name, else empty (default modelValue).
        let key = call.assigned_to.as_deref().unwrap_or("");
        if !seen.insert(key.to_owned()) {
          findings.push(call.span.clone());
        }
      }
    }
    for span in findings {
      context.report(
        self.meta(),
        span,
        "duplicate `defineModel` declaration".into(),
        Some("Keep a single `defineModel` per model name.".into()),
      );
    }
  }
}

const VALID_V_SLOT_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-slot",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-slot",
};

struct ValidVSlot;
static VALID_V_SLOT: ValidVSlot = ValidVSlot;

impl Rule for ValidVSlot {
  fn meta(&self) -> &'static RuleMeta {
    &VALID_V_SLOT_META
  }
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }
  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    let Some(slot) = element.directive("slot") else {
      return;
    };
    // `v-for` iterates the whole element; combining it with `v-slot` on the
    // same node makes the slot scope ambiguous (Vue rejects this at compile time).
    if element.directive("for").is_none() {
      return;
    }
    context.report(
      self.meta(),
      slot.span.clone(),
      "`v-slot` cannot be used together with `v-for` on the same element".into(),
      Some("Wrap the `v-for` content in a nested element, or move `v-slot` to the parent `<template>`.".into()),
    );
  }
}

const NO_DUPE_ELSE_IF_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-dupe-v-else-if",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-dupe-v-else-if",
};

struct NoDupeVElseIf;
static NO_DUPE_V_ELSE_IF: NoDupeVElseIf = NoDupeVElseIf;

impl Rule for NoDupeVElseIf {
  fn meta(&self) -> &'static RuleMeta {
    &NO_DUPE_ELSE_IF_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    // Facts are a flat, parent-before-child pre-order list with no explicit
    // sibling links. Two `v-else-if` directives are only compared when they sit
    // on directly adjacent flat entries, which holds for the common case of a
    // leaf-element `v-if`/`v-else-if` chain but under-approximates chains whose
    // earlier branches contain nested elements.
    let elements = &context.template().elements;
    let mut findings = Vec::new();
    for pair in elements.windows(2) {
      let [previous, current] = pair else {
        continue;
      };
      let Some(previous_directive) = previous.directive("else-if") else {
        continue;
      };
      let Some(current_directive) = current.directive("else-if") else {
        continue;
      };
      let Some(previous_expression) = previous_directive.expression.as_deref() else {
        continue;
      };
      let Some(current_expression) = current_directive.expression.as_deref() else {
        continue;
      };
      if previous_expression.trim().is_empty() {
        continue;
      }
      if previous_expression.trim() == current_expression.trim() {
        findings.push((current_directive.span.clone(), current_expression.trim().to_owned()));
      }
    }
    for (span, expression) in findings {
      context.report(
        self.meta(),
        span,
        format!("`v-else-if=\"{expression}\"` repeats the previous branch's condition"),
        Some("The later branch is unreachable; remove it or fix the condition.".into()),
      );
    }
  }
}

const REQUIRE_TOGGLE_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/require-toggle-inside-transition",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/require-toggle-inside-transition",
};

struct RequireToggleInsideTransition;
static REQUIRE_TOGGLE_INSIDE_TRANSITION: RequireToggleInsideTransition =
  RequireToggleInsideTransition;

impl Rule for RequireToggleInsideTransition {
  fn meta(&self) -> &'static RuleMeta {
    &REQUIRE_TOGGLE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    // Facts expose no parent/child links, so the next flat entry is used as a
    // best-effort proxy for the wrapped child: `collect_element` always pushes
    // a parent before recursing into its own element children, so when
    // `has_children` is true the following entry is that first nested element
    // in the common case of a single immediate child. Transition wrappers with
    // only text/comment children, or whose real child is not the very next
    // element, are not checked (documented limitation).
    let elements = &context.template().elements;
    let mut findings = Vec::new();
    for (index, element) in elements.iter().enumerate() {
      if !element.tag.eq_ignore_ascii_case("transition") || !element.has_children {
        continue;
      }
      let Some(child) = elements.get(index.saturating_add(1)) else {
        continue;
      };
      let has_toggle = child.directive("if").is_some()
        || child.directive("show").is_some()
        || child.bound_attribute("is").is_some()
        || child.attribute("is").is_some();
      if has_toggle {
        continue;
      }
      findings.push(element.span.clone());
    }
    for span in findings {
      context.report(
        self.meta(),
        span,
        "`<transition>` wraps content with no `v-if` / `v-show` / dynamic `:is` toggle".into(),
        Some("Add a toggle directive on the transitioned child, or remove the wrapper.".into()),
      );
    }
  }
}

const NO_DEPRECATED_FILTER_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-deprecated-filter",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-deprecated-filter",
};

struct NoDeprecatedFilter;
static NO_DEPRECATED_FILTER: NoDeprecatedFilter = NoDeprecatedFilter;

impl Rule for NoDeprecatedFilter {
  fn meta(&self) -> &'static RuleMeta {
    &NO_DEPRECATED_FILTER_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    // Vue 2 pipe filters are SFC template syntax only — never JSX/TSX.
    if is_jsx_module_path(context.file()) {
      return;
    }
    let template = context.template();
    let mut findings = Vec::new();
    if template.expressions.is_empty() {
      for element in &template.elements {
        for directive in &element.directives {
          let Some(expression) = directive.expression.as_deref() else {
            continue;
          };
          if has_filter_pipe(expression) {
            findings.push(directive.span.clone());
          }
        }
      }
    } else {
      for expression in &template.expressions {
        if has_filter_pipe(&expression.expression) {
          findings.push(expression.span.clone());
        }
      }
    }
    for span in findings {
      context.report(
        self.meta(),
        span,
        "Vue 2 pipe filters (`expr | filterName`) were removed in Vue 3".into(),
        Some("Replace the filter with a method call or a computed property.".into()),
      );
    }
  }
}

fn is_jsx_module_path(path: &std::path::Path) -> bool {
  path
    .extension()
    .and_then(|extension| extension.to_str())
    .is_some_and(|extension| matches!(extension, "jsx" | "tsx"))
}

/// Legacy filter syntax: spaced pipe ` | ` whose RHS looks like a filter name
/// (`ident` or `ident(...)`), not `||`, bitwise-or without spaces, or TS unions
/// like `Foo.Bar | Foo.Baz`.
fn has_filter_pipe(expression: &str) -> bool {
  let bytes = expression.as_bytes();
  let mut index = 0;
  while index < bytes.len() {
    if bytes.get(index) != Some(&b'|') {
      index += 1;
      continue;
    }
    let previous_is_pipe = index > 0 && bytes.get(index - 1) == Some(&b'|');
    let next_is_pipe = bytes.get(index + 1) == Some(&b'|');
    if previous_is_pipe || next_is_pipe {
      index += 1;
      continue;
    }
    let previous_is_space = index > 0 && bytes.get(index - 1) == Some(&b' ');
    let next_is_space = bytes.get(index + 1) == Some(&b' ');
    if !(previous_is_space && next_is_space) {
      index += 1;
      continue;
    }
    let rhs_start = index.saturating_add(2);
    let Some(rhs) = bytes.get(rhs_start..) else {
      index += 1;
      continue;
    };
    if looks_like_filter_rhs(rhs) {
      return true;
    }
    index += 1;
  }
  false
}

fn looks_like_filter_rhs(rhs: &[u8]) -> bool {
  let mut index = 0;
  while rhs.get(index).is_some_and(u8::is_ascii_whitespace) {
    index += 1;
  }
  let Some(&first) = rhs.get(index) else {
    return false;
  };
  if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
    return false;
  }
  index += 1;
  while rhs
    .get(index)
    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'$')
  {
    index += 1;
  }
  while rhs.get(index).is_some_and(u8::is_ascii_whitespace) {
    index += 1;
  }
  // Filter name alone, chained ` | next`, or `filter(arg)`. Reject `Foo.Bar`.
  matches!(rhs.get(index), None | Some(b'|' | b'('))
}

fn looks_like_component(element: &TemplateElementFact) -> bool {
  let tag = element.tag.as_str();
  tag.contains('-') || tag.starts_with(|character: char| character.is_ascii_uppercase())
}

#[must_use]
pub fn directive_rules() -> Vec<&'static dyn Rule> {
  vec![
    &VALID_V_IF,
    &VALID_V_ELSE_IF,
    &VALID_V_ELSE,
    &VALID_V_SHOW,
    &VALID_V_MODEL,
    &VALID_V_FOR,
    &VALID_V_MEMO,
    &VALID_V_ON,
    &VALID_V_BIND,
    &VALID_V_SLOT,
    &NO_CHILD_CONTENT,
    &NO_TEXTAREA_MUSTACHE,
    &NO_TEMPLATE_KEY,
    &NO_DUPLICATE_ATTRIBUTES,
    &NO_DEPRECATED_V_BIND_SYNC,
    &NO_DEPRECATED_SLOT_ATTRIBUTE,
    &NO_V_TEXT_V_HTML_ON_COMPONENT,
    &NO_IMPORT_COMPILER_MACROS,
    &NO_DUPLICATE_DEFINE_MODEL,
    &NO_DUPE_V_ELSE_IF,
    &REQUIRE_TOGGLE_INSIDE_TRANSITION,
    &NO_DEPRECATED_FILTER,
  ]
}

#[cfg(test)]
mod filter_pipe_tests {
  use super::{has_filter_pipe, is_jsx_module_path};
  use std::path::Path;

  #[test]
  fn detects_vue2_filter_pipes() {
    assert!(has_filter_pipe("message | capitalize"));
    assert!(has_filter_pipe("message | capitalize | upper"));
    assert!(has_filter_pipe("n | currency(2)"));
  }

  #[test]
  fn rejects_ts_unions_and_non_filters() {
    assert!(!has_filter_pipe("props.name as PresetAppName.Monitor | PresetAppName.Dashboard"));
    assert!(!has_filter_pipe("Foo.Bar | Foo.Baz"));
    assert!(!has_filter_pipe("a || b"));
    assert!(!has_filter_pipe("a|b"));
  }

  #[test]
  fn jsx_paths_are_recognized() {
    assert!(is_jsx_module_path(Path::new("Comp.tsx")));
    assert!(is_jsx_module_path(Path::new("Comp.jsx")));
    assert!(!is_jsx_module_path(Path::new("Comp.vue")));
  }
}
