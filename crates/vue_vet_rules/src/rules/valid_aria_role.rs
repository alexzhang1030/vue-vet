use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/valid-aria-role",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/valid-aria-role",
};

pub(super) struct ValidAriaRole;

pub(super) static RULE: ValidAriaRole = ValidAriaRole;

impl Rule for ValidAriaRole {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
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
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    let Some(attribute) = element.attribute("role") else {
      return;
    };
    let invalid = attribute.value.as_deref().is_some_and(|value| {
      !value
        .split_ascii_whitespace()
        .any(|role| VALID_ROLES.iter().any(|valid| role.eq_ignore_ascii_case(valid)))
    });
    if !invalid {
      return;
    }
    context.report(
      self.meta(),
      attribute.span.clone(),
      "role does not contain a recognized concrete ARIA role".into(),
      Some("Use a valid non-abstract ARIA role, or rely on the element's native semantics.".into()),
    );
  }
}
