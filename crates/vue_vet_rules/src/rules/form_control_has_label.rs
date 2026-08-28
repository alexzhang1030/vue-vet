use std::collections::BTreeSet;

use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity, SourceSpan};

use super::a11y_content::{
  AssocToken, association_token, has_accessible_name_attrs, is_form_control,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/form-control-has-label",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/form-control-has-label",
};

pub(super) struct FormControlHasLabel;

pub(super) static RULE: FormControlHasLabel = FormControlHasLabel;

impl Rule for FormControlHasLabel {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    let elements = context.template().elements.as_slice();
    let mut label_targets = BTreeSet::new();
    for element in elements {
      if !element.tag.eq_ignore_ascii_case("label") {
        continue;
      }
      if let Some(token) = association_token(element, "for") {
        label_targets.insert(token_key(&token));
      }
    }

    let mut findings = Vec::<SourceSpan>::new();
    for element in elements {
      if !is_form_control(element)
        || element.has_label_ancestor
        || has_accessible_name_attrs(element)
      {
        continue;
      }
      if let Some(token) = association_token(element, "id")
        && label_targets.contains(&token_key(&token))
      {
        continue;
      }
      findings.push(element.span);
    }

    for span in findings {
      context.report(
        self.meta(),
        span,
        "form control has no associated label".into(),
        Some(
          "Nest the control in a label, set a matching `for`/`id` pair, or add aria-label/aria-labelledby."
            .into(),
        ),
      );
    }
  }
}

fn token_key(token: &AssocToken<'_>) -> String {
  match token {
    AssocToken::Static(value) => format!("static:{value}"),
    AssocToken::Expr(value) => format!("expr:{value}"),
  }
}
