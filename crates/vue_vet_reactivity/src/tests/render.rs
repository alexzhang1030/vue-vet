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
