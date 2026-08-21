use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateDirectiveFact {
  pub name: String,
  pub raw_name: String,
  pub argument: Option<String>,
  pub expression: Option<String>,
  pub modifiers: Vec<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateAttributeFact {
  pub name: String,
  pub value: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(
  clippy::struct_excessive_bools,
  reason = "independent template fact flags collected while the Vize tree is available"
)]
pub struct TemplateElementFact {
  pub tag: String,
  pub span: SourceSpan,
  pub attributes: Vec<TemplateAttributeFact>,
  pub directives: Vec<TemplateDirectiveFact>,
  /// True when the element node lists any child AST nodes (including whitespace/comments).
  pub has_children: bool,
  /// True when the subtree exposes screen-reader content: non-whitespace text,
  /// interpolation, `v-text`/`v-html`, or `img`/`area` with a non-empty `alt`.
  /// Element-only trees (icon `<div>`s) are false even when `has_children` is true.
  /// Vue component children also contribute (propagated from the template walk).
  #[serde(default)]
  pub has_accessible_content: bool,
  /// True when a labelable control (`input` / `textarea` / `select` / …) appears
  /// in the descendant tree. Used by `label-has-for` because element spans cover
  /// only the start tag, not nested children.
  #[serde(default)]
  pub has_labelable_descendant: bool,
  /// True when this element is nested under a `<label>` ancestor. Used by
  /// `form-control-has-label` (start-tag spans cannot prove nesting by containment).
  #[serde(default)]
  pub has_label_ancestor: bool,
  /// True when nested under a Vue component that supplies a name-like prop
  /// (`content` / `title` / `label` / `text` / `aria-label`), e.g. tooltip
  /// wrappers around icon-only buttons.
  #[serde(default)]
  pub has_accessible_name_ancestor: bool,
}

impl TemplateElementFact {
  #[must_use]
  pub fn attribute(&self, name: &str) -> Option<&TemplateAttributeFact> {
    self.attributes.iter().find(|attribute| attribute.name.eq_ignore_ascii_case(name))
  }

  #[must_use]
  pub fn directive(&self, name: &str) -> Option<&TemplateDirectiveFact> {
    self.directives.iter().find(|directive| directive.name == name)
  }

  #[must_use]
  pub fn bound_attribute(&self, name: &str) -> Option<&TemplateDirectiveFact> {
    self.directives.iter().find(|directive| {
      directive.name == "bind"
        && directive.argument.as_deref().is_some_and(|argument| argument.eq_ignore_ascii_case(name))
    })
  }

  #[must_use]
  pub fn event(&self, name: &str) -> Option<&TemplateDirectiveFact> {
    self.directives.iter().find(|directive| {
      directive.name == "on"
        && directive.argument.as_deref().is_some_and(|argument| argument.eq_ignore_ascii_case(name))
    })
  }

  #[must_use]
  pub fn has_key(&self) -> bool {
    self.attribute("key").is_some() || self.bound_attribute("key").is_some()
  }
}

/// One template expression surface that may read script bindings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateExpressionFact {
  /// Where the expression appears (`if`, `for`, `bind`, `on`, `interpolation`, …).
  pub surface: String,
  /// Raw expression text.
  pub expression: String,
  /// Exact SFC-absolute span of the expression when known.
  pub span: SourceSpan,
  /// Free identifier reads when resolved (`Some`, possibly empty). `None` means
  /// unknown and join may fall back to a lexical scan (hand-built fixtures).
  #[serde(default)]
  pub identifiers: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateFacts {
  pub elements: Vec<TemplateElementFact>,
  /// Flattened expression surfaces (directives + interpolations) with spans.
  #[serde(default)]
  pub expressions: Vec<TemplateExpressionFact>,
}
