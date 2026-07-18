use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

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

  fn run(&self, context: &mut RuleContext<'_>) {
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
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.attribute("role"))
      .filter(|attribute| {
        attribute.value.as_deref().is_some_and(|value| {
          !value
            .split_ascii_whitespace()
            .any(|role| VALID_ROLES.iter().any(|valid| role.eq_ignore_ascii_case(valid)))
        })
      })
      .map(|attribute| attribute.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "role does not contain a recognized concrete ARIA role".into(),
        Some(
          "Use a valid non-abstract ARIA role, or rely on the element's native semantics.".into(),
        ),
      );
    }
  }
}
