use std::fmt::Write;

use super::helpers::*;

const PARENT: &str = r#"<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const value = ref()
onMounted(() => {
  value.value.toFixed(2)
})
</script>
<template>
  <Child v-model="value" />
</template>
"#;

const CHILD: &str = r#"<script setup lang="ts">
const model = defineModel({ default: 1 })
defineExpose({ model })
</script>
<template>
  <div>{{ model }}</div>
</template>
"#;

const SHARED_CHILD: &str = r#"<script lang="ts">
const shared = { n: 1 }
</script>
<script setup lang="ts">
const model = defineModel({ default: () => shared })
defineExpose({ model })
</script>
<template>
  <div>{{ model.n }}</div>
</template>
"#;

const LITERAL_CHILD: &str = r#"<script setup lang="ts">
const model = defineModel({ default: { n: 1 } })
defineExpose({ model })
</script>
<template>
  <div>{{ model.n }}</div>
</template>
"#;

const SHARED_PARENT: &str = r#"<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const left = ref(null)
const right = ref(null)
onMounted(() => {
  left.value.model.n = 'text'
  right.value.model.n.toFixed(2)
})
</script>
<template>
  <Child ref="left" />
  <Child ref="right" />
</template>
"#;

const FRESH_CHILD: &str = r#"<script setup lang="ts">
const model = defineModel({ default: () => ({ n: 1 }) })
defineExpose({ model })
</script>
<template>
  <div>{{ model.n }}</div>
</template>
"#;

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn unsynced_model_default_reports_parent_demand_span() {
  let root = std::env::temp_dir().join(format!("vue-vet-model-default-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), CHILD).unwrap_or_else(|error| panic!("child: {error}"));
  std::fs::write(root.join("Parent.vue"), PARENT).unwrap_or_else(|error| panic!("parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let needle = "value.value.toFixed(2)";
  let Some(offset) = PARENT.find(needle) else {
    panic!("demand missing");
  };
  let found = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.rule_id == "vue-vet/reactivity/no-model-default-unsynced-parent-demand"
      && diagnostic.file == FileId::from("Parent.vue")
  });
  let Some(diagnostic) = found else {
    panic!("expected unsynced parent demand; {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, offset);
  assert_eq!(diagnostic.span.length, needle.len());
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn null_defined_guarded_and_conditional_parents_stay_quiet() {
  let root = std::env::temp_dir().join(format!("vue-vet-model-safe-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), CHILD).unwrap_or_else(|error| panic!("child: {error}"));
  std::fs::write(
    root.join("Null.vue"),
    PARENT.replace("const value = ref()", "const value = ref(null)"),
  )
  .unwrap_or_else(|error| panic!("null: {error}"));
  std::fs::write(
    root.join("Defined.vue"),
    PARENT.replace("const value = ref()", "const value = ref(1)"),
  )
  .unwrap_or_else(|error| panic!("defined: {error}"));
  std::fs::write(
    root.join("Guarded.vue"),
    PARENT.replace("value.value.toFixed(2)", "value.value?.toFixed(2)"),
  )
  .unwrap_or_else(|error| panic!("guarded: {error}"));
  std::fs::write(
    root.join("Conditional.vue"),
    PARENT.replace("<Child v-model=\"value\" />", "<Child v-if=\"true\" v-model=\"value\" />"),
  )
  .unwrap_or_else(|error| panic!("conditional: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let hits: Vec<_> = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-model-default-unsynced-parent-demand"
    })
    .collect();
  assert!(hits.is_empty(), "safe parents must stay quiet: {hits:?}");
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn shared_default_reports_sibling_demand_and_fresh_factory_is_quiet() {
  let root = std::env::temp_dir().join(format!("vue-vet-shared-default-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), SHARED_CHILD)
    .unwrap_or_else(|error| panic!("child: {error}"));
  std::fs::write(root.join("Parent.vue"), SHARED_PARENT)
    .unwrap_or_else(|error| panic!("parent: {error}"));
  std::fs::write(root.join("FreshChild.vue"), FRESH_CHILD)
    .unwrap_or_else(|error| panic!("fresh child: {error}"));
  std::fs::write(
    root.join("FreshParent.vue"),
    SHARED_PARENT
      .replace("import Child from './Child.vue'", "import Child from './FreshChild.vue'"),
  )
  .unwrap_or_else(|error| panic!("fresh parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let needle = "right.value.model.n.toFixed(2)";
  let Some(offset) = SHARED_PARENT.find(needle) else {
    panic!("shared demand missing");
  };
  let found = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.rule_id == "vue-vet/reactivity/no-shared-default-cross-instance-demand"
      && diagnostic.file == FileId::from("Parent.vue")
  });
  let Some(diagnostic) = found else {
    panic!("expected shared-default demand; {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, offset);
  assert_eq!(diagnostic.span.length, needle.len());
  assert!(
    snapshot.summary.diagnostics.iter().all(|diagnostic| {
      diagnostic.file != FileId::from("FreshParent.vue")
        || diagnostic.rule_id != "vue-vet/reactivity/no-shared-default-cross-instance-demand"
    }),
    "fresh factory must stay quiet: {:?}",
    snapshot.summary.diagnostics
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn unicode_and_crlf_parent_demand_keep_byte_spans() {
  let root = std::env::temp_dir().join(format!("vue-vet-model-span-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), CHILD).unwrap_or_else(|error| panic!("child: {error}"));
  let unicode = r#"<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const 值 = ref()
onMounted(() => {
  值.value.toFixed(2)
})
</script>
<template>
  <Child v-model="值" />
</template>
"#;
  std::fs::write(root.join("Unicode.vue"), unicode)
    .unwrap_or_else(|error| panic!("unicode: {error}"));
  let crlf = PARENT.replace('\n', "\r\n");
  std::fs::write(root.join("Crlf.vue"), &crlf).unwrap_or_else(|error| panic!("crlf: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let unicode_needle = "值.value.toFixed(2)";
  let Some(unicode_offset) = unicode.find(unicode_needle) else {
    panic!("unicode demand missing");
  };
  let unicode_hit = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.file == FileId::from("Unicode.vue")
      && diagnostic.rule_id == "vue-vet/reactivity/no-model-default-unsynced-parent-demand"
  });
  let Some(diagnostic) = unicode_hit else {
    panic!("unicode finding missing: {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, unicode_offset);
  assert_eq!(diagnostic.span.length, unicode_needle.len());
  let crlf_needle = "value.value.toFixed(2)";
  let Some(crlf_offset) = crlf.find(crlf_needle) else {
    panic!("crlf demand missing");
  };
  let crlf_hit = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.file == FileId::from("Crlf.vue")
      && diagnostic.rule_id == "vue-vet/reactivity/no-model-default-unsynced-parent-demand"
  });
  let Some(diagnostic) = crlf_hit else {
    panic!("crlf finding missing: {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, crlf_offset);
  assert_eq!(diagnostic.span.length, crlf_needle.len());
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn child_or_parent_edit_matches_fresh_analysis() {
  let root = std::env::temp_dir().join(format!("vue-vet-model-incr-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), CHILD).unwrap_or_else(|error| panic!("child: {error}"));
  std::fs::write(root.join("Parent.vue"), PARENT).unwrap_or_else(|error| panic!("parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  session.analyze().unwrap_or_else(|error| panic!("initial: {error}"));
  let edited_child = CHILD.replace("default: 1", "default: 2");
  session
    .apply_changes(ChangeSet::upsert(root.join("Child.vue"), edited_child.clone()))
    .unwrap_or_else(|error| panic!("edit child: {error}"));
  let incremental = session.analyze_affected().unwrap_or_else(|error| panic!("affected: {error}"));
  let clean = open_session_threads(root.clone(), 1);
  let fresh = clean
    .analyze_with_overlays(&BTreeMap::from([(root.join("Child.vue"), edited_child)]))
    .unwrap_or_else(|error| panic!("fresh: {error}"));
  assert_analysis_parity(&incremental, &fresh);
  let edited_parent = PARENT.replace("toFixed(2)", "toFixed(1)");
  session
    .apply_changes(ChangeSet::upsert(root.join("Parent.vue"), edited_parent.clone()))
    .unwrap_or_else(|error| panic!("edit parent: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("parent affected: {error}"));
  let clean = open_session_threads(root.clone(), 1);
  let fresh = clean
    .analyze_with_overlays(&BTreeMap::from([
      (root.join("Child.vue"), CHILD.replace("default: 1", "default: 2")),
      (root.join("Parent.vue"), edited_parent),
    ]))
    .unwrap_or_else(|error| panic!("parent fresh: {error}"));
  assert_analysis_parity(&incremental, &fresh);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn literal_object_default_reports_sibling_demand() {
  let root = std::env::temp_dir().join(format!("vue-vet-literal-default-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), LITERAL_CHILD)
    .unwrap_or_else(|error| panic!("child: {error}"));
  std::fs::write(root.join("Parent.vue"), SHARED_PARENT)
    .unwrap_or_else(|error| panic!("parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let needle = "right.value.model.n.toFixed(2)";
  let Some(offset) = SHARED_PARENT.find(needle) else {
    panic!("literal demand missing");
  };
  let found = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.rule_id == "vue-vet/reactivity/no-shared-default-cross-instance-demand"
      && diagnostic.file == FileId::from("Parent.vue")
  });
  let Some(diagnostic) = found else {
    panic!("expected literal shared-default demand; {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, offset);
  assert_eq!(diagnostic.span.length, needle.len());
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn kebab_tag_reports_parent_demand_span() {
  let root = std::env::temp_dir().join(format!("vue-vet-kebab-model-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("MyChild.vue"), CHILD).unwrap_or_else(|error| panic!("child: {error}"));
  let parent = PARENT
    .replace("import Child from './Child.vue'", "import MyChild from './MyChild.vue'")
    .replace("<Child v-model=\"value\" />", "<my-child v-model=\"value\" />");
  std::fs::write(root.join("Parent.vue"), &parent)
    .unwrap_or_else(|error| panic!("parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let needle = "value.value.toFixed(2)";
  let Some(offset) = parent.find(needle) else {
    panic!("kebab demand missing");
  };
  let found = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.rule_id == "vue-vet/reactivity/no-model-default-unsynced-parent-demand"
      && diagnostic.file == FileId::from("Parent.vue")
  });
  let Some(diagnostic) = found else {
    panic!("expected kebab parent demand; {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, offset);
  assert_eq!(diagnostic.span.length, needle.len());
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn early_return_and_changed_child_write_stay_quiet() {
  let root = std::env::temp_dir().join(format!("vue-vet-model-guards-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), CHILD).unwrap_or_else(|error| panic!("child: {error}"));
  std::fs::write(
    root.join("Early.vue"),
    PARENT.replace(
      "value.value.toFixed(2)",
      "if (value.value === undefined) return\n  value.value.toFixed(2)",
    ),
  )
  .unwrap_or_else(|error| panic!("early: {error}"));
  let changed_child = r#"<script setup lang="ts">
import { onMounted } from 'vue'
const model = defineModel({ default: 1 })
onMounted(() => {
  model.value = 2
})
defineExpose({ model })
</script>
<template>
  <div>{{ model }}</div>
</template>
"#;
  std::fs::write(root.join("ChangedChild.vue"), changed_child)
    .unwrap_or_else(|error| panic!("changed child: {error}"));
  std::fs::write(
    root.join("Changed.vue"),
    PARENT.replace("import Child from './Child.vue'", "import Child from './ChangedChild.vue'"),
  )
  .unwrap_or_else(|error| panic!("changed parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let hits: Vec<_> = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-model-default-unsynced-parent-demand"
        && (diagnostic.file == FileId::from("Early.vue")
          || diagnostic.file == FileId::from("Changed.vue"))
    })
    .collect();
  assert!(hits.is_empty(), "early-return and changed emit must stay quiet: {hits:?}");
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn unrelated_child_ref_write_still_reports() {
  let root = std::env::temp_dir().join(format!("vue-vet-unrelated-write-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  let child = r#"<script setup lang="ts">
import { ref, onMounted } from 'vue'
const model = defineModel({ default: 1 })
const label = ref('a')
onMounted(() => {
  label.value = 'b'
})
defineExpose({ model })
</script>
<template>
  <div>{{ model }} {{ label }}</div>
</template>
"#;
  std::fs::write(root.join("Child.vue"), child).unwrap_or_else(|error| panic!("child: {error}"));
  std::fs::write(root.join("Parent.vue"), PARENT).unwrap_or_else(|error| panic!("parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  assert!(
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-model-default-unsynced-parent-demand"
        && diagnostic.file == FileId::from("Parent.vue")
    }),
    "unrelated child write must not silence the parent demand: {:?}",
    snapshot.summary.diagnostics
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn shared_unicode_and_crlf_keep_byte_spans() {
  let root = std::env::temp_dir().join(format!("vue-vet-shared-span-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), SHARED_CHILD)
    .unwrap_or_else(|error| panic!("child: {error}"));
  let unicode = r#"<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const 左 = ref(null)
const 右 = ref(null)
onMounted(() => {
  左.value.model.n = 'text'
  右.value.model.n.toFixed(2)
})
</script>
<template>
  <Child ref="左" />
  <Child ref="右" />
</template>
"#;
  std::fs::write(root.join("Unicode.vue"), unicode)
    .unwrap_or_else(|error| panic!("unicode: {error}"));
  let crlf = SHARED_PARENT.replace('\n', "\r\n");
  std::fs::write(root.join("Crlf.vue"), &crlf).unwrap_or_else(|error| panic!("crlf: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let unicode_needle = "右.value.model.n.toFixed(2)";
  let Some(unicode_offset) = unicode.find(unicode_needle) else {
    panic!("unicode shared demand missing");
  };
  let unicode_hit = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.file == FileId::from("Unicode.vue")
      && diagnostic.rule_id == "vue-vet/reactivity/no-shared-default-cross-instance-demand"
  });
  let Some(diagnostic) = unicode_hit else {
    panic!("unicode shared finding missing: {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, unicode_offset);
  assert_eq!(diagnostic.span.length, unicode_needle.len());
  let crlf_needle = "right.value.model.n.toFixed(2)";
  let Some(crlf_offset) = crlf.find(crlf_needle) else {
    panic!("crlf shared demand missing");
  };
  let crlf_hit = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.file == FileId::from("Crlf.vue")
      && diagnostic.rule_id == "vue-vet/reactivity/no-shared-default-cross-instance-demand"
  });
  let Some(diagnostic) = crlf_hit else {
    panic!("crlf shared finding missing: {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span.offset, crlf_offset);
  assert_eq!(diagnostic.span.length, crlf_needle.len());
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn shared_findings_are_linear_in_demand_sites() {
  let root = std::env::temp_dir().join(format!("vue-vet-shared-growth-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  std::fs::write(root.join("Child.vue"), SHARED_CHILD)
    .unwrap_or_else(|error| panic!("child: {error}"));
  let count = 8_usize;
  let mut parent = String::from(
    "<script setup lang=\"ts\">\nimport { ref, onMounted } from 'vue'\nimport Child from './Child.vue'\n",
  );
  for index in 0..count {
    writeln!(parent, "const r{index} = ref(null)").unwrap_or_else(|error| panic!("write: {error}"));
  }
  parent.push_str("onMounted(() => {\n");
  for index in 0..count {
    writeln!(parent, "  r{index}.value.model.n = 'text'")
      .unwrap_or_else(|error| panic!("write: {error}"));
  }
  for index in 0..count {
    writeln!(parent, "  r{index}.value.model.n.toFixed(2)")
      .unwrap_or_else(|error| panic!("write: {error}"));
  }
  parent.push_str("})\n</script>\n<template>\n");
  for index in 0..count {
    writeln!(parent, "  <Child ref=\"r{index}\" />")
      .unwrap_or_else(|error| panic!("write: {error}"));
  }
  parent.push_str("</template>\n");
  std::fs::write(root.join("Parent.vue"), parent).unwrap_or_else(|error| panic!("parent: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let hits: Vec<_> = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-shared-default-cross-instance-demand"
    })
    .collect();
  assert_eq!(
    hits.len(),
    count,
    "shared findings must be one per demand site, not N(N-1); {hits:?}"
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "fixture IO must fail the integration test")]
fn rule_fixtures_match_reporter_snapshots() {
  for (rule, suffix) in [
    (
      "no-model-default-unsynced-parent-demand",
      "vue-vet/reactivity/no-model-default-unsynced-parent-demand",
    ),
    (
      "no-shared-default-cross-instance-demand",
      "vue-vet/reactivity/no-shared-default-cross-instance-demand",
    ),
  ] {
    let source_dir = fixture(&format!("rules/{rule}/invalid"));
    let snap_dir = fixture(&format!("snapshots/{rule}"));
    std::fs::create_dir_all(&snap_dir)
      .unwrap_or_else(|error| panic!("mkdir {snap_dir:?}: {error}"));
    let root = std::env::temp_dir().join(format!("vue-vet-snap-{rule}-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&root);
    copy_dir(&source_dir, &root);
    install_module_seeds_vue_stub(&root);
    let session = open_session_threads(root.clone(), 1);
    let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze {rule}: {error}"));
    let mut by_file: BTreeMap<String, Vec<&vue_vet_core::Diagnostic>> = BTreeMap::new();
    for diagnostic in &snapshot.summary.diagnostics {
      if diagnostic.rule_id == suffix {
        by_file.entry(diagnostic.file.as_str().to_owned()).or_default().push(diagnostic);
      }
    }
    for (file, diagnostics) in &by_file {
      if file.ends_with("Child.vue") {
        continue;
      }
      let stem =
        std::path::Path::new(file).file_stem().and_then(|stem| stem.to_str()).unwrap_or(file);
      let actual = serde_json::to_string_pretty(diagnostics)
        .unwrap_or_else(|error| panic!("serialize {file}: {error}"));
      let snap_path = snap_dir.join(format!("{stem}.json"));
      if std::env::var_os("UPDATE_SOURCE_CONTRACT_SNAPSHOTS").is_some() {
        std::fs::write(&snap_path, format!("{actual}\n"))
          .unwrap_or_else(|error| panic!("write {snap_path:?}: {error}"));
      }
      let expected = std::fs::read_to_string(&snap_path)
        .unwrap_or_else(|error| panic!("missing snapshot {snap_path:?}: {error}"));
      assert_eq!(actual, expected.trim_end(), "snapshot changed for {rule}/{file}");
      assert!(!diagnostics.is_empty(), "{file} must report {rule}");
    }
    let valid_dir = fixture(&format!("rules/{rule}/valid"));
    let valid_root =
      std::env::temp_dir().join(format!("vue-vet-valid-{rule}-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&valid_root);
    copy_dir(&valid_dir, &valid_root);
    install_module_seeds_vue_stub(&valid_root);
    let session = open_session_threads(valid_root.clone(), 1);
    let snapshot = session.analyze().unwrap_or_else(|error| panic!("valid {rule}: {error}"));
    let hits: Vec<_> = snapshot
      .summary
      .diagnostics
      .iter()
      .filter(|diagnostic| diagnostic.rule_id == suffix)
      .collect();
    assert!(hits.is_empty(), "valid {rule} fixtures must stay quiet: {hits:?}");
    let _ignored = std::fs::remove_dir_all(root);
    let _ignored = std::fs::remove_dir_all(valid_root);
  }
}

#[expect(clippy::panic, reason = "fixture copy failures must fail the integration test")]
fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
  std::fs::create_dir_all(to).unwrap_or_else(|error| panic!("mkdir {}: {error}", to.display()));
  for entry in
    std::fs::read_dir(from).unwrap_or_else(|error| panic!("read {}: {error}", from.display()))
  {
    let entry = entry.unwrap_or_else(|error| panic!("entry: {error}"));
    let dest = to.join(entry.file_name());
    if entry.path().is_dir() {
      copy_dir(&entry.path(), &dest);
    } else {
      std::fs::copy(entry.path(), dest).unwrap_or_else(|error| panic!("copy: {error}"));
    }
  }
}
