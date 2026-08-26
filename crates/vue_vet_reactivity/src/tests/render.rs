use super::helpers::*;

#[test]
fn recognizes_render_scopes_for_jsx_shapes_and_factory_wrappers() {
  let options_render = graph_tsx(
    "import { ref } from 'vue';\n\
     const count = ref(0);\n\
     export default { render() { return <div>{count.value}</div>; } };",
  );
  assert!(
    options_render.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Render
        && scope.reads.iter().any(|read| read.binding == "count")
    }),
    "options render must become a Render scope; got {:?}",
    options_render.scopes
  );

  let setup_return = graph_tsx(
    "import { ref, defineComponent } from 'vue';\n\
     const count = ref(0);\n\
     export default defineComponent({ setup() { return () => <div>{count.value}</div>; } });",
  );
  assert!(
    setup_return.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "defineComponent setup→render must become a Render scope"
  );

  let aliased = graph_tsx(
    "import { ref, defineComponent as dc } from 'vue';\n\
     const count = ref(0);\n\
     export default dc({ setup() { return () => <span>{count.value}</span>; } });",
  );
  assert!(
    aliased.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "defineComponent import aliases must resolve as component factories"
  );

  let wrapper = graph_tsx(
    "import { ref, defineComponent } from 'vue';\n\
     const definePage = (options) => defineComponent(options);\n\
     const count = ref(0);\n\
     export default definePage({ setup() { return () => <p>{count.value}</p>; } });",
  );
  assert!(
    wrapper.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "same-file identity forwarders must resolve as component factories"
  );

  let functional = graph_tsx(
    "import { ref } from 'vue';\n\
     const count = ref(0);\n\
     export function Comp() { return <div>{count.value}</div>; }",
  );
  assert!(
    functional.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "exported functional components returning JSX must become Render scopes"
  );

  // Options-object shapes inside unknown factories are still recognized (structure-
  // first). Opaque factories only stay quiet when there is no options/setup/render
  // object and no exported functional component.
  let opaque = graph_tsx(
    "import { ref } from 'vue';\n\
     import { definePage } from '#imports';\n\
     const count = ref(0);\n\
     export default definePage(() => <div>{count.value}</div>);",
  );
  assert!(
    !opaque.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "unknown factory wrapping a bare render callback must stay quiet; got {:?}",
    opaque.scopes
  );
}

#[test]
fn identifier_render_getters_agree_with_inline() {
  fn render_reads(source: &str) -> Vec<(String, Option<String>, ReactiveReadKind)> {
    let graph = graph_tsx(source);
    assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
    graph
      .scopes
      .iter()
      .filter(|scope| scope.kind == TrackingScopeKind::Render)
      .flat_map(|scope| {
        scope.reads.iter().map(|read| (read.binding.clone(), read.property.clone(), read.kind))
      })
      .collect()
  }
  let cases = [
    (
      "options render identifier",
      "import { ref } from 'vue';\n\
       const count = ref(0);\n\
       export default { render() { return <div>{count.value}</div>; } };",
      "import { ref } from 'vue';\n\
       const count = ref(0);\n\
       function renderFn() { return <div>{count.value}</div>; }\n\
       export default { render: renderFn };",
    ),
    (
      "options render peeled identifier",
      "import { ref } from 'vue';\n\
       const count = ref(0);\n\
       export default { render() { return <div>{count.value}</div>; } };",
      "import { ref } from 'vue';\n\
       const count = ref(0);\n\
       const renderFn = () => <div>{count.value}</div>;\n\
       export default { render: (renderFn as () => unknown) };",
    ),
    (
      "setup return identifier",
      "import { ref, defineComponent } from 'vue';\n\
       const count = ref(0);\n\
       export default defineComponent({ setup() { return () => <div>{count.value}</div>; } });",
      "import { ref, defineComponent } from 'vue';\n\
       const count = ref(0);\n\
       function renderFn() { return <div>{count.value}</div>; }\n\
       export default defineComponent({ setup() { return renderFn; } });",
    ),
    (
      "setup return peeled identifier",
      "import { ref, defineComponent } from 'vue';\n\
       const count = ref(0);\n\
       export default defineComponent({ setup() { return () => <div>{count.value}</div>; } });",
      "import { ref, defineComponent } from 'vue';\n\
       const count = ref(0);\n\
       const renderFn = () => <div>{count.value}</div>;\n\
       export default defineComponent({ setup() { return (renderFn); } });",
    ),
  ];
  for (label, inline, ident) in cases {
    let expected = render_reads(inline);
    let actual = render_reads(ident);
    assert_eq!(
      actual, expected,
      "{label}: identifier render getter must match inline; ident={actual:?}"
    );
    assert!(
      actual.iter().any(|read| read.0 == "count" && read.1.as_deref() == Some("value")),
      "{label}: expected count.value; got {actual:?}"
    );
  }
}

#[test]
fn identifier_render_getters_stay_quiet_for_import_async_method() {
  let imported = graph_tsx(
    "import { ref } from 'vue';\n\
     import { renderFn } from './render';\n\
     const count = ref(0);\n\
     export default { render: renderFn };\n\
     void count.value;",
  );
  assert!(
    !imported.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "imported render identifier must stay quiet; scopes={:?}",
    imported.scopes
  );

  let async_fn = graph_tsx(
    "import { ref } from 'vue';\n\
     const count = ref(0);\n\
     async function renderFn() { return <div>{count.value}</div>; }\n\
     export default { render: renderFn };",
  );
  assert!(
    !async_fn.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Render
        && scope.reads.iter().any(|read| read.binding == "count")
    }),
    "async render identifier must stay quiet; scopes={:?}",
    async_fn.scopes
  );

  let method = graph_tsx(
    "import { ref } from 'vue';\n\
     const count = ref(0);\n\
     const api = { renderFn() { return <div>{count.value}</div>; } };\n\
     export default { render: api.renderFn };",
  );
  assert!(
    !method.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "method render identifier must stay quiet; scopes={:?}",
    method.scopes
  );
}

#[test]
fn classifies_conditional_reads_inside_render_scopes() {
  let graph = graph_tsx(
    "import { defineComponent, ref } from 'vue';\n\
     const enabled = ref(false);\n\
     const count = ref(0);\n\
     export default defineComponent(() => {\n\
       return () => {\n\
         if (!enabled.value) return <p>off</p>;\n\
         return <p>{count.value}</p>;\n\
       };\n\
     });",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Render
        && scope.reads.iter().any(|read| {
          read.binding == "count"
            && read.kind == ReactiveReadKind::Conditional
            && read.guards.iter().any(|guard| guard.role == ReactiveGuardRole::EarlyExit)
        })
    }),
    "count behind early-exit in render must be Conditional; got {:?}",
    graph.scopes
  );
}
