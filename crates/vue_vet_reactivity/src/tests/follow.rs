use std::collections::BTreeSet;

use super::helpers::*;

#[test]
fn same_file_zero_arg_helper_follow_reads() {
  #[derive(Clone, Copy)]
  enum Want {
    Unconditional,
    Tracked,
    Quiet,
    ThenOutside,
  }
  struct Case {
    label: &'static str,
    source: &'static str,
    kind: TrackingScopeKind,
    binding: &'static str,
    want: Want,
  }
  let cases = [
    Case {
      label: "computed(() => load()) tracks type.value",
      source: "import { ref, computed } from 'vue';\n\
               const type = ref('all');\n\
               function load() { return type.value; }\n\
               const paginator = computed(() => load());\n\
               void paginator.value;",
      kind: TrackingScopeKind::Computed,
      binding: "type",
      want: Want::Unconditional,
    },
    Case {
      label: "watchEffect arrow helper tracks count.value",
      source: "import { ref, watchEffect } from 'vue';\n\
               const count = ref(0);\n\
               const read = () => count.value;\n\
               watchEffect(() => { void read(); });",
      kind: TrackingScopeKind::WatchEffect,
      binding: "count",
      want: Want::Tracked,
    },
    Case {
      label: "two-hop zero-arg helpers track",
      source: "import { ref, computed } from 'vue';\n\
               const n = ref(1);\n\
               function inner() { return n.value; }\n\
               function outer() { return inner(); }\n\
               const c = computed(() => outer());\n\
               void c.value;",
      kind: TrackingScopeKind::Computed,
      binding: "n",
      want: Want::Tracked,
    },
    Case {
      label: "self-recursive helper still tracks its body read",
      source: "import { ref, computed } from 'vue';\n\
               const type = ref('all');\n\
               function load() { load(); return type.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      kind: TrackingScopeKind::Computed,
      binding: "type",
      want: Want::Unconditional,
    },
    Case {
      label: "cyclic helpers still track the body read",
      source: "import { ref, computed } from 'vue';\n\
               const type = ref('all');\n\
               function inner() { return load(); }\n\
               function load() { inner(); return type.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      kind: TrackingScopeKind::Computed,
      binding: "type",
      want: Want::Unconditional,
    },
    Case {
      label: "map callback load() still belongs to the tracking scope",
      source: "import { ref, computed } from 'vue';\n\
               const items = ref([1]);\n\
               const type = ref('all');\n\
               function load() { return type.value; }\n\
               const c = computed(() => items.value.map(() => load()));\n\
               void c.value;",
      kind: TrackingScopeKind::Computed,
      binding: "type",
      want: Want::Unconditional,
    },
    Case {
      label: "then()-only helper stays outside tracking",
      source: "import { ref, watchEffect } from 'vue';\n\
               const count = ref(0);\n\
               function load() { return count.value; }\n\
               watchEffect(() => { Promise.resolve().then(() => load()); });",
      kind: TrackingScopeKind::WatchEffect,
      binding: "count",
      want: Want::ThenOutside,
    },
    Case {
      label: "async helpers stay unfollowed",
      source: "import { ref, computed } from 'vue';\n\
               const type = ref('all');\n\
               async function load() { return type.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      kind: TrackingScopeKind::Computed,
      binding: "type",
      want: Want::Quiet,
    },
    Case {
      label: "args helpers stay unfollowed",
      source: "import { ref, computed } from 'vue';\n\
               const type = ref('all');\n\
               function load(_x: number) { return type.value; }\n\
               const c = computed(() => load(1));\n\
               void c.value;",
      kind: TrackingScopeKind::Computed,
      binding: "type",
      want: Want::Quiet,
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    let scope = helper_follow_scope(&graph, case.kind);
    match case.want {
      Want::Unconditional => {
        assert!(
          scope.is_some_and(|scope| {
            scope.reads.iter().any(|read| {
              read.binding == case.binding
                && read.property.as_deref() == Some("value")
                && read.kind == ReactiveReadKind::Unconditional
            })
          }),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::Tracked => {
        assert!(
          helper_follow_has_value_read(&graph, case.kind, case.binding)
            && scope.is_some_and(|scope| {
              scope.reads.iter().any(|read| {
                read.binding == case.binding && read.kind != ReactiveReadKind::OutsideTracking
              })
            }),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::Quiet => {
        assert!(
          scope.is_none_or(|scope| scope.reads.iter().all(|read| read.binding != case.binding)),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::ThenOutside => {
        assert!(scope.is_some(), "{}: missing scope; scopes={:?}", case.label, graph.scopes);
        let kinds: Vec<_> = scope
          .map(|scope| {
            scope
              .reads
              .iter()
              .filter(|read| read.binding == case.binding)
              .map(|read| read.kind)
              .collect()
          })
          .unwrap_or_default();
        assert!(
          kinds.iter().all(|kind| *kind == ReactiveReadKind::OutsideTracking),
          "{}: reads={kinds:?} scopes={:?}",
          case.label,
          graph.scopes
        );
      }
    }
  }
}

#[test]
fn same_file_zero_arg_helper_follow_uncertain() {
  #[derive(Clone, Copy)]
  enum Want {
    Maybe,
    Quiet,
  }
  struct Case {
    label: &'static str,
    source: &'static str,
    kind: TrackingScopeKind,
    name: &'static str,
    want: Want,
  }
  let cases = [
    Case {
      label: "computed(() => load()) records maybe",
      source: "import { computed } from 'vue';\n\
               declare function useMediaQuery(q: string): { value: boolean };\n\
               const isCoarse = useMediaQuery('(pointer: coarse)');\n\
               function load() { return isCoarse.value; }\n\
               const hint = computed(() => load());",
      kind: TrackingScopeKind::Computed,
      name: "isCoarse",
      want: Want::Maybe,
    },
    Case {
      label: "two-hop helper records maybe",
      source: "import { computed } from 'vue';\n\
               declare function useMediaQuery(q: string): { value: boolean };\n\
               const isCoarse = useMediaQuery('(pointer: coarse)');\n\
               function inner() { return isCoarse.value; }\n\
               function outer() { return inner(); }\n\
               const hint = computed(() => outer());",
      kind: TrackingScopeKind::Computed,
      name: "isCoarse",
      want: Want::Maybe,
    },
    Case {
      label: "watch(() => load()) records maybe",
      source: "import { watch } from 'vue';\n\
               declare function useMediaQuery(q: string): { value: boolean };\n\
               const isCoarse = useMediaQuery('(pointer: coarse)');\n\
               function load() { return isCoarse.value; }\n\
               watch(() => load(), () => {});",
      kind: TrackingScopeKind::WatchSources,
      name: "isCoarse",
      want: Want::Maybe,
    },
    Case {
      label: "helper-wrapped unref(mystery) is maybe",
      source: "import { computed, unref } from 'vue';\n\
               declare const mystery: unknown;\n\
               function load() { return unref(mystery); }\n\
               const hint = computed(() => load());",
      kind: TrackingScopeKind::Computed,
      name: "mystery",
      want: Want::Maybe,
    },
    Case {
      label: "helper-wrapped toValue(mystery) is maybe",
      source: "import { computed, toValue } from 'vue';\n\
               declare const mystery: unknown;\n\
               function load() { return toValue(mystery); }\n\
               const hint = computed(() => load());",
      kind: TrackingScopeKind::Computed,
      name: "mystery",
      want: Want::Maybe,
    },
    Case {
      label: "then()-only helper does not invent maybe",
      source: "import { computed } from 'vue';\n\
               declare function useMediaQuery(q: string): { value: boolean };\n\
               const isCoarse = useMediaQuery('(pointer: coarse)');\n\
               function load() { return isCoarse.value; }\n\
               const hint = computed(() => {\n\
                 Promise.resolve().then(() => load());\n\
                 return 'x';\n\
               });",
      kind: TrackingScopeKind::Computed,
      name: "isCoarse",
      want: Want::Quiet,
    },
    Case {
      label: "mixed in-tracking + then() keeps maybe",
      source: "import { computed } from 'vue';\n\
               declare function useMediaQuery(q: string): { value: boolean };\n\
               const isCoarse = useMediaQuery('(pointer: coarse)');\n\
               function load() { return isCoarse.value; }\n\
               const hint = computed(() => {\n\
                 const now = load();\n\
                 Promise.resolve().then(() => load());\n\
                 return now ? 'a' : 'b';\n\
               });",
      kind: TrackingScopeKind::Computed,
      name: "isCoarse",
      want: Want::Maybe,
    },
    Case {
      label: "async helpers stay unfollowed for uncertain",
      source: "import { computed } from 'vue';\n\
               declare function useMediaQuery(q: string): { value: boolean };\n\
               const isCoarse = useMediaQuery('(pointer: coarse)');\n\
               async function load() { return isCoarse.value; }\n\
               const hint = computed(() => load());",
      kind: TrackingScopeKind::Computed,
      name: "isCoarse",
      want: Want::Quiet,
    },
    Case {
      label: "args helpers stay unfollowed for uncertain",
      source: "import { computed } from 'vue';\n\
               declare function useMediaQuery(q: string): { value: boolean };\n\
               const isCoarse = useMediaQuery('(pointer: coarse)');\n\
               function load(_x: number) { return isCoarse.value; }\n\
               const hint = computed(() => load(1));",
      kind: TrackingScopeKind::Computed,
      name: "isCoarse",
      want: Want::Quiet,
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
    let scope = helper_follow_scope(&graph, case.kind);
    match case.want {
      Want::Maybe => {
        assert!(
          scope.is_some_and(|scope| {
            scope.reads.is_empty() && scope.uncertain_accesses.iter().any(|name| name == case.name)
          }),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::Quiet => {
        assert!(
          scope
            .is_none_or(|scope| { scope.uncertain_accesses.iter().all(|name| name != case.name) }),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
    }
  }
}

#[test]
fn same_file_zero_arg_helper_follow_writes() {
  #[derive(Clone, Copy)]
  enum Want {
    TargetValue,
    Target,
    Quiet,
  }
  struct Case {
    label: &'static str,
    source: &'static str,
    want: Want,
  }
  let cases = [
    Case {
      label: "computed(() => load()) records target.value write",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               function load() { target.value = source.value; return target.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "helper += records target.value write",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               function load() { target.value += source.value; return target.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "helper ++ records target.value write",
      source: "import { ref, computed } from 'vue';\n\
               const target = ref(0);\n\
               function load() { target.value++; return target.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "two-hop helper write is recorded",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               function inner() { target.value = source.value; return target.value; }\n\
               function outer() { return inner(); }\n\
               const c = computed(() => outer());\n\
               void c.value;",
      want: Want::Target,
    },
    Case {
      label: "then()-only helper does not invent writes",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               function load() { target.value = source.value; return target.value; }\n\
               const c = computed(() => {\n\
                 Promise.resolve().then(() => load());\n\
                 return source.value;\n\
               });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "mixed in-tracking + then() keeps the write",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               function load() { target.value = source.value; return target.value; }\n\
               const c = computed(() => {\n\
                 const now = load();\n\
                 Promise.resolve().then(() => load());\n\
                 return now;\n\
               });\n\
               void c.value;",
      want: Want::Target,
    },
    Case {
      label: "async helpers stay unfollowed for writes",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               async function load() { target.value = source.value; return target.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "args helpers stay unfollowed for writes",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               function load(_x: number) { target.value = source.value; return target.value; }\n\
               const c = computed(() => load(1));\n\
               void c.value;",
      want: Want::Quiet,
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
    let scope = helper_follow_scope(&graph, TrackingScopeKind::Computed);
    match case.want {
      Want::TargetValue => {
        assert!(
          scope.is_some_and(|scope| {
            scope
              .writes
              .iter()
              .any(|write| write.binding == "target" && write.property.as_deref() == Some("value"))
          }),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::Target => {
        assert!(
          scope.is_some_and(|scope| scope.writes.iter().any(|write| write.binding == "target")),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::Quiet => {
        assert!(
          scope.is_none_or(|scope| scope.writes.iter().all(|write| write.binding != "target")),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
    }
  }
}

#[test]
fn same_file_zero_arg_helper_follow_assignment_only() {
  struct Case {
    label: &'static str,
    source: &'static str,
    assignment_only: bool,
    expect_full_write: bool,
  }
  let cases = [
    Case {
      label: "watchEffect(() => { assign() }) is assignment_only",
      source: "import { ref, watchEffect } from 'vue';\n\
               const first = ref('a'); const last = ref('b'); const full = ref('');\n\
               function assign() { full.value = first.value + last.value; }\n\
               watchEffect(() => { assign(); });",
      assignment_only: true,
      expect_full_write: true,
    },
    Case {
      label: "watchEffect(() => assign()) expression arrow is assignment_only",
      source: "import { ref, watchEffect } from 'vue';\n\
               const first = ref('a'); const last = ref('b'); const full = ref('');\n\
               const assign = () => { full.value = first.value + last.value; };\n\
               watchEffect(() => assign());",
      assignment_only: true,
      expect_full_write: false,
    },
    Case {
      label: "two-hop assignment-only helpers stay assignment_only",
      source: "import { ref, watchEffect } from 'vue';\n\
               const first = ref('a'); const last = ref('b'); const full = ref('');\n\
               function inner() { full.value = first.value + last.value; }\n\
               function outer() { inner(); }\n\
               watchEffect(() => { outer(); });",
      assignment_only: true,
      expect_full_write: false,
    },
    Case {
      label: "then()-only helper is not assignment_only",
      source: "import { ref, watchEffect } from 'vue';\n\
               const first = ref('a'); const last = ref('b'); const full = ref('');\n\
               function assign() { full.value = first.value + last.value; }\n\
               watchEffect(() => { Promise.resolve().then(() => assign()); });",
      assignment_only: false,
      expect_full_write: false,
    },
    Case {
      label: "async helpers stay unfollowed for assignment_only",
      source: "import { ref, watchEffect } from 'vue';\n\
               const first = ref('a'); const last = ref('b'); const full = ref('');\n\
               async function assign() { full.value = first.value + last.value; }\n\
               watchEffect(() => { assign(); });",
      assignment_only: false,
      expect_full_write: false,
    },
    Case {
      label: "helper += is assignment_only with a write",
      source: "import { ref, watchEffect } from 'vue';\n\
               const first = ref(1); const full = ref(0);\n\
               function assign() { full.value += first.value; }\n\
               watchEffect(() => { assign(); });",
      assignment_only: true,
      expect_full_write: true,
    },
    Case {
      label: "helper ++ is assignment_only with a write",
      source: "import { ref, watchEffect } from 'vue';\n\
               const full = ref(0);\n\
               function assign() { full.value++; }\n\
               watchEffect(() => { assign(); });",
      assignment_only: true,
      expect_full_write: true,
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    let scope = helper_follow_scope(&graph, TrackingScopeKind::WatchEffect);
    assert_eq!(
      scope.map(|scope| scope.assignment_only),
      Some(case.assignment_only),
      "{}: scopes={:?}",
      case.label,
      graph.scopes
    );
    if case.expect_full_write {
      assert!(
        scope.is_some_and(|scope| {
          scope
            .writes
            .iter()
            .any(|write| write.binding == "full" && write.property.as_deref() == Some("value"))
        }),
        "{}: missing full.value write; scopes={:?}",
        case.label,
        graph.scopes
      );
    }
  }
}

#[test]
fn sync_hof_callback_nested_reads() {
  struct Case {
    label: &'static str,
    source: &'static str,
    tracked: &'static [&'static str],
  }
  let cases = [
    Case {
      label: "String.replace replacer",
      source: "import { ref, computed } from 'vue';\n\
               const text = ref('ab');\n\
               const flag = ref(true);\n\
               const d = computed(() => text.value.replace(/./g, c => flag.value ? c : ''));\n\
               void d.value;",
      tracked: &["text", "flag"],
    },
    Case {
      label: "String.replaceAll replacer",
      source: "import { ref, computed } from 'vue';\n\
               const text = ref('ab');\n\
               const flag = ref(true);\n\
               const d = computed(() => text.value.replaceAll(/./g, c => flag.value ? c : ''));\n\
               void d.value;",
      tracked: &["text", "flag"],
    },
    Case {
      label: "Array.from mapFn",
      source: "import { ref, computed } from 'vue';\n\
               const list = ref([1, 2]);\n\
               const factor = ref(2);\n\
               const d = computed(() => Array.from(list.value, x => x * factor.value));\n\
               void d.value;",
      tracked: &["list", "factor"],
    },
    Case {
      label: "JSON.parse reviver",
      source: "import { ref, computed } from 'vue';\n\
               const raw = ref('{\"a\":1}');\n\
               const flag = ref(true);\n\
               const d = computed(() => JSON.parse(raw.value, (k, v) => flag.value ? v : v));\n\
               void d.value;",
      tracked: &["raw", "flag"],
    },
    Case {
      label: "Array.from first-arg function stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const factor = ref(2);\n\
               const d = computed(() => Array.from(() => factor.value));\n\
               void d.value;",
      tracked: &[],
    },
    Case {
      label: "JSON.parse first-arg function stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const flag = ref(true);\n\
               const d = computed(() => JSON.parse(() => flag.value));\n\
               void d.value;",
      tracked: &[],
    },
    Case {
      label: "String.replace first-arg function stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const text = ref('ab');\n\
               const flag = ref(true);\n\
               const d = computed(() => text.value.replace(() => flag.value));\n\
               void d.value;",
      tracked: &["text"],
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    let scope = helper_follow_scope(&graph, TrackingScopeKind::Computed);
    let missing = case.tracked.iter().copied().find(|binding| {
      !scope.is_some_and(|scope| {
        scope
          .reads
          .iter()
          .any(|read| read.binding == *binding && read.property.as_deref() == Some("value"))
      })
    });
    assert!(
      missing.is_none(),
      "{}: missing {:?}.value; scopes={:?}",
      case.label,
      missing,
      graph.scopes
    );
    let invented = scope.is_some_and(|scope| {
      scope.reads.iter().any(|read| {
        matches!(read.binding.as_str(), "factor" | "flag")
          && read.property.as_deref() == Some("value")
          && !case.tracked.contains(&read.binding.as_str())
      })
    });
    assert!(
      !invented,
      "{}: first-arg function must not invent nested reads; scopes={:?}",
      case.label, graph.scopes
    );
  }
}

#[test]
fn sync_hof_and_to_value_callback_nested_writes() {
  #[derive(Clone, Copy)]
  enum Want {
    TargetValue,
    Quiet,
  }
  struct Case {
    label: &'static str,
    source: &'static str,
    want: Want,
  }
  let cases = [
    Case {
      label: "list.value.map writes target.value",
      source: "import { ref, computed } from 'vue';\n\
               const list = ref([0]); const target = ref(0);\n\
               const c = computed(() => {\n\
                 list.value.map(() => { target.value = 1; });\n\
                 return list.value;\n\
               });\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "forEach += writes target.value",
      source: "import { ref, computed } from 'vue';\n\
               const list = ref([1]); const target = ref(0);\n\
               const c = computed(() => {\n\
                 list.value.forEach(() => { target.value += 1; });\n\
                 return list.value;\n\
               });\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "Array.from mapFn writes target.value",
      source: "import { ref, computed } from 'vue';\n\
               const list = ref([1]); const target = ref(0);\n\
               const c = computed(() => Array.from(list.value, () => { target.value = 1; return 1; }));\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "toValue getter writes target.value",
      source: "import { ref, computed, toValue } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               const c = computed(() => toValue(() => { target.value = 1; return source.value; }));\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "helper-wrapped map write",
      source: "import { ref, computed } from 'vue';\n\
               const list = ref([0]); const target = ref(0);\n\
               function load() {\n\
                 list.value.map(() => { target.value = 1; });\n\
                 return list.value;\n\
               }\n\
               const c = computed(() => load());\n\
               void c.value;",
      want: Want::TargetValue,
    },
    Case {
      label: "then() callback write stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               const c = computed(() => {\n\
                 Promise.resolve().then(() => { target.value = 1; });\n\
                 return source.value;\n\
               });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "Array.from first-arg function write stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const target = ref(0);\n\
               const c = computed(() => Array.from(() => { target.value = 1; return 1; }));\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "setTimeout write stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const source = ref(0); const target = ref(0);\n\
               const c = computed(() => {\n\
                 setTimeout(() => { target.value = 1; });\n\
                 return source.value;\n\
               });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "identifier map(fn) write stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const list = ref([0]); const target = ref(0);\n\
               function mapper() { target.value = 1; return 1; }\n\
               const c = computed(() => list.value.map(mapper));\n\
               void c.value;",
      want: Want::Quiet,
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
    let scope = helper_follow_scope(&graph, TrackingScopeKind::Computed);
    match case.want {
      Want::TargetValue => {
        assert!(
          scope.is_some_and(|scope| {
            scope
              .writes
              .iter()
              .any(|write| write.binding == "target" && write.property.as_deref() == Some("value"))
          }),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::Quiet => {
        assert!(
          scope.is_none_or(|scope| scope.writes.iter().all(|write| write.binding != "target")),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
    }
  }
}

#[derive(Clone, Copy)]
enum IdentGetterWant {
  ComputedType,
  WatchEffectCount,
  WatchSourceValue,
  WriteTarget,
  AssignmentOnly,
  Quiet,
}

fn assert_ident_getter(source: &str, kind: TrackingScopeKind, want: IdentGetterWant, label: &str) {
  let graph = graph(source);
  assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
  let scope = helper_follow_scope(&graph, kind);
  let ok = match want {
    IdentGetterWant::ComputedType => scope.is_some_and(|scope| {
      scope.reads.iter().any(|read| {
        read.binding == "type"
          && read.property.as_deref() == Some("value")
          && read.kind == ReactiveReadKind::Unconditional
      })
    }),
    IdentGetterWant::WatchEffectCount => helper_follow_has_value_read(&graph, kind, "count"),
    IdentGetterWant::WatchSourceValue => scope.is_some_and(|scope| {
      scope.reads.iter().any(|read| {
        read.binding == "value"
          && read.property.as_deref() == Some("value")
          && read.kind != ReactiveReadKind::OutsideTracking
      })
    }),
    IdentGetterWant::WriteTarget => scope.is_some_and(|scope| {
      scope
        .writes
        .iter()
        .any(|write| write.binding == "target" && write.property.as_deref() == Some("value"))
    }),
    IdentGetterWant::AssignmentOnly => scope.is_some_and(|scope| scope.assignment_only),
    IdentGetterWant::Quiet => scope.is_none_or(|scope| {
      scope.reads.iter().all(|read| read.binding != "type")
        && scope.writes.iter().all(|write| write.binding != "target")
    }),
  };
  assert!(ok, "{label}: scopes={:?}", graph.scopes);
}

fn computed_type_reads(
  graph: &vue_vet_core::ReactivityGraph,
) -> BTreeSet<(String, Option<String>)> {
  helper_follow_scope(graph, TrackingScopeKind::Computed)
    .map(|scope| {
      scope
        .reads
        .iter()
        .filter(|read| read.kind != ReactiveReadKind::OutsideTracking)
        .map(|read| (read.binding.clone(), read.property.clone()))
        .collect()
    })
    .unwrap_or_default()
}

#[test]
fn identifier_getter_callback_tracks_like_inline() {
  for (label, source, kind, want) in [
    (
      "computed(load) tracks type.value",
      "import { ref, computed } from 'vue';\n\
       const type = ref('all');\n\
       function load() { return type.value; }\n\
       const paginator = computed(load);\n\
       void paginator.value;",
      TrackingScopeKind::Computed,
      IdentGetterWant::ComputedType,
    ),
    (
      "computed((load)) peels parens / TS wrappers",
      "import { ref, computed } from 'vue';\n\
       const type = ref('all');\n\
       const load = () => type.value;\n\
       const paginator = computed((load as () => string));\n\
       void paginator.value;",
      TrackingScopeKind::Computed,
      IdentGetterWant::ComputedType,
    ),
    (
      "computed({ get: load }) tracks type.value",
      "import { ref, computed } from 'vue';\n\
       const type = ref('all');\n\
       function load() { return type.value; }\n\
       const paginator = computed({ get: load, set() {} });\n\
       void paginator.value;",
      TrackingScopeKind::Computed,
      IdentGetterWant::ComputedType,
    ),
    (
      "watchEffect(load) tracks count.value",
      "import { ref, watchEffect } from 'vue';\n\
       const count = ref(0);\n\
       function load() { return count.value; }\n\
       watchEffect(load);",
      TrackingScopeKind::WatchEffect,
      IdentGetterWant::WatchEffectCount,
    ),
    (
      "function with unused params still tracks as identifier getter",
      "import { ref, computed } from 'vue';\n\
       const type = ref('all');\n\
       function load(_x: number) { return type.value; }\n\
       const c = computed(load);\n\
       void c.value;",
      TrackingScopeKind::Computed,
      IdentGetterWant::ComputedType,
    ),
    (
      "watch(load) source getter tracks value.value",
      "import { ref, watch } from 'vue';\n\
       const value = ref(0);\n\
       function load() { return value.value; }\n\
       watch(load, () => {});",
      TrackingScopeKind::WatchSources,
      IdentGetterWant::WatchSourceValue,
    ),
    (
      "watch([load]) array source getter tracks value.value",
      "import { ref, watch } from 'vue';\n\
       const value = ref(0);\n\
       function load() { return value.value; }\n\
       watch([load], () => {});",
      TrackingScopeKind::WatchSources,
      IdentGetterWant::WatchSourceValue,
    ),
    (
      "computed(load) records helper writes",
      "import { ref, computed } from 'vue';\n\
       const source = ref(0); const target = ref(0);\n\
       function load() { target.value = source.value; return target.value; }\n\
       const c = computed(load);\n\
       void c.value;",
      TrackingScopeKind::Computed,
      IdentGetterWant::WriteTarget,
    ),
    (
      "watchEffect(assign) is assignment_only",
      "import { ref, watchEffect } from 'vue';\n\
       const first = ref('a'); const last = ref('b'); const full = ref('');\n\
       function assign() { full.value = first.value + last.value; }\n\
       watchEffect(assign);",
      TrackingScopeKind::WatchEffect,
      IdentGetterWant::AssignmentOnly,
    ),
    (
      "imported getter stays quiet",
      "import { ref, computed } from 'vue';\n\
       import { load } from './helpers';\n\
       const type = ref('all');\n\
       const c = computed(load);\n\
       void type.value; void c;",
      TrackingScopeKind::Computed,
      IdentGetterWant::Quiet,
    ),
    (
      "async identifier getter stays quiet",
      "import { ref, computed } from 'vue';\n\
       const type = ref('all');\n\
       async function load() { return type.value; }\n\
       const c = computed(load);\n\
       void c.value;",
      TrackingScopeKind::Computed,
      IdentGetterWant::Quiet,
    ),
    (
      "method identifier getter stays quiet",
      "import { ref, computed } from 'vue';\n\
       const type = ref('all');\n\
       const obj = { load() { return type.value; } };\n\
       const c = computed(obj.load);\n\
       void c.value;",
      TrackingScopeKind::Computed,
      IdentGetterWant::Quiet,
    ),
  ] {
    assert_ident_getter(source, kind, want, label);
  }
}

#[test]
fn identifier_getter_agrees_with_helper_call() {
  let inline = graph(
    "import { ref, computed } from 'vue';\n\
     const type = ref('all');\n\
     function load() { return type.value; }\n\
     const paginator = computed(() => load());\n\
     void paginator.value;",
  );
  let ident = graph(
    "import { ref, computed } from 'vue';\n\
     const type = ref('all');\n\
     function load() { return type.value; }\n\
     const paginator = computed(load);\n\
     void paginator.value;",
  );
  assert_eq!(
    computed_type_reads(&inline),
    computed_type_reads(&ident),
    "computed(load) must agree with computed(() => load()) on tracking reads"
  );
}

#[test]
fn followed_helper_inherits_caller_guards() {
  struct Case {
    label: &'static str,
    source: &'static str,
    binding: &'static str,
    kind: ReactiveReadKind,
    guard: Option<&'static str>,
  }
  let cases = [
    Case {
      label: "ternary helper call is Conditional",
      source: "import { ref, computed } from 'vue';\n\
               const cond = ref(true);\n\
               const type = ref('all');\n\
               function load() { return type.value; }\n\
               const c = computed(() => (cond.value ? load() : 0));\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Conditional,
      guard: Some("cond"),
    },
    Case {
      label: "inline ternary stays Conditional",
      source: "import { ref, computed } from 'vue';\n\
               const cond = ref(true);\n\
               const type = ref('all');\n\
               const c = computed(() => (cond.value ? type.value : 0));\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Conditional,
      guard: Some("cond"),
    },
    Case {
      label: "both-arm helper calls stay Unconditional",
      source: "import { ref, computed } from 'vue';\n\
               const cond = ref(true);\n\
               const type = ref('all');\n\
               function load() { return type.value; }\n\
               const c = computed(() => (cond.value ? load() : load()));\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Unconditional,
      guard: None,
    },
    Case {
      label: "unguarded call plus ternary call stays Unconditional",
      source: "import { ref, computed } from 'vue';\n\
               const cond = ref(true);\n\
               const type = ref('all');\n\
               function load() { return type.value; }\n\
               const c = computed(() => { load(); return cond.value ? load() : 0; });\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Unconditional,
      guard: None,
    },
    Case {
      label: "two-hop outer() in ternary is Conditional",
      source: "import { ref, computed } from 'vue';\n\
               const cond = ref(true);\n\
               const type = ref('all');\n\
               function inner() { return type.value; }\n\
               function outer() { return inner(); }\n\
               const c = computed(() => (cond.value ? outer() : 0));\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Conditional,
      guard: Some("cond"),
    },
    Case {
      label: "inner ternary inside unconditionally called helper is Conditional",
      source: "import { ref, computed } from 'vue';\n\
               const cond = ref(true);\n\
               const type = ref('all');\n\
               function load() { return type.value; }\n\
               function outer() { return cond.value ? load() : 0; }\n\
               const c = computed(() => outer());\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Conditional,
      guard: Some("cond"),
    },
    Case {
      label: "early-exit inside helper is Conditional",
      source: "import { ref, computed } from 'vue';\n\
               const ready = ref(true);\n\
               const type = ref('all');\n\
               function load() { if (!ready.value) return 0; return type.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Conditional,
      guard: Some("ready"),
    },
    Case {
      label: "unconditional helper call stays Unconditional",
      source: "import { ref, computed } from 'vue';\n\
               const type = ref('all');\n\
               function load() { return type.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      binding: "type",
      kind: ReactiveReadKind::Unconditional,
      guard: None,
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
    let scope = helper_follow_scope(&graph, TrackingScopeKind::Computed);
    let read = scope.and_then(|scope| {
      scope
        .reads
        .iter()
        .find(|read| read.binding == case.binding && read.property.as_deref() == Some("value"))
    });
    assert_eq!(
      read.map(|read| read.kind),
      Some(case.kind),
      "{}: kind; scopes={:?}",
      case.label,
      graph.scopes
    );
    assert_eq!(
      read.and_then(|read| read.guarded_by.as_deref()),
      case.guard,
      "{}: guard; read={read:?}",
      case.label
    );
  }
}

fn effect_value_kind(source: &str, binding: &str) -> Option<ReactiveReadKind> {
  let graph = graph(source);
  assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
  helper_follow_scope(&graph, TrackingScopeKind::WatchEffect).and_then(|scope| {
    scope
      .reads
      .iter()
      .find(|read| read.binding == binding && read.property.as_deref() == Some("value"))
      .map(|read| read.kind)
  })
}

#[test]
fn followed_helper_inherits_caller_pause() {
  struct Case {
    label: &'static str,
    source: &'static str,
    binding: &'static str,
    kind: ReactiveReadKind,
  }
  let cases = [
    Case {
      label: "pause inside helper is OutsideTracking",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               function load() { pauseTracking(); return value.value; }\n\
               watchEffect(() => { void load(); });",
      binding: "value",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "helper declared after the effect still classifies pause",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               watchEffect(() => { void load(); });\n\
               function load() { pauseTracking(); return value.value; }",
      binding: "value",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "caller pause then load() is OutsideTracking",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               function load() { return value.value; }\n\
               watchEffect(() => { pauseTracking(); void load(); });",
      binding: "value",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "inline pause stays OutsideTracking",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               watchEffect(() => { pauseTracking(); void value.value; });",
      binding: "value",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "enableTracking inside helper resumes",
      source: "import { ref, watchEffect, pauseTracking, enableTracking } from 'vue';\n\
               const resumed = ref(1);\n\
               function load() { pauseTracking(); enableTracking(); return resumed.value; }\n\
               watchEffect(() => { void load(); });",
      binding: "resumed",
      kind: ReactiveReadKind::Unconditional,
    },
    Case {
      label: "caller pause then helper enableTracking tracks",
      source: "import { ref, watchEffect, pauseTracking, enableTracking } from 'vue';\n\
               const value = ref(0);\n\
               function load() { enableTracking(); return value.value; }\n\
               watchEffect(() => { pauseTracking(); void load(); });",
      binding: "value",
      kind: ReactiveReadKind::Unconditional,
    },
    Case {
      label: "unpaused call plus later paused call stays Unconditional",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               function load() { return value.value; }\n\
               watchEffect(() => { load(); pauseTracking(); load(); });",
      binding: "value",
      kind: ReactiveReadKind::Unconditional,
    },
    Case {
      label: "two-hop pause in inner is OutsideTracking",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               function inner() { pauseTracking(); return value.value; }\n\
               function outer() { return inner(); }\n\
               watchEffect(() => { void outer(); });",
      binding: "value",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "two-hop pause in outer before inner() is OutsideTracking",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               function inner() { return value.value; }\n\
               function outer() { pauseTracking(); return inner(); }\n\
               watchEffect(() => { void outer(); });",
      binding: "value",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "identifier getter with pause is OutsideTracking",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const value = ref(0);\n\
               function load() { pauseTracking(); return value.value; }\n\
               watchEffect(load);",
      binding: "value",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "helper pause leaks to a later sibling read",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const after = ref(2);\n\
               function load() { pauseTracking(); return 0; }\n\
               watchEffect(() => { load(); void after.value; });",
      binding: "after",
      kind: ReactiveReadKind::OutsideTracking,
    },
    Case {
      label: "read before a pausing helper still tracks",
      source: "import { ref, watchEffect, pauseTracking } from 'vue';\n\
               const before = ref(1);\n\
               function load() { pauseTracking(); return 0; }\n\
               watchEffect(() => { void before.value; load(); });",
      binding: "before",
      kind: ReactiveReadKind::Unconditional,
    },
  ];
  for case in cases {
    assert_eq!(
      effect_value_kind(case.source, case.binding),
      Some(case.kind),
      "{}: scopes={:?}",
      case.label,
      graph(case.source).scopes
    );
  }
}

#[test]
fn helper_pause_agrees_with_inline() {
  let inline = effect_value_kind(
    "import { ref, watchEffect, pauseTracking } from 'vue';\n\
     const value = ref(0);\n\
     watchEffect(() => { pauseTracking(); void value.value; });",
    "value",
  );
  let helper = effect_value_kind(
    "import { ref, watchEffect, pauseTracking } from 'vue';\n\
     const value = ref(0);\n\
     function load() { pauseTracking(); return value.value; }\n\
     watchEffect(() => { void load(); });",
    "value",
  );
  assert_eq!(inline, Some(ReactiveReadKind::OutsideTracking));
  assert_eq!(helper, inline, "computed/effect helper pause must agree with the inlined window");
}
