//! Link-time fixed points over resolved module links.
//!
//! [`resolve_exports`] refines `ExportState` through barrels, forwards, and value
//! bags; the callback-slot resolvers propagate declared callback shapes through
//! the same barrel edges. Pure lattice rules live in [`super::super::export_lattice`].

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use vue_vet_core::ModuleId;

use super::super::{
  ComposableShape, ExportState, ExportSummary, ModuleExportFacts, ModuleLink, OptionsCallbackSlots,
  TraceModulesError, TypedCallbackParamSlots, ValueBag, export_lattice,
};

pub(super) fn resolved_links_partial(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &[ModuleLink],
) -> (BTreeMap<(ModuleId, String), ModuleId>, Vec<TraceModulesError>) {
  let mut resolved = BTreeMap::new();
  let mut ambiguous = BTreeSet::new();
  let mut issues = Vec::new();
  for link in links {
    if !facts.contains_key(&link.from) || !facts.contains_key(&link.to) {
      issues.push(TraceModulesError::UnknownLink { from: link.from.clone(), to: link.to.clone() });
      continue;
    }
    let key = (link.from.clone(), link.specifier.clone());
    if ambiguous.contains(&key) {
      continue;
    }
    match resolved.entry(key) {
      Entry::Vacant(entry) => {
        entry.insert(link.to.clone());
      }
      Entry::Occupied(entry) if entry.get() == &link.to => {}
      Entry::Occupied(entry) => {
        let key = entry.key().clone();
        entry.remove();
        ambiguous.insert(key);
        issues.push(TraceModulesError::AmbiguousLink {
          from: link.from.clone(),
          specifier: link.specifier.clone(),
        });
      }
    }
  }
  (resolved, issues)
}

pub(super) fn resolve_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, ExportState>> {
  use std::collections::VecDeque;

  let mut resolved =
    facts.keys().map(|id| (id.clone(), BTreeMap::new())).collect::<BTreeMap<_, _>>();

  // Cheap path: seed finished locals once (same as pre-forward linking).
  // `import { x as y }; export { y }` barrels resolve via import links in the
  // fixed-point below — only locals with finished state are inserted here.
  for (id, module_facts) in facts {
    for export in &module_facts.summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if let Some(state) = module_facts.summary.locals.get(local) {
        insert_export(&mut resolved, id, exported, state.clone());
      }
    }
  }

  // Working copies only for modules that still need ForwardReturn / value-bag /
  // generic-method instantiate refine.
  let mut working_locals: BTreeMap<ModuleId, BTreeMap<String, ExportState>> = BTreeMap::new();
  for (id, module_facts) in facts {
    if module_facts.summary.locals.values().any(|state| {
      matches!(
        state,
        ExportState::ForwardReturn(_)
          | ExportState::ValueFactory(_)
          | ExportState::ValueBag(_)
          | ExportState::ValueFactoryCall(_)
          | ExportState::GenericMethodInstantiate { .. }
      ) || matches!(
        state,
        ExportState::Composable(shape) if shape.has_pending_value_bag_fields()
      )
    }) {
      working_locals.insert(id.clone(), module_facts.summary.locals.clone());
    }
  }

  // target module → consumers that import/re-export from it
  let mut reverse_users: BTreeMap<&ModuleId, Vec<&ModuleId>> = BTreeMap::new();
  for ((from, _), to) in links {
    reverse_users.entry(*to).or_default().push(*from);
  }

  let mut queue = VecDeque::new();
  let mut queued = BTreeSet::new();
  for (id, module_facts) in facts {
    let needs_reexport =
      module_facts.summary.exports.iter().any(|export| {
        matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. })
      });
    // `import { d as x }; export { x }` — Local export name is not in `locals`.
    let needs_import_local_export = module_facts.summary.exports.iter().any(|export| {
      matches!(
        export,
        ExportSummary::Local { local, .. }
          if !module_facts.summary.locals.contains_key(local)
            && module_facts.summary.imports.iter().any(|import| import.local == *local)
      )
    });
    if needs_reexport || working_locals.contains_key(id) || needs_import_local_export {
      queue.push_back(id);
      queued.insert(id);
    }
  }

  while let Some(id) = queue.pop_front() {
    queued.remove(id);
    let Some(module_facts) = facts.get(id) else {
      continue;
    };
    let mut changed = false;
    let mut refined_forward = false;

    // Resolve ForwardReturn / refine value bags against imports + working locals.
    if let Some(locals) = working_locals.get_mut(id) {
      let names: Vec<String> = locals.keys().cloned().collect();
      for name in names {
        let Some(state) = locals.get(&name).cloned() else {
          continue;
        };
        if !matches!(
          state,
          ExportState::ForwardReturn(_)
            | ExportState::ValueFactory(_)
            | ExportState::ValueBag(_)
            | ExportState::ValueFactoryCall(_)
            | ExportState::Composable(_)
            | ExportState::GenericMethodInstantiate { .. }
        ) {
          continue;
        }
        let refined = refine_export_state(state, id, locals, facts, links, &resolved);
        if locals.get(&name) != Some(&refined) {
          refined_forward = true;
          locals.insert(name, refined);
          changed = true;
        }
      }
      for export in &module_facts.summary.exports {
        let ExportSummary::Local { local, exported } = export else {
          continue;
        };
        if let Some(state) = locals.get(local)
          && let Some(publish) =
            publishable_export_state(state, id, locals, facts, links, &resolved)
        {
          changed |= insert_export(&mut resolved, id, exported, publish);
        }
      }
    }

    for export in &module_facts.summary.exports {
      match export {
        ExportSummary::Local { local, exported } => {
          // Barrel: `import { d as defineTypedComponent }; export { defineTypedComponent }`.
          if module_facts.summary.locals.contains_key(local) {
            continue;
          }
          if let Some(state) =
            resolve_name_export_state(id, local, &BTreeMap::new(), facts, links, &resolved, 0)
          {
            changed |= insert_export(&mut resolved, id, exported, state);
          }
        }
        ExportSummary::Reexport { source, imported, exported } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(state) = resolved.get(target).and_then(|exports| exports.get(imported)).cloned()
          else {
            continue;
          };
          changed |= insert_export(&mut resolved, id, exported, state);
        }
        ExportSummary::Star { source } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(target_exports) = resolved.get(target).cloned() else {
            continue;
          };
          for (exported, state) in target_exports {
            if exported != "default" {
              changed |= insert_export(&mut resolved, id, &exported, state);
            }
          }
        }
      }
    }
    if !changed {
      continue;
    }
    if let Some(users) = reverse_users.get(id) {
      for consumer in users {
        if queued.insert(consumer) {
          queue.push_back(consumer);
        }
      }
    }
    // Only re-enter when a forward/value-bag refine may unlock more locals.
    if refined_forward && working_locals.contains_key(id) && queued.insert(id) {
      queue.push_back(id);
    }
  }

  resolved
}

fn refine_export_state(
  state: ExportState,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> ExportState {
  match state {
    ExportState::ForwardReturn(callee) => export_lattice::refine_forward_return(
      resolve_name_export_state(module_id, &callee, locals, facts, links, resolved, 0),
      callee,
    ),
    ExportState::ValueFactory(bag) => {
      ExportState::ValueFactory(refine_value_bag(bag, module_id, locals, facts, links, resolved))
    }
    ExportState::ValueBag(bag) => {
      ExportState::ValueBag(refine_value_bag(bag, module_id, locals, facts, links, resolved))
    }
    ExportState::Composable(shape) => ExportState::Composable(refine_composable_shape(
      shape, module_id, locals, facts, links, resolved,
    )),
    // Keep the call marker so each export publish re-snapshots the callee bag
    // (avoid sticky MethodForward clones after the factory later refines).
    ExportState::ValueFactoryCall(callee) => ExportState::ValueFactoryCall(callee),
    ExportState::GenericMethodInstantiate { callee, property, type_arg_shapes } => {
      let callee_state =
        resolve_name_export_state(module_id, &callee, locals, facts, links, resolved, 0);
      let keep = ExportState::GenericMethodInstantiate {
        callee,
        property: property.clone(),
        type_arg_shapes: type_arg_shapes.clone(),
      };
      export_lattice::refine_generic_method_instantiate(
        callee_state.as_ref(),
        &property,
        &type_arg_shapes,
        keep,
      )
    }
    other => other,
  }
}

fn refine_composable_shape(
  shape: ComposableShape,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> ComposableShape {
  export_lattice::refine_composable_pending(shape, |root| {
    resolve_name_export_state(module_id, root, locals, facts, links, resolved, 0)
  })
}

/// Materialize [`ExportState::ValueFactoryCall`] against current resolved exports.
fn publishable_export_state(
  state: &ExportState,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> Option<ExportState> {
  let materialized = match state {
    ExportState::ValueFactoryCall(callee) => {
      let callee_state =
        resolve_name_export_state(module_id, callee, locals, facts, links, resolved, 0);
      export_lattice::value_factory_call_bag(callee_state.as_ref()).map(|bag| {
        ExportState::ValueBag(refine_value_bag(
          bag.clone(),
          module_id,
          locals,
          facts,
          links,
          resolved,
        ))
      })
    }
    _ => None,
  };
  export_lattice::as_publishable(state, materialized)
}

fn refine_value_bag(
  bag: ValueBag,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> ValueBag {
  export_lattice::refine_value_bag(bag, |name| {
    resolve_name_export_state(module_id, name, locals, facts, links, resolved, 0)
  })
}

fn resolve_name_export_state(
  module_id: &ModuleId,
  name: &str,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  depth: u8,
) -> Option<ExportState> {
  // Pure order: locals → ES import → bare `#nuxt-imports:{name}` (PCR Name resolve).
  let imports: Vec<export_lattice::ImportBindingView<'_>> = facts
    .get(module_id)
    .map(|module_facts| {
      module_facts
        .summary
        .imports
        .iter()
        .map(|import| export_lattice::ImportBindingView {
          local: import.local.as_str(),
          source: import.source.as_str(),
          imported: import.imported.as_str(),
        })
        .collect()
    })
    .unwrap_or_default();
  export_lattice::resolve_name_export_state(
    name,
    locals,
    &imports,
    |specifier| links.get(&(module_id, specifier)).copied().cloned(),
    |target, export_name| resolved.get(target)?.get(export_name).cloned(),
    depth,
  )
}

/// Borrowed index over owned resolved links — avoids re-allocating key pairs on lookup.
pub(super) fn link_index(
  links: &BTreeMap<(ModuleId, String), ModuleId>,
) -> BTreeMap<(&ModuleId, &str), &ModuleId> {
  links.iter().map(|((from, specifier), to)| ((from, specifier.as_str()), to)).collect()
}

fn insert_export(
  resolved: &mut BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  module: &ModuleId,
  exported: &str,
  state: ExportState,
) -> bool {
  // Provisional / unresolved halves never cross the seed barrier alone.
  if !export_lattice::is_seedable(&state) {
    return false;
  }
  // Publish value bags even when some MethodForward entries remain. Waiting for
  // *every* forward (e.g. `useMutation` next to resolved `useQuery`) blocked the
  // whole factory export; unresolved methods stay quiet at `resolve_path`.
  let Some(module_exports) = resolved.get_mut(module) else {
    return false;
  };
  match module_exports.entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(state);
      true
    }
    Entry::Occupied(mut entry) => match export_lattice::merge_published(entry.get(), &state) {
      export_lattice::PublishMerge::Unchanged => false,
      export_lattice::PublishMerge::Replace => {
        entry.insert(state);
        true
      }
      export_lattice::PublishMerge::Ambiguous => {
        entry.insert(ExportState::Ambiguous);
        true
      }
    },
  }
}

/// Propagate options-callback slots through `export { x } from` / `export *` barrels.
///
/// Independent of [`resolve_exports`]: a `declare function` may publish callback bags
/// without a seedable return [`ExportState`].
pub(super) fn resolve_options_callback_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, OptionsCallbackSlots>> {
  use std::collections::VecDeque;

  // Empty-slot graphs (typical synthetic / re-export benches) must not pay a
  // full barrel fixpoint that queues every `export { … } from`.
  if !facts
    .values()
    .any(|module| module.summary.options_callback_slots.values().any(|slots| !slots.is_empty()))
  {
    return BTreeMap::new();
  }

  let mut resolved: BTreeMap<ModuleId, BTreeMap<String, OptionsCallbackSlots>> = BTreeMap::new();

  for (id, module_facts) in facts {
    for (name, slots) in &module_facts.summary.options_callback_slots {
      if slots.is_empty() {
        continue;
      }
      resolved.entry(id.clone()).or_default().insert(name.clone(), slots.clone());
    }
    for export in &module_facts.summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if local == exported {
        continue;
      }
      if let Some(slots) = module_facts.summary.options_callback_slots.get(local)
        && !slots.is_empty()
      {
        resolved.entry(id.clone()).or_default().insert(exported.clone(), slots.clone());
      }
    }
  }

  let mut reverse_users: BTreeMap<&ModuleId, Vec<&ModuleId>> = BTreeMap::new();
  for ((from, _), to) in links {
    reverse_users.entry(*to).or_default().push(*from);
  }

  let mut queue = VecDeque::new();
  let mut queued = BTreeSet::new();
  for (id, module_facts) in facts {
    let needs_reexport =
      module_facts.summary.exports.iter().any(|export| {
        matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. })
      });
    let needs_import_local_export = module_facts.summary.exports.iter().any(|export| {
      matches!(
        export,
        ExportSummary::Local { local, .. }
          if !module_facts.summary.locals.contains_key(local)
            && !module_facts.summary.options_callback_slots.contains_key(local)
            && module_facts.summary.imports.iter().any(|import| import.local == *local)
      )
    });
    if needs_reexport || needs_import_local_export {
      queue.push_back(id);
      queued.insert(id);
    }
  }

  while let Some(id) = queue.pop_front() {
    queued.remove(id);
    let Some(module_facts) = facts.get(id) else {
      continue;
    };
    let mut changed = false;
    for export in &module_facts.summary.exports {
      match export {
        ExportSummary::Local { local, exported } => {
          if module_facts.summary.options_callback_slots.contains_key(local)
            || module_facts.summary.locals.contains_key(local)
          {
            continue;
          }
          let Some(import) =
            module_facts.summary.imports.iter().find(|import| import.local == *local)
          else {
            continue;
          };
          let Some(target) = links.get(&(id, import.source.as_str())).copied() else {
            continue;
          };
          let Some(slots) =
            resolved.get(target).and_then(|exports| exports.get(&import.imported)).cloned()
          else {
            continue;
          };
          changed |= insert_options_callback_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Reexport { source, imported, exported } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(slots) = resolved.get(target).and_then(|exports| exports.get(imported)).cloned()
          else {
            continue;
          };
          changed |= insert_options_callback_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Star { source } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(target_slots) = resolved.get(target).cloned() else {
            continue;
          };
          for (exported, slots) in target_slots {
            if exported != "default" {
              changed |= insert_options_callback_export(&mut resolved, id, &exported, slots);
            }
          }
        }
      }
    }
    if !changed {
      continue;
    }
    if let Some(users) = reverse_users.get(id) {
      for consumer in users {
        if queued.insert(consumer) {
          queue.push_back(consumer);
        }
      }
    }
  }

  resolved
}

fn insert_options_callback_export(
  resolved: &mut BTreeMap<ModuleId, BTreeMap<String, OptionsCallbackSlots>>,
  module: &ModuleId,
  exported: &str,
  slots: OptionsCallbackSlots,
) -> bool {
  if slots.is_empty() {
    return false;
  }
  // Barrel-only modules are not pre-seeded; create on first insert.
  match resolved.entry(module.clone()).or_default().entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(mut entry) if entry.get() != &slots => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(_) => false,
  }
}

/// Propagate typed function-callback Ref formals through barrels (same as options slots).
pub(super) fn resolve_typed_callback_param_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>> {
  use std::collections::VecDeque;

  if !facts
    .values()
    .any(|module| module.summary.typed_callback_param_slots.values().any(|slots| !slots.is_empty()))
  {
    return BTreeMap::new();
  }

  let mut resolved: BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>> = BTreeMap::new();

  for (id, module_facts) in facts {
    for (name, slots) in &module_facts.summary.typed_callback_param_slots {
      if slots.is_empty() {
        continue;
      }
      resolved.entry(id.clone()).or_default().insert(name.clone(), slots.clone());
    }
    for export in &module_facts.summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if local == exported {
        continue;
      }
      if let Some(slots) = module_facts.summary.typed_callback_param_slots.get(local)
        && !slots.is_empty()
      {
        resolved.entry(id.clone()).or_default().insert(exported.clone(), slots.clone());
      }
    }
  }

  let mut reverse_users: BTreeMap<&ModuleId, Vec<&ModuleId>> = BTreeMap::new();
  for ((from, _), to) in links {
    reverse_users.entry(*to).or_default().push(*from);
  }

  let mut queue = VecDeque::new();
  let mut queued = BTreeSet::new();
  for (id, module_facts) in facts {
    let needs_reexport =
      module_facts.summary.exports.iter().any(|export| {
        matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. })
      });
    let needs_import_local_export = module_facts.summary.exports.iter().any(|export| {
      matches!(
        export,
        ExportSummary::Local { local, .. }
          if !module_facts.summary.locals.contains_key(local)
            && !module_facts.summary.typed_callback_param_slots.contains_key(local)
            && module_facts.summary.imports.iter().any(|import| import.local == *local)
      )
    });
    if needs_reexport || needs_import_local_export {
      queue.push_back(id);
      queued.insert(id);
    }
  }

  while let Some(id) = queue.pop_front() {
    queued.remove(id);
    let Some(module_facts) = facts.get(id) else {
      continue;
    };
    let mut changed = false;
    for export in &module_facts.summary.exports {
      match export {
        ExportSummary::Local { local, exported } => {
          if module_facts.summary.typed_callback_param_slots.contains_key(local)
            || module_facts.summary.locals.contains_key(local)
          {
            continue;
          }
          let Some(import) =
            module_facts.summary.imports.iter().find(|import| import.local == *local)
          else {
            continue;
          };
          let Some(target) = links.get(&(id, import.source.as_str())).copied() else {
            continue;
          };
          let Some(slots) =
            resolved.get(target).and_then(|exports| exports.get(&import.imported)).cloned()
          else {
            continue;
          };
          changed |= insert_typed_callback_param_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Reexport { source, imported, exported } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(slots) = resolved.get(target).and_then(|exports| exports.get(imported)).cloned()
          else {
            continue;
          };
          changed |= insert_typed_callback_param_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Star { source } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(target_slots) = resolved.get(target).cloned() else {
            continue;
          };
          for (exported, slots) in target_slots {
            if exported != "default" {
              changed |= insert_typed_callback_param_export(&mut resolved, id, &exported, slots);
            }
          }
        }
      }
    }
    if !changed {
      continue;
    }
    if let Some(users) = reverse_users.get(id) {
      for consumer in users {
        if queued.insert(consumer) {
          queue.push_back(consumer);
        }
      }
    }
  }

  resolved
}

fn insert_typed_callback_param_export(
  resolved: &mut BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>>,
  module: &ModuleId,
  exported: &str,
  slots: TypedCallbackParamSlots,
) -> bool {
  if slots.is_empty() {
    return false;
  }
  match resolved.entry(module.clone()).or_default().entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(mut entry) if entry.get() != &slots => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(_) => false,
  }
}
