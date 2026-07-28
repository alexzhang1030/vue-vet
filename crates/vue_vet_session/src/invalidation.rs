use std::collections::{BTreeMap, BTreeSet};

use vue_vet_core::FileId;
use vue_vet_project::ProjectGraph;

pub fn reverse_dependency_index(graph: &ProjectGraph) -> BTreeMap<FileId, BTreeSet<FileId>> {
  let mut reverse = BTreeMap::<FileId, BTreeSet<FileId>>::new();
  for edge in &graph.edges {
    let Some(from) = edge.from.strip_prefix("file:") else {
      continue;
    };
    let Some(to) = edge.to.strip_prefix("file:") else {
      continue;
    };
    reverse.entry(FileId::from(to)).or_default().insert(FileId::from(from));
  }
  reverse
}

pub fn expand_reverse_dependencies(
  affected: &mut BTreeSet<FileId>,
  reverse: &BTreeMap<FileId, BTreeSet<FileId>>,
) {
  let mut pending = affected.iter().cloned().collect::<Vec<_>>();
  while let Some(file) = pending.pop() {
    let Some(dependents) = reverse.get(&file) else {
      continue;
    };
    for dependent in dependents {
      if affected.insert(dependent.clone()) {
        pending.push(dependent.clone());
      }
    }
  }
}
