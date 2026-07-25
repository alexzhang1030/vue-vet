use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/maintainability/no-redundant-role",
  category: "maintainability",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/maintainability/no-redundant-role",
};

pub(super) struct NoRedundantRole;

pub(super) static RULE: NoRedundantRole = NoRedundantRole;

impl Rule for NoRedundantRole {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    let Some(attribute) = element.attribute("role") else {
      return;
    };
    let Some(role) = attribute.value.as_deref() else {
      return;
    };
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
    if !redundant {
      return;
    }
    context.report(
      self.meta(),
      attribute.span.clone(),
      "explicit role duplicates the element's native semantics".into(),
      Some("Remove the role and keep the native element semantics.".into()),
    );
  }
}
