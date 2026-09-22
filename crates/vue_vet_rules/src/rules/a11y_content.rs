//! Shared helpers and the three accessible-name content rules.

use vue_vet_core::{
  Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity, TemplateElementFact,
};

#[must_use]
pub(super) const fn is_anchor_like(tag: &str) -> bool {
  tag.eq_ignore_ascii_case("a")
    || tag.eq_ignore_ascii_case("RouterLink")
    || tag.eq_ignore_ascii_case("router-link")
    || tag.eq_ignore_ascii_case("NuxtLink")
    || tag.eq_ignore_ascii_case("nuxt-link")
}

#[must_use]
pub(super) fn is_heading(tag: &str) -> bool {
  matches!(tag.to_ascii_lowercase().as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

#[must_use]
pub(super) fn has_accessible_name_attrs(element: &TemplateElementFact) -> bool {
  element.attribute("aria-label").is_some()
    || element.bound_attribute("aria-label").is_some()
    || element.attribute("aria-labelledby").is_some()
    || element.bound_attribute("aria-labelledby").is_some()
    || has_nonempty_title_name(element)
    // Tooltip / menu wrappers that publish a name prop for their default slot.
    || element.has_accessible_name_ancestor
}

/// HTML-AAM: nonempty `title` / `:title` is an accessible-name fallback.
fn has_nonempty_title_name(element: &TemplateElementFact) -> bool {
  if element.bound_attribute("title").is_some_and(|directive| {
    directive.expression.as_deref().is_some_and(|expression| !expression.trim().is_empty())
  }) {
    return true;
  }
  element
    .attribute("title")
    .and_then(|attribute| attribute.value.as_deref())
    .is_some_and(|value| !value.trim().is_empty())
}

#[must_use]
pub(super) fn is_form_control(element: &TemplateElementFact) -> bool {
  match element.tag.to_ascii_lowercase().as_str() {
    "textarea" | "select" | "meter" | "output" | "progress" => true,
    "input" => !input_type_skips_label(element),
    _ => false,
  }
}

fn input_type_skips_label(element: &TemplateElementFact) -> bool {
  let Some(type_name) = element
    .attribute("type")
    .and_then(|attribute| attribute.value.as_deref())
    .map(str::trim)
    .filter(|value| !value.is_empty())
  else {
    // Missing type defaults to text — needs a label.
    return false;
  };
  matches!(
    type_name.to_ascii_lowercase().as_str(),
    "hidden" | "button" | "submit" | "reset" | "image"
  )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AssocToken<'a> {
  Static(&'a str),
  Expr(&'a str),
}

#[must_use]
pub(super) fn association_token<'a>(
  element: &'a TemplateElementFact,
  name: &str,
) -> Option<AssocToken<'a>> {
  if let Some(attribute) = element.attribute(name) {
    let value = attribute.value.as_deref()?.trim();
    if value.is_empty() {
      return None;
    }
    return Some(AssocToken::Static(value));
  }
  let directive = element.bound_attribute(name)?;
  let expression = directive.expression.as_deref()?.trim();
  if expression.is_empty() {
    return None;
  }
  Some(AssocToken::Expr(expression))
}

const CONTENT_HELP: &str =
  "Add text content, an img/area with alt, or an aria-label/aria-labelledby binding.";

pub(super) struct ContentRule {
  meta: &'static RuleMeta,
  matches: fn(&str) -> bool,
  message: &'static str,
}

const fn is_button(tag: &str) -> bool {
  tag.eq_ignore_ascii_case("button")
}

const ANCHOR_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/anchor-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/anchor-has-content",
  group: None,
};

const BUTTON_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/button-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/button-has-content",
  group: None,
};

const HEADING_META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/heading-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/heading-has-content",
  group: None,
};

pub(super) static ANCHOR: ContentRule = ContentRule {
  meta: &ANCHOR_META,
  matches: is_anchor_like,
  message: "link has no accessible content",
};

pub(super) static BUTTON: ContentRule = ContentRule {
  meta: &BUTTON_META,
  matches: is_button,
  message: "button has no accessible content",
};

pub(super) static HEADING: ContentRule = ContentRule {
  meta: &HEADING_META,
  matches: is_heading,
  message: "heading has no accessible content",
};

impl Rule for ContentRule {
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
    if !(self.matches)(&element.tag)
      || element.has_accessible_content
      || has_accessible_name_attrs(element)
    {
      return;
    }
    context.report(self.meta(), element.span, self.message.into(), Some(CONTENT_HELP.into()));
  }
}
