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
  /// Proven object-form `v-bind="{ key: … }"` (Oxc object literal, no opaque spread).
  #[serde(default, skip_serializing_if = "is_false_flag")]
  pub object_bind_has_key: bool,
  /// Vize `ElementType::Component` (or JSX identifier-reference / member tag).
  /// Native HTML/SVG/MathML elements stay false so Transition static-child
  /// positives are preserved; lowercase imported components stay true.
  #[serde(default, skip_serializing_if = "is_false_flag")]
  pub is_component: bool,
  /// Nested under `v-if` / `v-else-if` / `v-else` (own directive or ancestor).
  /// Start-tag spans cannot prove nesting by containment.
  #[serde(default, skip_serializing_if = "is_false_flag")]
  pub has_conditional_ancestor: bool,
  /// Nested under `v-for` (own directive or ancestor).
  #[serde(default, skip_serializing_if = "is_false_flag")]
  pub has_for_ancestor: bool,
  /// Nested under `Suspense` / `Transition` / `KeepAlive` / `Teleport`.
  #[serde(default, skip_serializing_if = "is_false_flag")]
  pub has_async_boundary_ancestor: bool,
  /// Nested under `v-slot` / `#default` (own directive or ancestor).
  #[serde(default, skip_serializing_if = "is_false_flag")]
  pub has_slot_ancestor: bool,
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde skip_serializing_if takes &T")]
const fn is_false_flag(value: &bool) -> bool {
  !*value
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
    self.attribute("key").is_some()
      || self.bound_attribute("key").is_some()
      || self.object_bind_has_key
  }

  /// Unconditional static mount: no branch, list, slot, or async boundary.
  ///
  /// Does not require [`Self::is_component`]. Project joins that already have a
  /// `ComponentUsage` / `AutoComponent` edge should use this so kebab-case tags
  /// Vize classifies as native elements still join.
  #[must_use]
  pub fn is_unconditional_mount(&self) -> bool {
    !self.has_conditional_ancestor
      && !self.has_for_ancestor
      && !self.has_async_boundary_ancestor
      && !self.has_slot_ancestor
      && !self.has_mount_directive()
      && !self.is_dynamic_component()
  }

  /// Unconditional static component instance: no branch, list, slot, or async boundary.
  #[must_use]
  pub fn is_static_unconditional_instance(&self) -> bool {
    self.is_component && self.is_unconditional_mount()
  }

  fn has_mount_directive(&self) -> bool {
    self.directives.iter().any(|directive| {
      matches!(directive.name.as_str(), "if" | "else-if" | "else" | "for" | "slot" | "is")
        || (directive.name == "bind" && directive.argument.as_deref() == Some("is"))
    })
  }

  fn is_dynamic_component(&self) -> bool {
    self.tag.eq_ignore_ascii_case("component") || self.tag.eq_ignore_ascii_case("async-component")
  }

  /// Template `ref="name"` (static attribute only).
  #[must_use]
  pub fn static_ref_name(&self) -> Option<&str> {
    self.attribute("ref").and_then(|attribute| attribute.value.as_deref())
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
  /// Parent / memo / condition / ref relations recorded during the Vize walk.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub allocations: Vec<super::TemplateAllocationFact>,
}
