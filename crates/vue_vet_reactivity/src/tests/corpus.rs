use std::collections::BTreeSet;

use super::helpers::*;

#[test]
fn covers_one_hundred_systematic_scenarios() {
  let fixtures = load_fixture_batches::<LocalFixture>(&SYSTEMATIC_FIXTURES);
  let names = fixtures.iter().map(|fixture| fixture.name.as_str()).collect::<BTreeSet<_>>();
  let sources = fixtures.iter().map(|fixture| fixture.source.as_str()).collect::<BTreeSet<_>>();
  for fixture in &fixtures {
    assert_local_fixture(fixture);
  }
  assert_eq!(fixtures.len(), 100, "the systematic corpus must contain exactly 100 cases");
  assert_eq!(names.len(), 100, "all systematic scenario names must be unique");
  assert_eq!(sources.len(), 100, "all systematic scenario sources must be unique");
}

#[test]
fn covers_one_hundred_complex_single_module_scenarios() {
  let fixtures = load_fixture_batches::<LocalFixture>(&COMPLEX_FIXTURES);
  let names = fixtures.iter().map(|fixture| fixture.name.as_str()).collect::<BTreeSet<_>>();
  let sources = fixtures.iter().map(|fixture| fixture.source.as_str()).collect::<BTreeSet<_>>();
  for fixture in &fixtures {
    assert_local_fixture(fixture);
  }
  assert_eq!(fixtures.len(), 100, "the complex corpus must contain exactly 100 cases");
  assert_eq!(names.len(), 100, "all complex scenario names must be unique");
  assert_eq!(sources.len(), 100, "all complex scenario sources must be unique");
}

#[test]
fn covers_eighty_real_cross_module_scenarios() {
  let fixtures = load_fixture_batches::<ModuleFixture>(&MODULE_FIXTURES);
  let names = fixtures.iter().map(|fixture| fixture.name.as_str()).collect::<BTreeSet<_>>();
  let signatures = fixtures
    .iter()
    .map(|fixture| module_fixture_signature(&fixture.modules, &fixture.links))
    .collect::<BTreeSet<_>>();
  for fixture in &fixtures {
    assert_module_case(&fixture.name, &fixture.modules, &fixture.links, &fixture.expected);
  }
  assert_eq!(fixtures.len(), 80, "the module corpus must contain exactly 80 cases");
  assert_eq!(names.len(), 80, "all module scenario names must be unique");
  assert_eq!(signatures.len(), 80, "all module scenario sources must be unique");
}

#[test]
fn validates_real_world_module_patterns() {
  let mut names = BTreeSet::new();
  let mut provenances = BTreeSet::new();
  for (case_dir, source) in REAL_WORLD_FIXTURES {
    let manifest_path = format!("real-world/{case_dir}/case.json");
    let fixture = parse_fixture::<RealWorldFixture>(&manifest_path, source);
    assert!(names.insert(fixture.name.clone()), "real-world fixture names must be unique");
    assert!(
      fixture.provenance.commit.len() == 40
        && fixture.provenance.commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
      "real-world fixture commits must be full hexadecimal SHAs: {}",
      fixture.name
    );
    assert!(
      !fixture.provenance.repository.is_empty()
        && !fixture.provenance.path.is_empty()
        && !fixture.provenance.adaptation.is_empty(),
      "real-world fixture provenance must be complete: {}",
      fixture.name
    );
    let provenance = format!(
      "{}@{}:{}",
      fixture.provenance.repository, fixture.provenance.commit, fixture.provenance.path
    );
    assert!(provenances.insert(provenance), "real-world provenance entries must be unique");
    let modules = load_real_world_modules(case_dir, &fixture.modules);
    assert_module_case(&fixture.name, &modules, &fixture.links, &fixture.expected);
  }
  assert_eq!(names.len(), 5, "the real-world corpus must retain five fixed-source cases");
}
