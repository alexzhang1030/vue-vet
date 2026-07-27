use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

use vue_vet_core::{RuleEnvironment, VueVersion};

/// Parsed package environments keyed by their containing directory.
///
/// Discovery owns filesystem I/O. Per-file analysis only performs ancestor
/// lookups against this immutable index.
#[derive(Clone, Debug, Default)]
pub struct PackageIndex {
  environments_by_directory: BTreeMap<PathBuf, RuleEnvironment>,
}

impl PackageIndex {
  pub fn insert(&mut self, package_path: &Path, source: &str) {
    let Some(directory) = package_path.parent() else {
      return;
    };
    let Ok(package) = serde_json::from_str::<serde_json::Value>(source) else {
      return;
    };
    self
      .environments_by_directory
      .insert(directory.to_path_buf(), environment_from_package(&package));
  }

  pub fn environment_for(&self, path: &Path, boundary: &Path) -> RuleEnvironment {
    let Some(mut directory) = path.parent() else {
      return RuleEnvironment::default();
    };
    loop {
      if !directory.starts_with(boundary) {
        return RuleEnvironment::default();
      }
      if let Some(environment) = self.environments_by_directory.get(directory) {
        return environment.clone();
      }
      if directory == boundary {
        return RuleEnvironment::default();
      }
      let Some(parent) = directory.parent() else {
        return RuleEnvironment::default();
      };
      directory = parent;
    }
  }
}

fn environment_from_package(package: &serde_json::Value) -> RuleEnvironment {
  let packages = dependency_package_names(package);
  let vue_version = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]
    .iter()
    .filter_map(|section| package.get(*section))
    .filter_map(|section| section.get("vue"))
    .filter_map(serde_json::Value::as_str)
    .find_map(VueVersion::parse_requirement);
  RuleEnvironment { vue_version, packages }
}

fn dependency_package_names(package: &serde_json::Value) -> Vec<String> {
  let mut names = BTreeSet::new();
  for section in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
    let Some(map) = package.get(section).and_then(serde_json::Value::as_object) else {
      continue;
    };
    names.extend(map.keys().cloned());
  }
  names.into_iter().collect()
}

#[cfg(test)]
mod tests {
  use super::PackageIndex;
  use std::path::Path;
  use vue_vet_core::VueVersion;

  #[test]
  fn nearest_package_environment_wins_without_io() {
    let mut index = PackageIndex::default();
    index.insert(
      Path::new("/repo/package.json"),
      r#"{"dependencies":{"vue":"^3.4.0","root-only":"1"}}"#,
    );
    index.insert(
      Path::new("/repo/apps/admin/package.json"),
      r#"{"dependencies":{"vue":"^3.5.0","admin-only":"1"}}"#,
    );

    let environment =
      index.environment_for(Path::new("/repo/apps/admin/src/App.vue"), Path::new("/repo"));
    assert_eq!(environment.vue_version, VueVersion::parse_requirement("^3.5.0"));
    assert_eq!(environment.packages, ["admin-only", "vue"]);
  }
}
