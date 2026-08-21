use crate::template::extract_template_facts;
use vue_vet_core::TemplateFacts;

#[expect(clippy::panic, reason = "fixture setup failures must fail the unit test")]
fn facts(template: &str) -> TemplateFacts {
  let source = format!("<template>{template}</template>");
  match extract_template_facts(&source, template, 10) {
    Ok(facts) => facts,
    Err(error) => panic!("template parse failed: {error}"),
  }
}

#[test]
#[expect(clippy::panic, reason = "fixture setup failures must fail the unit test")]
fn component_child_marks_parent_link_accessible() {
  let template = r#"
<NuxtLink to="/profile">
  <AccountInfo :account="account" />
</NuxtLink>
"#;
  let facts = facts(template);
  let Some(link) = facts.elements.iter().find(|element| element.tag == "NuxtLink") else {
    panic!("missing NuxtLink element");
  };
  assert!(
    link.has_accessible_content,
    "NuxtLink with AccountInfo child must have accessible content; elements={:?}",
    facts
      .elements
      .iter()
      .map(|element| (&element.tag, element.has_accessible_content, element.has_children))
      .collect::<Vec<_>>()
  );
}

#[test]
#[expect(clippy::panic, reason = "fixture setup failures must fail the unit test")]
fn tooltip_content_marks_nested_button_named() {
  let template = r#"
<CommonTooltip :content="label">
  <button type="button">
    <span class="i-ri:close-line" />
  </button>
</CommonTooltip>
"#;
  let facts = facts(template);
  let Some(button) = facts.elements.iter().find(|element| element.tag == "button") else {
    panic!("missing button element");
  };
  assert!(
    button.has_accessible_name_ancestor,
    "button under CommonTooltip :content must be named; elements={:?}",
    facts
      .elements
      .iter()
      .map(|element| {
        (&element.tag, element.has_accessible_name_ancestor, element.has_accessible_content)
      })
      .collect::<Vec<_>>()
  );
}
