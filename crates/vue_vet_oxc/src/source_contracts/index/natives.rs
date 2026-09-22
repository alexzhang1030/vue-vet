//! Native constructor aliases, collection ctors, and intrinsic poison.

use std::collections::{HashMap, HashSet};

use oxc_semantic::SymbolId;

use super::{CollectionCtor, NativeSymbol};

#[derive(Default)]
#[expect(
  clippy::struct_excessive_bools,
  reason = "each intrinsic poison is an independent whole-file proof"
)]
pub(in crate::source_contracts) struct NativeIndex {
  pub tainted_ctors: HashSet<&'static str>,
  pub shadowed_ctors: HashSet<&'static str>,
  pub symbols: HashMap<SymbolId, NativeSymbol>,
  pub ctor_aliases: HashMap<SymbolId, &'static str>,
  pub dates: HashSet<u64>,
  pub date_poisoned: bool,
  pub json_poisoned: bool,
  pub collections: HashMap<u64, CollectionCtor>,
  pub clone_intrinsic_poisoned: bool,
  pub string_capability_poisoned: bool,
  pub map_intrinsic_poisoned: bool,
}
