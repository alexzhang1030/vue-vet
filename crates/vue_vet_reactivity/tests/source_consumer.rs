//! Source-API smoke: `ModuleSource` + `trace_modules` only.
//!
//! Does not import Oxc types. Asserts a real `count` read, linked modules,
//! Unicode/CRLF SFC spans, and parse / language errors.

use vue_vet_core::ScriptKind;
use vue_vet_reactivity::{
  ModuleLink, ModuleSource, TraceModulesError, TraceModulesOptions, explain_tracking_scope,
  trace_modules, trace_modules_with_options,
};

#[test]
#[expect(clippy::panic, reason = "source API smoke must fail closed on missing graphs")]
fn source_consumer_traces_standalone_linked_and_sfc_without_oxc_types() {
  let source = "import { ref, computed } from 'vue'\nconst count=ref(1)\nconst result=computed(()=>count.value*2)";
  let module = ModuleSource::standalone("plain.ts", source, "ts", ScriptKind::Script);
  let graphs = match trace_modules(&[module], &[]) {
    Ok(graphs) => graphs,
    Err(error) => panic!("standalone trace: {error}"),
  };
  let Some(plain) = graphs.first() else {
    panic!("standalone graph missing");
  };
  let Some(scope) =
    plain.graph.scopes.iter().find(|scope| scope.binding.as_deref() == Some("result"))
  else {
    panic!("result scope missing: {:?}", plain.graph.scopes);
  };
  let explained = explain_tracking_scope("plain.ts", scope);
  assert!(explained.analysis_complete, "plain.ts analysis must be complete");
  assert!(
    explained.tracks.iter().any(|dep| dep.path == "count.value"),
    "plain.ts must track count.value: {:?}",
    explained.tracks
  );

  let producer = ModuleSource::standalone(
    "producer.ts",
    "import { ref } from 'vue'\nexport const count=ref(1)",
    "ts",
    ScriptKind::Script,
  );
  let consumer = ModuleSource::standalone(
    "consumer.ts",
    "import { computed } from 'vue'\nimport { count } from './producer'\nexport const result=computed(()=>count.value)",
    "ts",
    ScriptKind::Script,
  );
  let modules = [producer, consumer];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let serial = match trace_modules_with_options(
    &modules,
    &links,
    TraceModulesOptions { max_workers: 1, ..TraceModulesOptions::default() },
  ) {
    Ok(graphs) => graphs,
    Err(error) => panic!("serial trace: {error}"),
  };
  let parallel = match trace_modules_with_options(
    &modules,
    &links,
    TraceModulesOptions { max_workers: 4, ..TraceModulesOptions::default() },
  ) {
    Ok(graphs) => graphs,
    Err(error) => panic!("parallel trace: {error}"),
  };
  assert_eq!(serial, parallel, "worker count must not change graphs");
  let Some(consumer_graph) = serial.iter().find(|module| module.id.as_str() == "consumer.ts")
  else {
    panic!("consumer.ts graph missing");
  };
  assert!(
    consumer_graph
      .graph
      .scopes
      .iter()
      .any(|scope| scope.reads.iter().any(|read| read.binding == "count")),
    "consumer.ts must read count: {:?}",
    consumer_graph.graph.scopes
  );

  let sfc = "<template>\u{4e2d}\u{6587}</template>\r\n<script setup lang=\"ts\">\r\nimport { ref, computed } from 'vue'\r\nconst count=ref(1)\r\nconst result=computed(()=>count.value)\r\n</script>";
  let Some(offset) = sfc.find("import") else {
    panic!("SFC import offset");
  };
  let Some(end) = sfc.find("</script>") else {
    panic!("SFC script end");
  };
  let Some(script) = sfc.get(offset..end) else {
    panic!("SFC script slice");
  };
  let module =
    ModuleSource::sfc_script("Unicode.vue", script, "ts", ScriptKind::Setup, offset, sfc);
  let graphs = match trace_modules(&[module], &[]) {
    Ok(graphs) => graphs,
    Err(error) => panic!("sfc trace: {error}"),
  };
  let Some(sfc_module) = graphs.first() else {
    panic!("SFC graph missing");
  };
  let Some(scope) =
    sfc_module.graph.scopes.iter().find(|scope| scope.binding.as_deref() == Some("result"))
  else {
    panic!("SFC result scope missing");
  };
  let Some(read) = scope.reads.first() else {
    panic!("SFC count read missing");
  };
  let end = read.span.offset.saturating_add(read.span.length);
  let Some(snippet) = sfc.get(read.span.offset..end) else {
    panic!("SFC span out of range");
  };
  assert_eq!(snippet, "count.value", "SFC span must cover the count read");
  assert_eq!(read.span.line, 5, "CRLF SFC count read must map to line 5");

  let invalid = ModuleSource::standalone("invalid.ts", "const =", "ts", ScriptKind::Script);
  assert!(
    matches!(trace_modules(&[invalid], &[]), Err(TraceModulesError::Parse { .. })),
    "parse errors must surface"
  );
  let unknown = ModuleSource::standalone("unknown.txt", "hello", "unsupported", ScriptKind::Script);
  assert!(
    matches!(trace_modules(&[unknown], &[]), Err(TraceModulesError::UnsupportedLanguage { .. })),
    "unsupported language must surface"
  );
}
