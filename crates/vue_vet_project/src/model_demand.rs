//! Project join for component model-default demand owners.
//!
//! Indexes uses by child path, models by `(file, name)`, and instance
//! writes/demands by path so joins never scan all instance pairs.

use std::collections::BTreeMap;

use vue_vet_core::{
  ModelDefaultOrigin, ModelDemandFileFacts, ModelPrimitiveKind, RefInitKind, ScriptKind,
  SharedDefaultCrossInstanceDemandFact, SourceSpan, TemplateElementFact,
  UnsyncedModelParentDemandFact,
};

use crate::model::{EdgeKind, GraphEdge, ProjectFile};

const EMPTY_WRITES: &[vue_vet_core::InstancePathWriteFact] = &[];
const EMPTY_DEMANDS: &[vue_vet_core::InstanceMemberDemandFact] = &[];

/// Adapter-only join counters. Production storage is ZST.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelDemandStats {
  pub uses: u64,
  pub models: u64,
  pub path_lookups: u64,
  pub joins: u64,
}

impl ModelDemandStats {
  #[cfg(test)]
  #[must_use]
  pub const fn work(self) -> u64 {
    self
      .uses
      .saturating_add(self.models)
      .saturating_add(self.path_lookups)
      .saturating_add(self.joins)
  }
}

#[derive(Default)]
struct Counter {
  #[cfg(test)]
  uses: std::cell::Cell<u64>,
  #[cfg(test)]
  models: std::cell::Cell<u64>,
  #[cfg(test)]
  path_lookups: std::cell::Cell<u64>,
  #[cfg(test)]
  joins: std::cell::Cell<u64>,
}

impl Counter {
  #[cfg(test)]
  fn add_uses(&self, n: u64) {
    self.uses.set(self.uses.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_uses(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  fn add_models(&self, n: u64) {
    self.models.set(self.models.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_models(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  fn add_lookups(&self, n: u64) {
    self.path_lookups.set(self.path_lookups.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_lookups(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  fn add_joins(&self, n: u64) {
    self.joins.set(self.joins.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_joins(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  const fn snapshot(&self) -> ModelDemandStats {
    ModelDemandStats {
      uses: self.uses.get(),
      models: self.models.get(),
      path_lookups: self.path_lookups.get(),
      joins: self.joins.get(),
    }
  }

  #[cfg(not(test))]
  #[expect(clippy::unused_self, reason = "production snapshot is always zero")]
  const fn snapshot(&self) -> ModelDemandStats {
    ModelDemandStats { uses: 0, models: 0, path_lookups: 0, joins: 0 }
  }
}

struct UseSite<'a> {
  parent: &'a ProjectFile,
  child: &'a ProjectFile,
  element: &'a TemplateElementFact,
  model_directive: Option<&'a vue_vet_core::TemplateDirectiveFact>,
}

/// Join parent/child model facts. Findings are stored per parent file.
#[must_use]
pub fn join_model_demand_facts(
  files: &[&ProjectFile],
  edges: &[GraphEdge],
) -> (BTreeMap<String, ModelDemandFileFacts>, ModelDemandStats) {
  let work = Counter::default();
  let files_by_path: BTreeMap<&str, &ProjectFile> =
    files.iter().map(|file| (file.path.as_str(), *file)).collect();
  let mut uses_by_child: BTreeMap<&str, Vec<UseSite<'_>>> = BTreeMap::new();
  for edge in edges {
    work.add_uses(1);
    if !matches!(edge.kind, EdgeKind::ComponentUsage | EdgeKind::AutoComponent) {
      continue;
    }
    let parent_path = edge.from.strip_prefix("file:").unwrap_or(edge.from.as_str());
    let child_path = edge.to.strip_prefix("file:").unwrap_or(edge.to.as_str());
    let Some(parent) = files_by_path.get(parent_path).copied() else {
      continue;
    };
    let Some(child) = files_by_path.get(child_path).copied() else {
      continue;
    };
    work.add_lookups(1);
    let Some(element) = parent
      .facts
      .template
      .elements
      .iter()
      .find(|element| element.span.offset == edge.evidence.offset)
    else {
      continue;
    };
    let model_directive = element.directive("model");
    uses_by_child.entry(child_path).or_default().push(UseSite {
      parent,
      child,
      element,
      model_directive,
    });
  }

  let mut findings: BTreeMap<String, ModelDemandFileFacts> = BTreeMap::new();
  join_unsynced_parent(&uses_by_child, &work, &mut findings);
  join_shared_instances(&uses_by_child, &work, &mut findings);
  for facts in findings.values_mut() {
    facts.unsynced_parent_demands.sort_by_key(|fact| fact.demand_span.offset);
    facts.shared_cross_instance_demands.sort_by_key(|fact| fact.demand_span.offset);
  }
  (findings, work.snapshot())
}

fn join_unsynced_parent(
  uses_by_child: &BTreeMap<&str, Vec<UseSite<'_>>>,
  work: &Counter,
  findings: &mut BTreeMap<String, ModelDemandFileFacts>,
) {
  for uses in uses_by_child.values() {
    for site in uses {
      work.add_joins(1);
      let Some(model) = site.model_directive else {
        continue;
      };
      if !site.element.is_unconditional_mount() {
        continue;
      }
      if !model.modifiers.is_empty() {
        continue;
      }
      let model_name =
        model.argument.as_deref().filter(|name| !name.is_empty()).unwrap_or("modelValue");
      let Some(expression) = model.expression.as_deref().map(str::trim) else {
        continue;
      };
      if !is_simple_ident(expression) {
        continue;
      }
      let Some(default) = child_model(site.child, model_name, work) else {
        continue;
      };
      if default.origin != ModelDefaultOrigin::LiteralPrimitive
        || default.has_get_set
        || default.escaped
      {
        continue;
      }
      let Some(primitive) = default.primitive else {
        continue;
      };
      let Some(init) = parent_ref(site.parent, expression, work) else {
        continue;
      };
      if init.escaped || !init.kind.is_undefined() {
        continue;
      }
      if parent_wrote_before_mount(site.parent, expression, work) {
        continue;
      }
      if child_emitted_change(site.child, work) {
        continue;
      }
      let Some(demand) = mounted_demand(site.parent, expression, work) else {
        continue;
      };
      if demand.optional || demand.guarded {
        continue;
      }
      if !primitive.supplies(&demand.member) || init.kind.supplies(&demand.member) {
        continue;
      }
      let parent_facts = findings.entry(site.parent.path.as_str().to_owned()).or_default();
      if parent_facts
        .unsynced_parent_demands
        .iter()
        .any(|existing| existing.demand_span.offset == demand.span.offset)
      {
        continue;
      }
      parent_facts.unsynced_parent_demands.push(UnsyncedModelParentDemandFact {
        demand_span: demand.span,
        parent_version_span: init.span,
        v_model_span: model.span,
        child_default_span: default.span,
        child_file: site.child.path.as_str().to_owned(),
        model_name: model_name.to_owned(),
        parent_binding: expression.to_owned(),
        demanded_member: demand.member.clone(),
      });
    }
  }
}

fn join_shared_instances(
  uses_by_child: &BTreeMap<&str, Vec<UseSite<'_>>>,
  work: &Counter,
  findings: &mut BTreeMap<String, ModelDemandFileFacts>,
) {
  for uses in uses_by_child.values() {
    let mut live: Vec<&UseSite<'_>> =
      uses.iter().filter(|site| site.element.is_unconditional_mount()).collect();
    live.sort_by_key(|site| (site.parent.path.as_str(), site.element.span.offset));
    if live.len() < 2 {
      continue;
    }
    // Same parent, two live uses, proven shared default.
    let mut by_parent: BTreeMap<&str, Vec<&UseSite<'_>>> = BTreeMap::new();
    for site in live {
      by_parent.entry(site.parent.path.as_str()).or_default().push(site);
    }
    for parent_uses in by_parent.values() {
      if parent_uses.len() < 2 {
        continue;
      }
      let Some(first) = parent_uses.first() else {
        continue;
      };
      let parent = first.parent;
      let child = first.child;
      let Some(default) = child_shared_default(child, work) else {
        continue;
      };
      if default.escaped {
        continue;
      }
      let Some(own_paths) = shared_own_paths(child, default, work) else {
        continue;
      };
      let expose = child_expose(child, work);
      let mut writes: BTreeMap<String, Vec<(&UseSite<'_>, SourceSpan, RefInitKind)>> =
        BTreeMap::new();
      let mut demands: BTreeMap<String, Vec<(&UseSite<'_>, SourceSpan, String)>> = BTreeMap::new();
      for site in parent_uses {
        if site_binds_model(site, &default.model_name) {
          continue;
        }
        let Some(instance) = site.element.static_ref_name() else {
          continue;
        };
        if !instance_ref_ok(parent, instance, work) {
          continue;
        }
        work.add_lookups(1);
        for write in parent_writes(parent, instance, work)
          .iter()
          .filter(|write| instance_matches_write(write, instance))
        {
          if write.path.is_empty() {
            continue;
          }
          let Some((head, rest)) = write.path.split_first() else {
            continue;
          };
          if !expose_has(&expose, head) {
            continue;
          }
          let path = rest.join(".");
          writes.entry(path).or_default().push((site, write.span, write.rhs_kind));
        }
        for demand in parent_instance_demands(parent, instance, work)
          .iter()
          .filter(|demand| instance_matches_demand(demand, instance))
        {
          if demand.optional || demand.guarded || demand.path.is_empty() {
            continue;
          }
          let Some((head, rest)) = demand.path.split_first() else {
            continue;
          };
          if !expose_has(&expose, head) {
            continue;
          }
          let path = rest.join(".");
          demands.entry(path).or_default().push((site, demand.span, demand.member.clone()));
        }
      }
      for (path, path_writes) in &writes {
        work.add_lookups(1);
        let Some(original) = own_paths.iter().find(|(name, _)| name == path).map(|(_, kind)| *kind)
        else {
          continue;
        };
        let Some(path_demands) = demands.get(path) else {
          continue;
        };
        for (write_site, write_span, rhs) in path_writes {
          if !rhs_changes(original, *rhs) {
            continue;
          }
          for (demand_site, demand_span, member) in path_demands {
            work.add_joins(1);
            if std::ptr::eq(*write_site, *demand_site) {
              continue;
            }
            if write_site.parent.path != demand_site.parent.path {
              continue;
            }
            if !original.supplies(member) || rhs.supplies(member) {
              continue;
            }
            let parent_facts = findings.entry(parent.path.as_str().to_owned()).or_default();
            if parent_facts
              .shared_cross_instance_demands
              .iter()
              .any(|existing| existing.demand_span.offset == demand_span.offset)
            {
              continue;
            }
            parent_facts.shared_cross_instance_demands.push(SharedDefaultCrossInstanceDemandFact {
              demand_span: *demand_span,
              default_span: default.span,
              instance_a_span: write_site.element.span,
              instance_b_span: demand_site.element.span,
              write_span: *write_span,
              child_file: child.path.as_str().to_owned(),
              path: path.clone(),
              demanded_member: member.clone(),
            });
          }
        }
      }
    }
  }
}

fn child_model<'a>(
  child: &'a ProjectFile,
  name: &str,
  work: &Counter,
) -> Option<&'a vue_vet_core::ModelDefaultFact> {
  work.add_models(1);
  setup_block(child)?.source_contracts.model_defaults.iter().find(|model| model.model_name == name)
}

fn child_shared_default<'a>(
  child: &'a ProjectFile,
  work: &Counter,
) -> Option<&'a vue_vet_core::ModelDefaultFact> {
  work.add_models(1);
  setup_block(child)?.source_contracts.model_defaults.iter().find(|model| {
    matches!(
      model.origin,
      ModelDefaultOrigin::SharedObjectFactory | ModelDefaultOrigin::SharedObjectLiteral
    ) && !model.has_get_set
  })
}

fn shared_own_paths(
  child: &ProjectFile,
  default: &vue_vet_core::ModelDefaultFact,
  work: &Counter,
) -> Option<Vec<(String, vue_vet_core::ModelPrimitiveKind)>> {
  if !default.own_paths.is_empty() {
    return Some(default.own_paths.clone());
  }
  let shared = shared_object(child, default.shared_binding.as_deref(), work)?;
  if shared.escaped {
    return None;
  }
  Some(shared.own_paths.clone())
}

fn shared_object<'a>(
  child: &'a ProjectFile,
  name: Option<&str>,
  work: &Counter,
) -> Option<&'a vue_vet_core::SharedObjectBindingFact> {
  let name = name?;
  work.add_lookups(1);
  child.facts.script.blocks.iter().find_map(|block| {
    block.source_contracts.shared_object_bindings.iter().find(|binding| binding.binding == name)
  })
}

fn site_binds_model(site: &UseSite<'_>, model_name: &str) -> bool {
  let Some(model) = site.model_directive else {
    return false;
  };
  let bound = model.argument.as_deref().filter(|name| !name.is_empty()).unwrap_or("modelValue");
  bound == model_name
}

fn child_expose(child: &ProjectFile, work: &Counter) -> Vec<String> {
  work.add_lookups(1);
  setup_block(child)
    .and_then(|block| block.source_contracts.define_expose.first())
    .map(|expose| expose.names.clone())
    .unwrap_or_default()
}

fn expose_has(names: &[String], head: &str) -> bool {
  names.iter().any(|name| name == head)
}

fn parent_ref<'a>(
  parent: &'a ProjectFile,
  name: &str,
  work: &Counter,
) -> Option<&'a vue_vet_core::OrdinaryRefInitFact> {
  work.add_lookups(1);
  setup_block(parent)?.source_contracts.ordinary_ref_inits.iter().find(|init| init.binding == name)
}

fn mounted_demand<'a>(
  parent: &'a ProjectFile,
  name: &str,
  work: &Counter,
) -> Option<&'a vue_vet_core::MountedMemberDemandFact> {
  work.add_lookups(1);
  setup_block(parent)?
    .source_contracts
    .mounted_member_demands
    .iter()
    .find(|demand| demand.receiver == name)
}

fn parent_wrote_before_mount(parent: &ProjectFile, name: &str, work: &Counter) -> bool {
  work.add_lookups(1);
  let Some(block) = setup_block(parent) else {
    return true;
  };
  block.source_contracts.ordinary_ref_inits.iter().any(|init| {
    init.binding == name && !matches!(init.kind, RefInitKind::Undefined | RefInitKind::Unknown)
  }) || block.member_writes.iter().any(|write| write.object == name)
    || block
      .source_contracts
      .model_value_writes
      .iter()
      .any(|write| write.binding == name && !write.unchanged_default)
}

fn child_emitted_change(child: &ProjectFile, work: &Counter) -> bool {
  work.add_lookups(1);
  setup_block(child).is_some_and(|block| {
    block.source_contracts.model_value_writes.iter().any(|write| !write.unchanged_default)
  })
}

fn instance_ref_ok(parent: &ProjectFile, name: &str, work: &Counter) -> bool {
  work.add_lookups(1);
  parent_ref(parent, name, work).is_some_and(|init| {
    matches!(init.kind, RefInitKind::Null | RefInitKind::Undefined) && !init.escaped
  })
}

fn parent_writes<'a>(
  parent: &'a ProjectFile,
  instance: &str,
  work: &Counter,
) -> &'a [vue_vet_core::InstancePathWriteFact] {
  work.add_lookups(1);
  setup_block(parent).map_or(EMPTY_WRITES, |block| {
    // Filter is done by callers via instance name; return all and filter there
    // would allocate. Index by instance in a small borrowed scan.
    let _ = instance;
    block.source_contracts.instance_path_writes.as_slice()
  })
}

fn parent_instance_demands<'a>(
  parent: &'a ProjectFile,
  instance: &str,
  work: &Counter,
) -> &'a [vue_vet_core::InstanceMemberDemandFact] {
  work.add_lookups(1);
  let _ = instance;
  setup_block(parent)
    .map_or(EMPTY_DEMANDS, |block| block.source_contracts.instance_member_demands.as_slice())
}

fn setup_block(file: &ProjectFile) -> Option<&vue_vet_core::ScriptBlockFacts> {
  file
    .facts
    .script
    .blocks
    .iter()
    .find(|block| block.kind == ScriptKind::Setup)
    .or_else(|| file.facts.script.blocks.first())
}

const fn rhs_changes(original: ModelPrimitiveKind, rhs: RefInitKind) -> bool {
  !matches!(
    (original, rhs),
    (ModelPrimitiveKind::Number, RefInitKind::Number)
      | (ModelPrimitiveKind::String, RefInitKind::String)
      | (ModelPrimitiveKind::Boolean, RefInitKind::Boolean)
      | (_, RefInitKind::Unknown)
  )
}

fn is_simple_ident(value: &str) -> bool {
  let mut chars = value.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  (first.is_alphabetic() || first == '_' || first == '$')
    && chars.all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '$')
}

fn instance_matches_write(write: &vue_vet_core::InstancePathWriteFact, instance: &str) -> bool {
  write.instance == instance
}

fn instance_matches_demand(
  demand: &vue_vet_core::InstanceMemberDemandFact,
  instance: &str,
) -> bool {
  demand.instance == instance
}
