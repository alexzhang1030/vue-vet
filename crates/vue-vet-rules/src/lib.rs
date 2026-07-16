use vue_vet_core::{
  Confidence, Rule, RuleContext, RuleMeta, RuleRegistry, Severity, SourceSpan, TemplateElementFact,
};

const NO_V_HTML_META: RuleMeta = RuleMeta {
  id: "vue-vet/security/no-v-html",
  category: "security",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/security/no-v-html",
};
const REQUIRE_V_FOR_KEY_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/require-v-for-key",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/require-v-for-key",
};
const NO_V_IF_WITH_V_FOR_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-v-if-with-v-for",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-v-if-with-v-for",
};
const NO_DUPLICATE_DEFINE_PROPS_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-props",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-props",
};
const VALID_V_HTML_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-html",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-html",
};
const VALID_V_TEXT_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-text",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-text",
};
const REQUIRE_COMPONENT_IS_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/require-component-is",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/require-component-is",
};
const IMG_HAS_ALT_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/img-has-alt",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/img-has-alt",
};
const IFRAME_HAS_TITLE_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/iframe-has-title",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/iframe-has-title",
};
const ANCHOR_HAS_CONTENT_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/anchor-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/anchor-has-content",
};
const BUTTON_HAS_CONTENT_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/button-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/button-has-content",
};
const CLICK_HAS_KEY_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/click-events-have-key-events",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/click-events-have-key-events",
};
const NO_AUTOFOCUS_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-autofocus",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-autofocus",
};
const NO_NATIVE_MODIFIER_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-deprecated-v-on-native-modifier",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-deprecated-v-on-native-modifier",
};
const NO_MUTATING_PROPS_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-mutating-props",
  category: "reactivity",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-mutating-props",
};

#[derive(Clone, Copy)]
enum TemplateCheck {
  NoVHtml,
  RequireVForKey,
  NoVIfWithVFor,
  ValidVHtml,
  ValidVText,
  RequireComponentIs,
  ImgHasAlt,
  IframeHasTitle,
  AnchorHasContent,
  ButtonHasContent,
  ClickHasKey,
  NoAutofocus,
  NoNativeModifier,
}

struct TemplateRule {
  meta: &'static RuleMeta,
  check: TemplateCheck,
}

struct Finding {
  span: SourceSpan,
  message: &'static str,
  help: &'static str,
}

impl Rule for TemplateRule {
  fn meta(&self) -> &'static RuleMeta {
    self.meta
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let findings = context
      .template()
      .elements
      .iter()
      .filter_map(|element| check_template(self.check, element))
      .collect::<Vec<_>>();
    for finding in findings {
      context.report(self.meta(), finding.span, finding.message.into(), Some(finding.help.into()));
    }
  }
}

fn check_template(check: TemplateCheck, element: &TemplateElementFact) -> Option<Finding> {
  match check {
    TemplateCheck::NoVHtml => element.directive("html").map(|directive| Finding {
      span: directive.span.clone(),
      message: "`v-html` can render untrusted HTML into the page",
      help: "Prefer normal template interpolation. If raw HTML is required, \
        sanitize it at the trust boundary.",
    }),
    TemplateCheck::RequireVForKey => {
      element.directive("for").filter(|_| !element.has_key()).map(|directive| Finding {
        span: directive.span.clone(),
        message: "`v-for` requires a stable `:key`",
        help: "Bind a stable identity from the item; do not use the array index \
          when order can change.",
      })
    }
    TemplateCheck::NoVIfWithVFor => element
      .directive("for")
      .filter(|_| element.directive("if").is_some())
      .map(|directive| Finding {
        span: directive.span.clone(),
        message: "`v-if` and `v-for` on the same element have surprising precedence",
        help: "Move `v-if` to a wrapping `<template>` or pre-filter the collection.",
      }),
    TemplateCheck::ValidVHtml => invalid_content_directive(element, "html"),
    TemplateCheck::ValidVText => invalid_content_directive(element, "text"),
    TemplateCheck::RequireComponentIs => (element.tag.eq_ignore_ascii_case("component")
      && element.attribute("is").is_none()
      && element.bound_attribute("is").is_none())
    .then_some(Finding {
      span: element.span.clone(),
      message: "dynamic `<component>` requires an `is` binding",
      help: "Add `:is=\"component\"` with a component definition or registered component name.",
    }),
    TemplateCheck::ImgHasAlt => missing_accessible_name(element, "img", "alt", "image"),
    TemplateCheck::IframeHasTitle => {
      missing_accessible_name(element, "iframe", "title", "inline frame")
    }
    TemplateCheck::AnchorHasContent => empty_control(element, "a", "link"),
    TemplateCheck::ButtonHasContent => empty_control(element, "button", "button"),
    TemplateCheck::ClickHasKey => click_without_keyboard(element),
    TemplateCheck::NoAutofocus => element.attribute("autofocus").map(|attribute| Finding {
      span: attribute.span.clone(),
      message: "autofocus can disorient keyboard and screen-reader users",
      help: "Let users choose focus, or move focus programmatically only after \
        an explicit interaction.",
    }),
    TemplateCheck::NoNativeModifier => element
      .directives
      .iter()
      .find(|directive| {
        directive.name == "on" && directive.modifiers.iter().any(|modifier| modifier == "native")
      })
      .map(|directive| Finding {
        span: directive.span.clone(),
        message: "the `.native` event modifier was removed in Vue 3",
        help: "Declare emitted events on the child component; undeclared \
          listeners fall through natively.",
      }),
  }
}

fn invalid_content_directive(element: &TemplateElementFact, name: &str) -> Option<Finding> {
  let directive = element.directive(name)?;
  let invalid = directive.expression.as_deref().is_none_or(str::is_empty)
    || directive.argument.is_some()
    || !directive.modifiers.is_empty()
    || element.has_children;
  invalid.then_some(Finding {
    span: directive.span.clone(),
    message: if name == "html" { "invalid `v-html` usage" } else { "invalid `v-text` usage" },
    help: "Provide exactly one expression, no argument or modifiers, and no child content.",
  })
}

fn missing_accessible_name(
  element: &TemplateElementFact,
  tag: &str,
  attribute: &str,
  noun: &'static str,
) -> Option<Finding> {
  (element.tag.eq_ignore_ascii_case(tag)
    && element.attribute(attribute).is_none()
    && element.bound_attribute(attribute).is_none())
  .then_some(Finding {
    span: element.span.clone(),
    message: if tag == "img" {
      "image is missing an `alt` attribute"
    } else {
      "iframe is missing a `title` attribute"
    },
    help: if noun == "image" {
      "Describe meaningful images, or use alt=\"\" for decorative images."
    } else {
      "Add a concise title describing the embedded content."
    },
  })
}

fn empty_control(element: &TemplateElementFact, tag: &str, noun: &'static str) -> Option<Finding> {
  (element.tag.eq_ignore_ascii_case(tag)
    && !element.has_children
    && element.attribute("aria-label").is_none()
    && element.bound_attribute("aria-label").is_none()
    && element.attribute("aria-labelledby").is_none()
    && element.bound_attribute("aria-labelledby").is_none())
  .then_some(Finding {
    span: element.span.clone(),
    message: if noun == "link" {
      "link has no accessible content"
    } else {
      "button has no accessible content"
    },
    help: "Add visible content or an aria-label/aria-labelledby binding.",
  })
}

fn click_without_keyboard(element: &TemplateElementFact) -> Option<Finding> {
  const NON_INTERACTIVE: [&str; 6] = ["div", "span", "p", "section", "article", "li"];
  let is_non_interactive = NON_INTERACTIVE.iter().any(|tag| element.tag.eq_ignore_ascii_case(tag));
  (is_non_interactive
    && element.event("click").is_some()
    && element.event("keydown").is_none()
    && element.event("keyup").is_none()
    && element.event("keypress").is_none())
  .then_some(Finding {
    span: element.event("click").map_or_else(|| element.span.clone(), |event| event.span.clone()),
    message: "clickable non-interactive element has no keyboard handler",
    help: "Prefer a native button or link; otherwise add keyboard behavior and \
      an appropriate role.",
  })
}

struct NoMutatingProps;

struct NoDuplicateDefineProps;

impl Rule for NoDuplicateDefineProps {
  fn meta(&self) -> &'static RuleMeta {
    &NO_DUPLICATE_DEFINE_PROPS_META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == vue_vet_core::ScriptKind::Setup)
      .flat_map(|block| block.calls.iter().filter(|call| call.callee == "defineProps").skip(1))
      .map(|call| call.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "`defineProps` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineProps` call.".into()),
      );
    }
  }
}

impl Rule for NoMutatingProps {
  fn meta(&self) -> &'static RuleMeta {
    &NO_MUTATING_PROPS_META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| {
        block.kind == vue_vet_core::ScriptKind::Setup
          && block.calls.iter().any(|call| call.callee == "defineProps")
      })
      .flat_map(|block| &block.member_writes)
      .filter(|write| write.object == "props")
      .map(|write| write.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "props are readonly and must not be mutated".into(),
        Some("Emit an event or copy the prop into local state owned by this component.".into()),
      );
    }
  }
}

macro_rules! template_rule {
  ($name:ident, $meta:ident, $check:ident) => {
    static $name: TemplateRule = TemplateRule { meta: &$meta, check: TemplateCheck::$check };
  };
}

template_rule!(NO_V_HTML, NO_V_HTML_META, NoVHtml);
template_rule!(REQUIRE_V_FOR_KEY, REQUIRE_V_FOR_KEY_META, RequireVForKey);
template_rule!(NO_V_IF_WITH_V_FOR, NO_V_IF_WITH_V_FOR_META, NoVIfWithVFor);
template_rule!(VALID_V_HTML, VALID_V_HTML_META, ValidVHtml);
template_rule!(VALID_V_TEXT, VALID_V_TEXT_META, ValidVText);
template_rule!(REQUIRE_COMPONENT_IS, REQUIRE_COMPONENT_IS_META, RequireComponentIs);
template_rule!(IMG_HAS_ALT, IMG_HAS_ALT_META, ImgHasAlt);
template_rule!(IFRAME_HAS_TITLE, IFRAME_HAS_TITLE_META, IframeHasTitle);
template_rule!(ANCHOR_HAS_CONTENT, ANCHOR_HAS_CONTENT_META, AnchorHasContent);
template_rule!(BUTTON_HAS_CONTENT, BUTTON_HAS_CONTENT_META, ButtonHasContent);
template_rule!(CLICK_HAS_KEY, CLICK_HAS_KEY_META, ClickHasKey);
template_rule!(NO_AUTOFOCUS, NO_AUTOFOCUS_META, NoAutofocus);
template_rule!(NO_NATIVE_MODIFIER, NO_NATIVE_MODIFIER_META, NoNativeModifier);
static NO_MUTATING_PROPS: NoMutatingProps = NoMutatingProps;
static NO_DUPLICATE_DEFINE_PROPS: NoDuplicateDefineProps = NoDuplicateDefineProps;

#[must_use]
pub fn builtin_registry() -> RuleRegistry {
  RuleRegistry::new(vec![
    &NO_V_HTML,
    &REQUIRE_V_FOR_KEY,
    &NO_V_IF_WITH_V_FOR,
    &NO_DUPLICATE_DEFINE_PROPS,
    &VALID_V_HTML,
    &VALID_V_TEXT,
    &REQUIRE_COMPONENT_IS,
    &IMG_HAS_ALT,
    &IFRAME_HAS_TITLE,
    &ANCHOR_HAS_CONTENT,
    &BUTTON_HAS_CONTENT,
    &CLICK_HAS_KEY,
    &NO_AUTOFOCUS,
    &NO_NATIVE_MODIFIER,
    &NO_MUTATING_PROPS,
  ])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn builtins_have_stable_metadata() {
    let metadata = builtin_registry().metadata();
    assert_eq!(metadata.len(), 15, "the recommended preset contains fifteen rules");
    assert!(
      metadata.windows(2).all(|pair| matches!(pair, [first, second] if first.id < second.id)),
      "registry metadata must be sorted by stable rule ID"
    );
    assert!(
      metadata.iter().all(|meta| meta.confidence == Confidence::High),
      "the recommended preset must contain only high-confidence rules"
    );
  }
}
