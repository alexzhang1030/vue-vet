//! Link-time fixed points over resolved module links.
//!
//! [`resolve_exports`] refines `ExportState` through barrels, forwards, and value
//! bags; the callback-slot resolvers propagate declared callback shapes through
//! the same barrel edges. Pure lattice rules live in [`super::super::export_lattice`].

use std::collections::{BTreeMap, BTreeSet, VecDeque, btree_map::Entry};

use vue_vet_core::ModuleId;

use super::super::export_lattice::NAME_RESOLVE_MAX_DEPTH;

use super::super::{
  ComposableShape, ExportState, ExportSummary, ModuleExportFacts, ModuleLink, ModuleSummary,
  OptionsCallbackSlots, TraceModulesError, TypedCallbackParamSlots, ValueBag, export_lattice,
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

/// One visit of a module inside [`barrel_fixpoint`].
#[derive(Clone, Copy, Default)]
struct Visit {
  /// The module's published surface changed — its consumers are re-queued.
  changed: bool,
  /// A local refine may unlock more of the module's own locals — re-queue it too.
  revisit_self: bool,
}

/// Monotone worklist over the reverse link graph (target → consumers).
///
/// `seeds` is the initial queue; `visit` processes one module and reports
/// whether its published surface changed. Consumers of a changed module are
/// re-queued; the module itself only when `revisit_self` is set. Every module
/// is visited at most `NAME_RESOLVE_MAX_DEPTH + 1` times — the same convention
/// as name resolve (visit index starts at 0, `> NAME_RESOLVE_MAX_DEPTH` stops) —
/// so a non-monotone step cannot spin. Publishes are monotone, so the cap is a
/// safety net, not a tuning knob.
fn barrel_fixpoint<'f>(
  facts: &'f BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&'f ModuleId, &str), &'f ModuleId>,
  seeds: Vec<&'f ModuleId>,
  mut visit: impl FnMut(&ModuleId, &ModuleExportFacts) -> Visit,
) {
  let mut reverse_users: BTreeMap<&ModuleId, Vec<&ModuleId>> = BTreeMap::new();
  for ((from, _), to) in links {
    reverse_users.entry(*to).or_default().push(*from);
  }
  let mut queued: BTreeSet<&ModuleId> = seeds.iter().copied().collect();
  let mut queue: VecDeque<&ModuleId> = seeds.into_iter().collect();
  let mut visits: BTreeMap<&ModuleId, u8> = BTreeMap::new();
  while let Some(id) = queue.pop_front() {
    queued.remove(id);
    let Some(module_facts) = facts.get(id) else {
      continue;
    };
    let depth = visits.entry(id).or_insert(0);
    if *depth > NAME_RESOLVE_MAX_DEPTH {
      continue;
    }
    *depth = depth.saturating_add(1);
    let outcome = visit(id, module_facts);
    if !outcome.changed {
      continue;
    }
    if let Some(users) = reverse_users.get(id) {
      for consumer in users {
        if queued.insert(consumer) {
          queue.push_back(consumer);
        }
      }
    }
    if outcome.revisit_self && queued.insert(id) {
      queue.push_back(id);
    }
  }
}

/// Barrel modules that must enter the fixed point even without local state:
/// `export { x } from` / `export *`, or `import { d as x }; export { x }` where
/// the local name is neither in `locals` nor already a known slot.
fn needs_barrel_visit(
  module_facts: &ModuleExportFacts,
  known_local: impl Fn(&str) -> bool,
) -> bool {
  let summary = &module_facts.summary;
  let needs_reexport = summary
    .exports
    .iter()
    .any(|export| matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. }));
  let needs_import_local_export = summary.exports.iter().any(|export| {
    matches!(
      export,
      ExportSummary::Local { local, .. }
        if !known_local(local) && summary.imports.iter().any(|import| import.local == *local)
    )
  });
  needs_reexport || needs_import_local_export
}

pub(super) fn resolve_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, ExportState>> {
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

  let seeds = facts
    .iter()
    .filter(|(id, module_facts)| {
      working_locals.contains_key(*id)
        || needs_barrel_visit(module_facts, |local| module_facts.summary.locals.contains_key(local))
    })
    .map(|(id, _)| id)
    .collect();

  barrel_fixpoint(facts, links, seeds, |id, module_facts| {
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
    // Only re-enter when a forward/value-bag refine may unlock more locals.
    Visit { changed, revisit_self: refined_forward && working_locals.contains_key(id) }
  });

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
  resolve_slot_exports(
    facts,
    links,
    |summary| &summary.options_callback_slots,
    OptionsCallbackSlots::is_empty,
  )
}

/// Propagate typed function-callback Ref formals through barrels (same as options slots).
pub(super) fn resolve_typed_callback_param_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>> {
  resolve_slot_exports(
    facts,
    links,
    |summary| &summary.typed_callback_param_slots,
    TypedCallbackParamSlots::is_empty,
  )
}

/// Barrel fixed point for one declared-slot family (`slots_of` picks the field).
///
/// `resolved` only ever holds non-empty bags: pre-seeding filters with
/// `is_empty`, and the fixed point copies bags between `resolved` entries.
fn resolve_slot_exports<S: Clone + PartialEq>(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  slots_of: impl Fn(&ModuleSummary) -> &BTreeMap<String, S>,
  is_empty: impl Fn(&S) -> bool,
) -> BTreeMap<ModuleId, BTreeMap<String, S>> {
  // Empty-slot graphs (typical synthetic / re-export benches) must not pay a
  // full barrel fixpoint that queues every `export { … } from`.
  if !facts.values().any(|module| slots_of(&module.summary).values().any(|slots| !is_empty(slots)))
  {
    return BTreeMap::new();
  }

  let mut resolved: BTreeMap<ModuleId, BTreeMap<String, S>> = BTreeMap::new();

  for (id, module_facts) in facts {
    let slots = slots_of(&module_facts.summary);
    for (name, bag) in slots {
      if is_empty(bag) {
        continue;
      }
      resolved.entry(id.clone()).or_default().insert(name.clone(), bag.clone());
    }
    for export in &module_facts.summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if local == exported {
        continue;
      }
      if let Some(bag) = slots.get(local)
        && !is_empty(bag)
      {
        resolved.entry(id.clone()).or_default().insert(exported.clone(), bag.clone());
      }
    }
  }

  let seeds = facts
    .iter()
    .filter(|(_, module_facts)| {
      needs_barrel_visit(module_facts, |local| {
        module_facts.summary.locals.contains_key(local)
          || slots_of(&module_facts.summary).contains_key(local)
      })
    })
    .map(|(id, _)| id)
    .collect();

  barrel_fixpoint(facts, links, seeds, |id, module_facts| {
    let slots = slots_of(&module_facts.summary);
    let mut changed = false;
    for export in &module_facts.summary.exports {
      match export {
        ExportSummary::Local { local, exported } => {
          if slots.contains_key(local) || module_facts.summary.locals.contains_key(local) {
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
          let Some(bag) =
            resolved.get(target).and_then(|exports| exports.get(&import.imported)).cloned()
          else {
            continue;
          };
          changed |= insert_slot_export(&mut resolved, id, exported, bag);
        }
        ExportSummary::Reexport { source, imported, exported } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(bag) = resolved.get(target).and_then(|exports| exports.get(imported)).cloned()
          else {
            continue;
          };
          changed |= insert_slot_export(&mut resolved, id, exported, bag);
        }
        ExportSummary::Star { source } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(target_slots) = resolved.get(target).cloned() else {
            continue;
          };
          for (exported, bag) in target_slots {
            if exported != "default" {
              changed |= insert_slot_export(&mut resolved, id, &exported, bag);
            }
          }
        }
      }
    }
    Visit { changed, revisit_self: false }
  });

  resolved
}

fn insert_slot_export<S: PartialEq>(
  resolved: &mut BTreeMap<ModuleId, BTreeMap<String, S>>,
  module: &ModuleId,
  exported: &str,
  slots: S,
) -> bool {
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
