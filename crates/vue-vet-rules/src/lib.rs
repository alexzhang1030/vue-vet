use vue_vet_core::{
  Confidence, ReactiveBindingKind, ReactiveReadKind, Rule, RuleContext, RuleMeta, RuleRegistry, Severity, SourceSpan,
  TemplateElementFact,
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
const NO_DUPLICATE_DEFINE_EMITS_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-emits",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-emits",
};
const NO_DUPLICATE_DEFINE_SLOTS_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-slots",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-slots",
};
const NO_DUPLICATE_DEFINE_EXPOSE_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-expose",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-expose",
};
const NO_DUPLICATE_DEFINE_OPTIONS_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-options",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-options",
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
const NO_CONDITIONAL_WATCH_EFFECT_DEPENDENCY_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-conditional-watch-effect-dependency",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-conditional-watch-effect-dependency",
};
const NO_NONREACTIVE_PROPS_DESTRUCTURE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-nonreactive-props-destructure",
  category: "reactivity",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-nonreactive-props-destructure",
};
const PREFER_USE_TEMPLATE_REF_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/prefer-use-template-ref",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/prefer-use-template-ref",
};
const NO_POSITIVE_TABINDEX_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-positive-tabindex",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-positive-tabindex",
};
const NO_ARIA_HIDDEN_ON_FOCUSABLE_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-aria-hidden-on-focusable",
  category: "accessibility",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-aria-hidden-on-focusable",
};
const VALID_ARIA_ROLE_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/valid-aria-role",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/valid-aria-role",
};
const NO_REDUNDANT_ROLE_META: RuleMeta = RuleMeta {
  id: "vue-vet/maintainability/no-redundant-role",
  category: "maintainability",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/maintainability/no-redundant-role",
};
const NO_DEPRECATED_SLOT_SCOPE_META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-deprecated-slot-scope",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-deprecated-slot-scope",
};
const NO_DISTRACTING_ELEMENTS_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-distracting-elements",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-distracting-elements",
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
  NoPositiveTabindex,
  NoAriaHiddenOnFocusable,
  ValidAriaRole,
  NoRedundantRole,
  NoDeprecatedSlotScope,
  NoDistractingElements,
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
    TemplateCheck::NoPositiveTabindex => positive_tabindex(element),
    TemplateCheck::NoAriaHiddenOnFocusable => aria_hidden_on_focusable(element),
    TemplateCheck::ValidAriaRole => invalid_aria_role(element),
    TemplateCheck::NoRedundantRole => redundant_role(element),
    TemplateCheck::NoDeprecatedSlotScope => deprecated_slot_scope(element),
    TemplateCheck::NoDistractingElements => distracting_element(element),
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

fn positive_tabindex(element: &TemplateElementFact) -> Option<Finding> {
  let attribute = element.attribute("tabindex")?;
  attribute
    .value
    .as_deref()
    .and_then(|value| value.trim().parse::<i32>().ok())
    .is_some_and(|value| value > 0)
    .then_some(Finding {
      span: attribute.span.clone(),
      message: "positive tabindex creates a surprising keyboard navigation order",
      help: "Use tabindex=\"0\" to join the natural order or tabindex=\"-1\" for programmatic focus.",
    })
}

fn aria_hidden_on_focusable(element: &TemplateElementFact) -> Option<Finding> {
  let span = element
    .attribute("aria-hidden")
    .filter(|attribute| {
      attribute.value.as_deref().is_some_and(|value| value.eq_ignore_ascii_case("true"))
    })
    .map(|attribute| &attribute.span)
    .or_else(|| {
      element
        .bound_attribute("aria-hidden")
        .filter(|directive| directive.expression.as_deref().is_some_and(|value| value == "true"))
        .map(|directive| &directive.span)
    })?;
  element_is_focusable(element).then_some(Finding {
    span: span.clone(),
    message: "focusable element is hidden from assistive technology",
    help: "Remove aria-hidden, or remove the element from keyboard interaction as well.",
  })
}

fn element_is_focusable(element: &TemplateElementFact) -> bool {
  if element.attribute("disabled").is_some() {
    return false;
  }
  let native = match element.tag.to_ascii_lowercase().as_str() {
    "a" => element.attribute("href").is_some() || element.bound_attribute("href").is_some(),
    "button" | "select" | "textarea" => true,
    "input" => element
      .attribute("type")
      .and_then(|attribute| attribute.value.as_deref())
      .is_none_or(|kind| !kind.eq_ignore_ascii_case("hidden")),
    _ => false,
  };
  native
    || element
      .attribute("tabindex")
      .and_then(|attribute| attribute.value.as_deref())
      .and_then(|value| value.trim().parse::<i32>().ok())
      .is_some_and(|value| value >= 0)
}

fn invalid_aria_role(element: &TemplateElementFact) -> Option<Finding> {
  const VALID_ROLES: &[&str] = &[
    "alert",
    "alertdialog",
    "application",
    "article",
    "banner",
    "blockquote",
    "button",
    "caption",
    "cell",
    "checkbox",
    "code",
    "columnheader",
    "combobox",
    "complementary",
    "contentinfo",
    "definition",
    "deletion",
    "dialog",
    "directory",
    "document",
    "emphasis",
    "feed",
    "figure",
    "form",
    "generic",
    "grid",
    "gridcell",
    "group",
    "heading",
    "img",
    "insertion",
    "link",
    "list",
    "listbox",
    "listitem",
    "log",
    "main",
    "marquee",
    "math",
    "menu",
    "menubar",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "meter",
    "navigation",
    "none",
    "note",
    "option",
    "paragraph",
    "presentation",
    "progressbar",
    "radio",
    "radiogroup",
    "region",
    "row",
    "rowgroup",
    "rowheader",
    "scrollbar",
    "search",
    "searchbox",
    "separator",
    "slider",
    "spinbutton",
    "status",
    "strong",
    "subscript",
    "superscript",
    "switch",
    "tab",
    "table",
    "tablist",
    "tabpanel",
    "term",
    "textbox",
    "time",
    "timer",
    "toolbar",
    "tooltip",
    "tree",
    "treegrid",
    "treeitem",
  ];
  let attribute = element.attribute("role")?;
  let value = attribute.value.as_deref()?;
  (!value
    .split_ascii_whitespace()
    .any(|role| VALID_ROLES.iter().any(|valid| role.eq_ignore_ascii_case(valid))))
  .then_some(Finding {
    span: attribute.span.clone(),
    message: "role does not contain a recognized concrete ARIA role",
    help: "Use a valid non-abstract ARIA role, or rely on the element's native semantics.",
  })
}

fn redundant_role(element: &TemplateElementFact) -> Option<Finding> {
  let attribute = element.attribute("role")?;
  let role = attribute.value.as_deref()?;
  let redundant = match element.tag.to_ascii_lowercase().as_str() {
    "a" => role.eq_ignore_ascii_case("link") && element.attribute("href").is_some(),
    "button" => role.eq_ignore_ascii_case("button"),
    "img" => role.eq_ignore_ascii_case("img"),
    "li" => role.eq_ignore_ascii_case("listitem"),
    "main" => role.eq_ignore_ascii_case("main"),
    "nav" => role.eq_ignore_ascii_case("navigation"),
    "ol" | "ul" => role.eq_ignore_ascii_case("list"),
    "table" => role.eq_ignore_ascii_case("table"),
    "textarea" => role.eq_ignore_ascii_case("textbox"),
    _ => false,
  };
  redundant.then_some(Finding {
    span: attribute.span.clone(),
    message: "explicit role duplicates the element's native semantics",
    help: "Remove the role and keep the native element semantics.",
  })
}

fn deprecated_slot_scope(element: &TemplateElementFact) -> Option<Finding> {
  element
    .attribute("slot-scope")
    .or_else(|| {
      if element.tag.eq_ignore_ascii_case("template") { element.attribute("scope") } else { None }
    })
    .map(|attribute| Finding {
      span: attribute.span.clone(),
      message: "slot-scope syntax was removed in Vue 3",
      help: "Use v-slot or the # shorthand on <template> or the receiving component.",
    })
}

fn distracting_element(element: &TemplateElementFact) -> Option<Finding> {
  (element.tag.eq_ignore_ascii_case("blink") || element.tag.eq_ignore_ascii_case("marquee"))
    .then_some(Finding {
      span: element.span.clone(),
      message: "distracting animated element is obsolete and inaccessible",
      help: "Use normal content and respect the user's reduced-motion preference for animation.",
    })
}

struct NoMutatingProps;
struct NoConditionalWatchEffectDependency;
struct NoNonreactivePropsDestructure;
struct PreferUseTemplateRef;

struct SingleCompilerMacroRule {
  meta: &'static RuleMeta,
  macro_name: &'static str,
}

impl Rule for SingleCompilerMacroRule {
  fn meta(&self) -> &'static RuleMeta {
    self.meta
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let macro_name = self.macro_name;
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == vue_vet_core::ScriptKind::Setup)
      .flat_map(|block| block.calls.iter().filter(|call| call.callee == macro_name).skip(1))
      .map(|call| call.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        format!("`{macro_name}` may only be called once in `<script setup>`"),
        Some(format!("Merge the declarations into a single `{macro_name}` call.")),
      );
    }
  }
}

impl Rule for NoConditionalWatchEffectDependency {
  fn meta(&self) -> &'static RuleMeta {
    &NO_CONDITIONAL_WATCH_EFFECT_DEPENDENCY_META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let reads = context
      .script()
      .blocks
      .iter()
      .flat_map(|block| &block.reactivity_graph.effects)
      .flat_map(|effect| {
        effect
          .reads
          .iter()
          .filter(|read| read.kind == ReactiveReadKind::Conditional)
          .filter(|read| {
            !effect.reads.iter().any(|candidate| {
              candidate.kind == ReactiveReadKind::Unconditional
                && candidate.span.offset < read.span.offset
                && candidate.binding == read.binding
                && candidate.property == read.property
            })
          })
          .map(|read| {
            let binding = read.property.as_ref().map_or_else(
              || read.binding.clone(),
              |property| format!("{}.{property}", read.binding),
            );
            let guards = read
              .guards
              .iter()
              .map(|guard| {
                guard.property.as_ref().map_or_else(
                  || guard.binding.clone(),
                  |property| format!("{}.{property}", guard.binding),
                )
              })
              .collect::<Vec<_>>()
              .join("`, `");
            (read.span.clone(), binding, guards)
          })
          .collect::<Vec<_>>()
      })
      .collect::<Vec<_>>();
    for (span, binding, guards) in reads {
      context.report(
        self.meta(),
        span,
        format!("`{binding}` is only tracked after the `{guards}` guard passes"),
        Some(
          "If every value must invalidate the effect, use explicit watch sources or read each             dependency before the guard."
            .into(),
        ),
      );
    }
  }
}

impl Rule for NoNonreactivePropsDestructure {
  fn meta(&self) -> &'static RuleMeta {
    &NO_NONREACTIVE_PROPS_DESTRUCTURE_META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if version.is_at_least(3, 5) {
      return;
    }
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == vue_vet_core::ScriptKind::Setup)
      .flat_map(|block| &block.destructures)
      .filter(|destructure| destructure.source_call == "defineProps")
      .map(|destructure| destructure.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "destructured props are not reactive before Vue 3.5".into(),
        Some(
          "Assign defineProps() to an object, then destructure toRefs(props), or keep property            access through the props object."
            .into(),
        ),
      );
    }
  }
}

impl Rule for PreferUseTemplateRef {
  fn meta(&self) -> &'static RuleMeta {
    &PREFER_USE_TEMPLATE_REF_META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if !version.is_at_least(3, 5) {
      return;
    }
    let template_refs = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.attribute("ref"))
      .filter_map(|attribute| attribute.value.as_deref())
      .collect::<Vec<_>>();
    let bindings = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == vue_vet_core::ScriptKind::Setup)
      .flat_map(|block| &block.reactivity_graph.bindings)
      .filter(|binding| {
        binding.kind == ReactiveBindingKind::Ref
          && binding.initialized_with_null
          && template_refs.iter().any(|template_ref| **template_ref == binding.name)
      })
      .map(|binding| (binding.span.clone(), binding.name.clone()))
      .collect::<Vec<_>>();
    for (span, name) in bindings {
      context.report(
        self.meta(),
        span,
        format!("`{name}` mirrors a static template ref with `ref(null)`"),
        Some(format!("Use `useTemplateRef('{name}')`, available in Vue 3.5 and newer.")),
      );
    }
  }
}

impl Rule for NoMutatingProps {
  fn meta(&self) -> &'static RuleMeta {
    &NO_MUTATING_PROPS_META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let prop_bindings = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == vue_vet_core::ScriptKind::Setup)
      .flat_map(|block| {
        block
          .calls
          .iter()
          .filter(|call| call.callee == "defineProps")
          .filter_map(|call| call.assigned_to.clone())
      })
      .collect::<Vec<_>>();
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == vue_vet_core::ScriptKind::Setup)
      .flat_map(|block| &block.member_writes)
      .filter(|write| prop_bindings.contains(&write.object))
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
template_rule!(NO_POSITIVE_TABINDEX, NO_POSITIVE_TABINDEX_META, NoPositiveTabindex);
template_rule!(
  NO_ARIA_HIDDEN_ON_FOCUSABLE,
  NO_ARIA_HIDDEN_ON_FOCUSABLE_META,
  NoAriaHiddenOnFocusable
);
template_rule!(VALID_ARIA_ROLE, VALID_ARIA_ROLE_META, ValidAriaRole);
template_rule!(NO_REDUNDANT_ROLE, NO_REDUNDANT_ROLE_META, NoRedundantRole);
template_rule!(NO_DEPRECATED_SLOT_SCOPE, NO_DEPRECATED_SLOT_SCOPE_META, NoDeprecatedSlotScope);
template_rule!(NO_DISTRACTING_ELEMENTS, NO_DISTRACTING_ELEMENTS_META, NoDistractingElements);
static NO_MUTATING_PROPS: NoMutatingProps = NoMutatingProps;
static NO_CONDITIONAL_WATCH_EFFECT_DEPENDENCY: NoConditionalWatchEffectDependency =
  NoConditionalWatchEffectDependency;
static NO_NONREACTIVE_PROPS_DESTRUCTURE: NoNonreactivePropsDestructure =
  NoNonreactivePropsDestructure;
static PREFER_USE_TEMPLATE_REF: PreferUseTemplateRef = PreferUseTemplateRef;
static NO_DUPLICATE_DEFINE_PROPS: SingleCompilerMacroRule =
  SingleCompilerMacroRule { meta: &NO_DUPLICATE_DEFINE_PROPS_META, macro_name: "defineProps" };
static NO_DUPLICATE_DEFINE_EMITS: SingleCompilerMacroRule =
  SingleCompilerMacroRule { meta: &NO_DUPLICATE_DEFINE_EMITS_META, macro_name: "defineEmits" };
static NO_DUPLICATE_DEFINE_SLOTS: SingleCompilerMacroRule =
  SingleCompilerMacroRule { meta: &NO_DUPLICATE_DEFINE_SLOTS_META, macro_name: "defineSlots" };
static NO_DUPLICATE_DEFINE_EXPOSE: SingleCompilerMacroRule =
  SingleCompilerMacroRule { meta: &NO_DUPLICATE_DEFINE_EXPOSE_META, macro_name: "defineExpose" };
static NO_DUPLICATE_DEFINE_OPTIONS: SingleCompilerMacroRule =
  SingleCompilerMacroRule { meta: &NO_DUPLICATE_DEFINE_OPTIONS_META, macro_name: "defineOptions" };

#[must_use]
pub fn builtin_registry() -> RuleRegistry {
  RuleRegistry::new(vec![
    &NO_V_HTML,
    &REQUIRE_V_FOR_KEY,
    &NO_V_IF_WITH_V_FOR,
    &NO_DUPLICATE_DEFINE_PROPS,
    &NO_DUPLICATE_DEFINE_EMITS,
    &NO_DUPLICATE_DEFINE_SLOTS,
    &NO_DUPLICATE_DEFINE_EXPOSE,
    &NO_DUPLICATE_DEFINE_OPTIONS,
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
    &NO_POSITIVE_TABINDEX,
    &NO_ARIA_HIDDEN_ON_FOCUSABLE,
    &VALID_ARIA_ROLE,
    &NO_REDUNDANT_ROLE,
    &NO_DEPRECATED_SLOT_SCOPE,
    &NO_DISTRACTING_ELEMENTS,
    &NO_MUTATING_PROPS,
    &NO_CONDITIONAL_WATCH_EFFECT_DEPENDENCY,
    &NO_NONREACTIVE_PROPS_DESTRUCTURE,
    &PREFER_USE_TEMPLATE_REF,
  ])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn builtins_have_stable_metadata() {
    let metadata = builtin_registry().metadata();
    assert_eq!(metadata.len(), 28, "the recommended preset contains twenty-eight rules");
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
