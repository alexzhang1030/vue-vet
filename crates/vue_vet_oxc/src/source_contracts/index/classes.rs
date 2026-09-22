//! Class records, `new` sites, and extracted methods.

use std::collections::{HashMap, HashSet};

use oxc_semantic::SymbolId;

use super::{ClassNewInfo, ClassRecord, ExtractedMethod};

#[derive(Default)]
pub(in crate::source_contracts) struct ClassIndex {
  pub records: HashMap<SymbolId, ClassRecord>,
  pub news: HashMap<u64, ClassNewInfo>,
  pub extracted_methods: HashMap<SymbolId, ExtractedMethod>,
  pub prototype_touch: HashSet<SymbolId>,
  pub prototype_mutated: bool,
}
