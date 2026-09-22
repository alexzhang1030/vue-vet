use super::support::*;

#[test]
fn template_expression_identifiers_use_oxc_ast_not_property_names() {
  assert_eq!(
    template_expression_identifiers("user.name + count", "interpolation"),
    vec!["count".to_owned(), "user".to_owned()],
    "static member properties must not be collected as free reads"
  );
  assert_eq!(
    template_expression_identifiers("item in items", "for"),
    vec!["items".to_owned()],
    "v-for must join only the iterable source, not the alias"
  );
  assert_eq!(
    template_expression_identifiers("(item, index) of list", "for"),
    vec!["list".to_owned()],
    "destructured v-for aliases must not appear as free reads"
  );
  assert_eq!(
    template_expression_identifiers("(item) => item + count", "on"),
    vec!["count".to_owned()],
    "handler parameters must not be treated as free template reads"
  );
  assert_eq!(
    template_expression_identifiers("(item) => { const local = item; return local + total }", "on"),
    vec!["total".to_owned()],
    "inner let/const bindings must be filtered from free reads"
  );
  assert_eq!(
    template_expression_identifiers("target = 0", "on"),
    vec!["target".to_owned()],
    "event assignment targets must remain identifier facts"
  );
  assert_eq!(
    template_expression_identifiers("'target'", "on"),
    Vec::<String>::new(),
    "a string containing a binding name is not an identifier fact"
  );
  assert_eq!(
    v_for_alias_identifiers("item in items"),
    vec!["item".to_owned()],
    "simple v-for aliases must be recovered"
  );
  assert_eq!(
    v_for_alias_identifiers("(item, index) of list"),
    vec!["index".to_owned(), "item".to_owned()],
    "paired v-for aliases must be recovered"
  );
  assert_eq!(
    v_for_alias_identifiers("({ id, label }, index) in rows"),
    vec!["id".to_owned(), "index".to_owned(), "label".to_owned()],
    "destructured v-for aliases must be recovered"
  );
  assert_eq!(
    slot_prop_alias_identifiers("{ value, meta }"),
    vec!["meta".to_owned(), "value".to_owned()],
    "slot prop destructuring must bind locals"
  );
  let shadowed = BTreeSet::from(["item".to_owned()]);
  assert_eq!(
    template_expression_identifiers_with_shadow("item + count", "interpolation", &shadowed),
    vec!["count".to_owned()],
    "template-local aliases must not appear as free reads"
  );
  assert!(
    template_expression_identifiers("{ value }", "slot").is_empty(),
    "slot prop patterns are bindings, not free reads"
  );
  assert!(
    template_expression_identifiers("??? not expression", "if").is_empty(),
    "parse failures stay quiet so callers can fall back"
  );
}

#[test]
fn template_demand_gated_matches_forced_full() {
  let (source, allocations) = shared_source_fanout(4);
  let (gated, gated_stats) = demand_stats(&source, allocations.clone(), false);
  let (full, full_stats) = demand_stats(&source, allocations, true);
  assert_eq!(gated, full, "gated facts must equal forced-full");
  assert_eq!(gated.pre_flush.len(), 4, "{gated:?}");
  assert!(gated_stats.work() > 0 && full_stats.work() > 0);
}

#[test]
fn template_demand_shared_source_fanout_grows_sub_quadratic() {
  let mut previous: Option<(u32, u64)> = None;
  for width in [8_u32, 16, 32] {
    let (source, allocations) = shared_source_fanout(width);
    let (facts, stats) = demand_stats(&source, allocations, false);
    assert_eq!(facts.pre_flush.len(), width as usize, "width {width}; {facts:?}");
    if let Some((prev_width, prev_work)) = previous {
      assert_eq!(width, prev_width * 2);
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-source fanout work grew from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
    }
    previous = Some((width, stats.work()));
  }
}

#[test]
fn template_demand_many_conditions_and_consumers_grow_together() {
  let mut previous: Option<(u32, u64)> = None;
  for width in [8_u32, 16, 32] {
    let mut source = String::from("import { onMounted, ref, watch } from 'vue'\n");
    let mut allocations = Vec::new();
    for index in 0..width {
      source.push_str("const visible");
      source.push_str(&index.to_string());
      source.push_str(" = ref(false)\nconst node");
      source.push_str(&index.to_string());
      source.push_str(" = ref(null)\nwatch(visible");
      source.push_str(&index.to_string());
      source.push_str(", () => { node");
      source.push_str(&index.to_string());
      source.push_str(".value.textContent }, { flush: 'pre' })\n");
      allocations.push(native_alloc(
        &format!("node{index}"),
        &format!("visible{index}"),
        300 + index as usize * 10,
      ));
    }
    source.push_str("onMounted(() => {\n");
    for index in 0..width {
      source.push_str("  visible");
      source.push_str(&index.to_string());
      source.push_str(".value = true\n");
    }
    source.push_str("})\n");
    let (facts, stats) = demand_stats(&source, allocations, false);
    assert_eq!(facts.pre_flush.len(), width as usize, "width {width}; {facts:?}");
    if let Some((prev_width, prev_work)) = previous {
      assert_eq!(width, prev_width * 2);
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
        "multi-condition work grew from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
    }
    previous = Some((width, stats.work()));
  }
}

#[test]
fn template_demand_many_watches_and_demands_grow_sub_quadratic() {
  let mut previous: Option<(u32, u64)> = None;
  for width in [20_u32, 40, 80] {
    let (source, allocations) = watches_times_demands(width);
    let (facts, stats) = demand_stats(&source, allocations, false);
    assert_eq!(facts.pre_flush.len(), width as usize, "width {width}; {facts:?}");
    if let Some((prev_width, prev_work)) = previous {
      assert_eq!(width, prev_width * 2);
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
        "watch×demand work grew from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
    }
    previous = Some((width, stats.work()));
  }
}

#[test]
fn template_demand_shadowed_parameter_is_deterministic() {
  let source = concat!(
    "import { onMounted, ref, watch } from 'vue'\n",
    "const visible = ref(false)\n",
    "const node = ref(null)\n",
    "const trim = (node: string) => node.trim()\n",
    "const isOn = (visible: boolean) => visible\n",
    "watch(visible, () => {\n",
    "  node.value.textContent\n",
    "}, { flush: 'pre' })\n",
    "onMounted(() => { visible.value = true })\n",
  );
  let allocations = vec![native_alloc("node", "visible", 200)];
  let (first, _) = demand_stats(source, allocations.clone(), false);
  assert_eq!(
    first.pre_flush.len(),
    1,
    "a shadowing parameter must not steal the script-setup template ref; {first:?}"
  );
  let encoded = format!("{first:?}");
  for _ in 0..16 {
    let (again, _) = demand_stats(source, allocations.clone(), false);
    assert_eq!(format!("{again:?}"), encoded, "repeated collector output must be byte-identical");
    assert_eq!(again.pre_flush.len(), 1);
    assert_eq!(again.memo_blocked.len(), 0);
  }
}

#[test]
fn template_demand_sibling_early_return_is_a_guard() {
  let source = concat!(
    "import { onMounted, ref, watch } from 'vue'\n",
    "const visible = ref(false)\n",
    "const node = ref(null)\n",
    "watch(visible, () => {\n",
    "  if (!node.value) return\n",
    "  node.value.textContent\n",
    "}, { flush: 'pre' })\n",
    "onMounted(() => { visible.value = true })\n",
  );
  let (facts, _) = demand_stats(source, vec![native_alloc("node", "visible", 200)], false);
  assert!(facts.is_empty(), "sibling early-return must guard the demand; {facts:?}");
}
