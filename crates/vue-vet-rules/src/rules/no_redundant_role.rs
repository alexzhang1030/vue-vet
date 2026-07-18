use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

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

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| {
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
        redundant.then_some(attribute.span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "explicit role duplicates the element's native semantics".into(),
        Some("Remove the role and keep the native element semantics.".into()),
      );
    }
  }
}
