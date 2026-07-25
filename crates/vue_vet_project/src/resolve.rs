//! Bundler-grade import resolution via `oxc_resolver` (Rolldown / enhanced-resolve).

use std::{
  collections::BTreeSet,
  path::{Component, Path, PathBuf},
};

use oxc_resolver::{
  AliasValue, ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};

/// Pinned `oxc_resolver` crate version hashed into the content-addressed cache key.
pub const OXC_RESOLVER_VERSION: &str = "11.21.0";

pub enum Resolution {
  File(String),
  External(String),
  Unresolved,
}

pub struct ProjectResolver {
  root: PathBuf,
  resolver: Resolver,
}

impl ProjectResolver {
  pub fn new(root: &Path) -> Self {
    let root = root.to_path_buf();
    let options = bundler_resolve_options(&root);
    Self { root, resolver: Resolver::new(options) }
  }

  pub fn resolve(&self, importer: &str, specifier: &str, known: &BTreeSet<String>) -> Resolution {
    if specifier == "#imports" {
      return Resolution::External(specifier.into());
    }
    let importer_path = absolute_under_root(&self.root, importer);
    self
      .resolver
      .resolve_file(&importer_path, specifier)
      .map_or(Resolution::Unresolved, |resolved| {
        classify_resolved(&self.root, resolved.full_path().as_path(), specifier, known)
      })
  }
}

fn bundler_resolve_options(root: &Path) -> ResolveOptions {
  let src = root.join("src");
  let yarn_pnp = root.join(".pnp.cjs").is_file() || root.join(".pnp.data.json").is_file();
  let tsconfig = if root.join(".nuxt/tsconfig.json").is_file() {
    Some(TsconfigDiscovery::Manual(TsconfigOptions {
      config_file: root.join(".nuxt/tsconfig.json"),
      references: TsconfigReferences::Auto,
    }))
  } else {
    Some(TsconfigDiscovery::Auto)
  };

  ResolveOptions {
    cwd: Some(root.to_path_buf()),
    tsconfig,
    alias: vec![
      ("@".into(), vec![AliasValue::Path(path_string(&src))]),
      ("~".into(), vec![AliasValue::Path(path_string(root))]),
    ],
    alias_fields: vec![vec!["browser".into()]],
    condition_names: vec!["import".into(), "module".into(), "browser".into(), "default".into()],
    exports_fields: vec![vec!["exports".into()]],
    imports_fields: vec![vec!["imports".into()]],
    extension_alias: vec![
      (".js".into(), vec![".js".into(), ".ts".into(), ".tsx".into()]),
      (".jsx".into(), vec![".jsx".into(), ".ts".into(), ".tsx".into()]),
      (".mjs".into(), vec![".mjs".into(), ".mts".into()]),
      (".cjs".into(), vec![".cjs".into(), ".cts".into()]),
    ],
    extensions: [".vue", ".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs", ".json"]
      .into_iter()
      .map(str::to_owned)
      .collect(),
    main_fields: vec!["browser".into(), "module".into(), "main".into()],
    main_files: vec!["index".into()],
    modules: vec!["node_modules".into()],
    symlinks: true,
    node_path: false,
    builtin_modules: false,
    module_type: true,
    allow_package_exports_in_directory_resolve: true,
    yarn_pnp,
    ..ResolveOptions::default()
  }
}

fn classify_resolved(
  root: &Path,
  absolute: &Path,
  specifier: &str,
  known: &BTreeSet<String>,
) -> Resolution {
  match relativize(root, absolute) {
    Some(relative) if known.contains(&relative) => Resolution::File(relative),
    Some(_) | None => Resolution::External(specifier.into()),
  }
}

fn absolute_under_root(root: &Path, logical: &str) -> PathBuf {
  let path = Path::new(logical);
  if path.is_absolute() { path.to_path_buf() } else { root.join(path) }
}

fn relativize(root: &Path, absolute: &Path) -> Option<String> {
  let root = dunce_canonicalize(root)?;
  let absolute = dunce_canonicalize(absolute)?;
  absolute.strip_prefix(&root).ok().map(normalized_path)
}

/// Prefer `dunce`-style simplification without adding a dependency: canonicalize
/// when possible, otherwise return the path as-is for prefix stripping.
fn dunce_canonicalize(path: &Path) -> Option<PathBuf> {
  path.canonicalize().ok().or_else(|| Some(path.to_path_buf()))
}

fn path_string(path: &Path) -> String {
  path.to_string_lossy().into_owned()
}

pub fn normalized_path(path: &Path) -> String {
  let mut parts = Vec::new();
  for component in path.components() {
    match component {
      Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
      Component::ParentDir => {
        parts.pop();
      }
      Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
    }
  }
  parts.join("/")
}

/// Repository-relative paths that affect bundler resolution and must join cache keys.
#[must_use]
pub fn resolver_config_inputs(root: &Path) -> Vec<String> {
  const CANDIDATES: &[&str] = &[
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "tsconfig.json",
    "tsconfig.app.json",
    "tsconfig.node.json",
    ".nuxt/tsconfig.json",
  ];
  let mut inputs = Vec::new();
  for candidate in CANDIDATES {
    if root.join(candidate).is_file() {
      inputs.push((*candidate).to_owned());
    }
  }
  // Additional root-level tsconfig*.json files (deterministic order).
  if let Ok(entries) = std::fs::read_dir(root) {
    let mut extras = entries
      .filter_map(Result::ok)
      .filter(|entry| {
        entry.file_type().is_ok_and(|kind| !kind.is_dir())
          && entry.file_name().to_str().is_some_and(|name| {
            name.starts_with("tsconfig")
              && Path::new(name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
          })
      })
      .filter_map(|entry| entry.file_name().into_string().ok())
      .filter(|name| !inputs.iter().any(|existing| existing == name))
      .collect::<Vec<_>>();
    extras.sort();
    inputs.extend(extras);
  }
  inputs
}
