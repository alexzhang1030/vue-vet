use super::helpers::*;

#[test]
fn define_component_props_member_reads_track_in_computed() {
  let options = graph(
    "import { computed, defineComponent } from 'vue';\n\
     export default defineComponent({\n\
       props: { displayMode: String },\n\
       setup(props) {\n\
         const mode = computed(() => props.displayMode || 'whiteboard');\n\
         return () => mode.value;\n\
       },\n\
     });",
  );
  assert!(
    options.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("displayMode"))
        && scope.uncertain_accesses.is_empty()
    }),
    "setup(props).displayMode must track; got {:?}",
    options.scopes
  );

  let functional = graph_tsx(
    "import { computed, defineComponent } from 'vue';\n\
     export default defineComponent((props: { title: string }) => {\n\
       const label = computed(() => props.title);\n\
       return () => <p>{label.value}</p>;\n\
     });",
  );
  assert!(
    functional.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("title"))
        && scope.uncertain_accesses.is_empty()
    }),
    "defineComponent((props) => props.title) must track; got {:?}",
    functional.scopes
  );

  // Opaque project helper — no Vue `defineComponent` link ⇒ quiet (under-approx).
  let opaque = graph_tsx(
    "import { computed } from 'vue';\n\
     declare function defineTypedComponent<P>(setup: (props: P) => unknown): unknown;\n\
     export const Panel = defineTypedComponent<{ open: boolean }>((props) => {\n\
       const shown = computed(() => props.open);\n\
       return () => <div>{shown.value}</div>;\n\
     });",
  );
  assert!(
    opaque.scopes.iter().all(|scope| {
      scope.kind != TrackingScopeKind::Computed
        || !scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
    }),
    "opaque defineTypedComponent must not invent props tracking; got {:?}",
    opaque.scopes
  );

  // Same-file identity forwarder to Vue `defineComponent` ⇒ props seed.
  let forwarded = graph_tsx(
    "import { computed, defineComponent } from 'vue';\n\
     const defineTypedComponent = <P,>(setup: (props: P) => unknown) => defineComponent(setup);\n\
     export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
       const shown = computed(() => props.open);\n\
       return () => <div>{shown.value}</div>;\n\
     });",
  );
  assert!(
    forwarded.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
        && scope.uncertain_accesses.is_empty()
    }),
    "defineComponent identity forwarder must seed props; got {:?}",
    forwarded.scopes
  );

  // Same-file multi-arg / alias wrap ⇒ ComponentFactory props seed.
  let multi_arg = graph_tsx(
    "import { computed, defineComponent } from 'vue';\n\
     function defineTypedComponent<P>(setup: (props: P) => unknown, extra?: object) {\n\
       const _setup = setup as any;\n\
       const _props = extra as any;\n\
       return defineComponent(_setup, _props) as unknown as (props: P) => unknown;\n\
     }\n\
     export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
       const shown = computed(() => props.open);\n\
       return () => <div>{shown.value}</div>;\n\
     });",
  );
  assert!(
    multi_arg.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
        && scope.uncertain_accesses.is_empty()
    }),
    "multi-arg defineComponent wrap must seed props; got {:?}",
    multi_arg.scopes
  );
}

#[test]
fn cross_module_component_factory_wrapper_seeds_props() {
  let modules = [
    ModuleSource::standalone(
      "factory.ts",
      "import { defineComponent } from 'vue';\n\
       export function defineTypedComponent(setup, extra) {\n\
         const _setup = setup;\n\
         const _props = extra;\n\
         return defineComponent(_setup, _props);\n\
       }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.tsx",
      "import { computed } from 'vue';\n\
       import { defineTypedComponent } from './factory';\n\
       export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
         const shown = computed(() => props.open);\n\
         return () => shown.value;\n\
       });",
      "tsx",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.tsx".into(),
    specifier: "./factory".into(),
    to: "factory.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.tsx");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "props" && binding.kind == ReactiveBindingKind::Reactive)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "cross-module ComponentFactory must seed props; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn cross_module_opaque_component_helper_does_not_seed_props() {
  let modules = [
    ModuleSource::standalone(
      "factory.ts",
      "export function defineTypedComponent(setup) { return setup; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.tsx",
      "import { computed } from 'vue';\n\
       import { defineTypedComponent } from './factory';\n\
       export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
         const shown = computed(() => props.open);\n\
         return () => shown.value;\n\
       });",
      "tsx",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.tsx".into(),
    specifier: "./factory".into(),
    to: "factory.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.tsx");
  assert!(
    consumer.is_some_and(|module| {
      !module.graph.bindings.iter().any(|binding| binding.name == "props")
        && module.graph.scopes.iter().all(|scope| {
          scope.kind != TrackingScopeKind::Computed
            || !scope
              .reads
              .iter()
              .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
        })
    }),
    "opaque helper must not invent props tracking; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}
