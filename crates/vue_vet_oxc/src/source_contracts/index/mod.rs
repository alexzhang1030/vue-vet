//! One-pass indexes: Vue imports, aliases, writes, escapes, span maps.
//!
//! Const alias roots are compressed before mutation indexing so forward
//! references share the same root-keyed summaries. Collection capability is a
//! dedicated poisoned-root set; generic `uncertain` / `escaped` stay source5.
//! Escape walks that exhaust `MAX_ESCAPE_DEPTH` on a leftover
//! logical/conditional/sequence/aggregate do not trust that node: leftover
//! spans are applied during the existing semantic-reference pass, including
//! unresolved native constructor/prototype identifiers. Const aliases of those
//! constructors and `.prototype` objects share one precomputed native-kind
//! identity (`intern_native_ctor`) queried by both ordinary and leftover
//! escapes. A true `globalThis` alias escape taints supported intrinsic
//! constructors.

use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, ArrayExpression, ArrayExpressionElement, AssignmentOperator, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingPattern, CallExpression,
    Expression, ForStatementLeft, FormalParameters, IdentifierReference, LogicalOperator,
    NewExpression, ObjectPropertyKind, PropertyKind, SimpleAssignmentTarget,
    StaticMemberExpression, UnaryOperator, VariableDeclarator,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};
use oxc_syntax::reference::ReferenceFlags;
use vue_vet_core::{ScriptKind, SourceSpan};

use super::MAX_DEPTH;
use super::atom::{PrimitiveAtom, atom_of_expression};
use super::class::{
  ClassNewInfo, ClassRecord, MemberRecord, analyze_class, class_binding_symbol, class_new_info,
};
use super::proof::{
  ANCESTOR_BUDGET, DemandOrigin, DemandRole, Reach, classify_reach, classify_reach_except_chain,
  classify_role, enclosing_call, is_custom_prototype_key, is_ts_wrapper, skip_ts_parent,
};
use super::shape::{
  self, CollectionCtor, HintClass, Literal, OptionValue, PrimitiveAtom as ShapePrimitiveAtom,
  PrimitiveKind, Scalar, ShapeHint, VueImport, VueUseImport, hint_of, intern_extractable_method,
  intern_native_ctor, is_actual_proxy_runtime_source, is_fresh_allocation,
  is_known_receiver_method, is_proxy_allocating_api, is_unresolved_date, literal_of,
  primitive_atom, resolve_vue_api, resolve_vueuse_api, scalar_of, span_key,
  unresolved_collection_kind,
};
use super::stats::{SourceContractStats, WorkCounter};
use super::timeline::{self, Timed, Timeline};
use crate::facts::source_span;

const MAX_ALIAS_DEPTH: u8 = 8;
const MAX_ROLE_ANCESTORS: u8 = 8;
/// Structured logical/conditional/sequence/aggregate walk. Exhaustion does
/// not trust the leftover node; raising this bound only moves the defect.
const MAX_ESCAPE_DEPTH: u8 = 8;

mod awaits;
mod classes;
mod natives;
mod options;
mod queries;
mod scan;
mod sites;
pub(super) use sites::*;

/// Object-literal maps. Callers still see one `Indexes`; these three stay together
/// because a closed object, its props, and its literal values are one record.
#[derive(Default)]
pub(super) struct ObjectIndex {
  pub objects: HashMap<u64, Vec<ObjectEntry>>,
  pub object_props: HashMap<u64, HashMap<String, ObjectProp>>,
  literals: HashMap<u64, Literal>,
}

pub(super) struct Indexes {
  pub vue_imports: HashMap<SymbolId, VueImport>,
  pub vueuse_imports: HashMap<SymbolId, VueUseImport>,
  pub alias_root: HashMap<SymbolId, SymbolId>,
  pub aliases_of: HashMap<SymbolId, Vec<SymbolId>>,
  pub escaped: HashSet<SymbolId>,
  pub uncertain: HashSet<SymbolId>,
  pub reassigned: HashSet<SymbolId>,
  pub unknown_member_touch: HashSet<SymbolId>,
  pub capability_poisoned: HashSet<SymbolId>,
  unresolved_escape_spans: Vec<Span>,
  unresolved_global_refs: Vec<(Span, &'static str)>,
  pub native_index: natives::NativeIndex,
  pub class_index: classes::ClassIndex,
  pub toref_identity_uncertain: HashSet<SymbolId>,
  pub toref_helper_escape: HashSet<SymbolId>,
  pub value_writes: HashMap<SymbolId, Vec<ValueWrite>>,
  /// Every `.value` write, including compound assigns. Ignore-window proof
  /// uses this list to see outside-updater writes that `value_writes` drops.
  value_write_events: HashMap<SymbolId, Vec<ValueWrite>>,
  pub value_reads: HashMap<SymbolId, Vec<MemberUse>>,
  reads: ValueReadLanes,
  pub member_reads_by_root: HashMap<SymbolId, Vec<NamedUse>>,
  pub chained_value_by_root: HashMap<SymbolId, Vec<NamedUse>>,
  pub member_calls_by_root: HashMap<SymbolId, Vec<NamedUse>>,
  pub identifier_calls: HashMap<SymbolId, Vec<CallUse>>,
  pub result_demands: HashMap<SymbolId, Vec<ResultDemand>>,
  pub value_demands: HashMap<SymbolId, Vec<ValueDemand>>,
  pub destructure_by_object: HashMap<SymbolId, Vec<(SymbolId, String)>>,
  call_destructure: HashMap<u64, Vec<(SymbolId, String)>>,
  value_writes_by_callable: HashMap<(SymbolId, Option<NodeId>), Vec<ValueWrite>>,
  pub awaits_by_callable: HashMap<Option<NodeId>, Vec<AwaitPositionSite>>,
  pub disposals_by_callable: HashMap<Option<NodeId>, Vec<DisposeSite>>,
  pub watches_by_source: HashMap<SymbolId, Vec<WatchConsumer>>,
  pub effect_callbacks: HashMap<NodeId, EffectCallback>,
  pub run_callback_scope: HashMap<NodeId, SymbolId>,
  pub async_callables: HashSet<NodeId>,
  pub interned: Vec<String>,
  pub call_nodes: HashMap<u64, NodeId>,

  pub member_writes: HashMap<(SymbolId, String), Vec<MemberWrite>>,
  pub capability_touch: HashSet<SymbolId>,
  pub closed_key_unknown: HashSet<SymbolId>,
  pub hints: HashMap<u64, ShapeHint>,
  pub primitives: HashMap<u64, ShapePrimitiveAtom>,
  pub callables: HashMap<u64, NodeId>,
  pub calls: HashMap<u64, CallInfo>,
  pub object_index: ObjectIndex,
  /// Inner object/array span for a (possibly asserted) expression span.
  pub literal_span: HashMap<u64, Span>,
  /// Array expression spans whose elements include a spread.
  pub array_spread: HashSet<u64>,

  pub arrays: HashSet<u64>,
  pub closed_objects: HashMap<u64, bool>,
  pub capability_uncertain: HashSet<SymbolId>,
  pub stmt_site: HashMap<NodeId, StmtSite>,
  events_by_block: HashMap<NodeId, Timeline>,
  control_events_by_block: HashMap<NodeId, Timeline>,
  pause_events_by_block: HashMap<NodeId, Timeline>,
  pub init_span: HashMap<SymbolId, Span>,
  pub value_write_roots: HashSet<SymbolId>,
  pub member_write_roots: HashSet<SymbolId>,

  stop_offsets: Timeline,
  producer_call_offsets: Timeline,
  allowed_offsets: Timeline,
  pub functions: HashMap<u64, FunctionInfo>,
  pub function_by_node: HashMap<NodeId, Span>,
  pub effect_calls: HashMap<u64, ArgUse>,
  pub watch_getters: HashMap<u64, ArgUse>,
  pub options: options::OptionsIndex,
  pub call_results: HashMap<u64, SymbolId>,
  pub arg_uses: HashMap<SymbolId, Vec<ArgUse>>,
  pub ident_calls: HashMap<SymbolId, Vec<IdentCall>>,
  pub handle_member_calls: HashMap<(SymbolId, String), Vec<IdentCall>>,

  root_members: HashMap<SymbolId, Vec<SymbolId>>,
  inactivity_by_handle_block: HashMap<(SymbolId, NodeId), Vec<IdentCall>>,
  pub atoms: HashMap<u64, PrimitiveAtom>,
  pub array_elements: HashMap<u64, Vec<Span>>,
  pub members: HashMap<u64, MemberInfo>,
  mixed_value_owners: HashSet<SymbolId>,
  mixed_member_owners: HashSet<(SymbolId, String)>,
  value_write_owner: HashMap<SymbolId, (Option<NodeId>, NodeId)>,
  member_write_owner: HashMap<(SymbolId, String), (Option<NodeId>, NodeId)>,
  member_call_by_span: HashMap<u64, NamedUse>,
  stops_by_region: HashMap<(SymbolId, Option<NodeId>, NodeId), Vec<MemberUse>>,
  barriers_by_region: HashMap<NodeId, Timeline>,
  closed_keys: HashMap<u64, HashSet<String>>,
  owners: HashMap<NodeId, Owner>,
  global_this_aliases: HashSet<SymbolId>,

  terminations_by_callable: HashMap<Option<NodeId>, Timeline>,
  pub object_literals: HashSet<u64>,
  pub news: HashMap<u64, NewInfo>,

  pub map_init_keys: HashMap<u64, Option<Vec<MapKeyRef>>>,
  pub member_calls: HashMap<(SymbolId, String), Vec<MemberCallSite>>,
  pub member_call_by_node: HashMap<NodeId, MemberCallSite>,
  pub map_ops: HashMap<SymbolId, Vec<MapOp>>,
  pub map_gets: HashMap<SymbolId, Vec<MemberCallSite>>,
  pub proxy_wrapper: HashMap<SymbolId, Span>,
  wrappers_of_alloc: HashMap<SymbolId, Vec<SymbolId>>,
  pub skip_marker_objects: HashSet<u64>,
  pub skip_written: HashSet<SymbolId>,

  pub region_start: HashMap<NodeId, usize>,
  pub init_offset: HashMap<SymbolId, usize>,
  pub known_truthy: HashMap<u64, bool>,
  pub known_nullish: HashMap<u64, bool>,
  pub helper_escaped: HashSet<SymbolId>,
  pending_map_key_args: Vec<PendingMapKeyArg>,
  wrapper_origin: HashMap<SymbolId, SymbolId>,
  pending_inline_writes: Vec<(SymbolId, String, MemberWrite)>,
  pending_inline_unknown: Vec<SymbolId>,
  canonical_of: HashMap<SymbolId, Option<SymbolId>>,
  unresolved_origin_touch: bool,
  alloc_capability: HashMap<SymbolId, bool>,
  await_index: AwaitIndexes,
  provides_by_key: HashMap<SymbolId, Vec<InjectionSite>>,
  injects_by_key: HashMap<SymbolId, Vec<InjectionSite>>,
  script_kind: ScriptKind,
  path_calls: HashMap<SymbolId, Vec<PathCall>>,
  path_reads: HashMap<SymbolId, Vec<PathRead>>,
  path_value_writes: HashMap<SymbolId, Vec<(Vec<String>, PathWrite)>>,
  pub(super) value_object_alias: HashMap<SymbolId, SymbolId>,
  nested_writes: HashMap<SymbolId, Vec<(String, NestedWrite)>>,
  work: WorkCounter,
}

impl Indexes {
  #[expect(
    clippy::too_many_arguments,
    reason = "the caller already resolved both import tables for its early exit; threading them avoids a second import pass"
  )]
  pub(super) fn build(
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
    vue_imports: HashMap<SymbolId, VueImport>,
    vueuse_imports: HashMap<SymbolId, VueUseImport>,
    work: WorkCounter,
  ) -> Self {
    let mut indexes = Self {
      vue_imports,
      vueuse_imports,
      alias_root: HashMap::new(),
      aliases_of: HashMap::new(),
      escaped: HashSet::new(),
      uncertain: HashSet::new(),
      reassigned: HashSet::new(),
      unknown_member_touch: HashSet::new(),
      capability_poisoned: HashSet::new(),
      unresolved_escape_spans: Vec::new(),
      unresolved_global_refs: Vec::new(),
      native_index: natives::NativeIndex::default(),
      class_index: classes::ClassIndex::default(),
      toref_identity_uncertain: HashSet::new(),
      toref_helper_escape: HashSet::new(),
      value_writes: HashMap::new(),
      value_write_events: HashMap::new(),
      value_reads: HashMap::new(),
      reads: ValueReadLanes::default(),
      member_reads_by_root: HashMap::new(),
      chained_value_by_root: HashMap::new(),
      member_calls_by_root: HashMap::new(),
      identifier_calls: HashMap::new(),
      result_demands: HashMap::new(),
      value_demands: HashMap::new(),
      destructure_by_object: HashMap::new(),
      call_destructure: HashMap::new(),
      value_writes_by_callable: HashMap::new(),
      awaits_by_callable: HashMap::new(),
      disposals_by_callable: HashMap::new(),
      watches_by_source: HashMap::new(),
      effect_callbacks: HashMap::new(),
      run_callback_scope: HashMap::new(),
      async_callables: HashSet::new(),
      interned: Vec::new(),
      call_nodes: HashMap::new(),

      member_writes: HashMap::new(),
      capability_touch: HashSet::new(),
      closed_key_unknown: HashSet::new(),
      hints: HashMap::new(),
      primitives: HashMap::new(),
      callables: HashMap::new(),
      calls: HashMap::new(),
      object_index: ObjectIndex::default(),
      literal_span: HashMap::new(),
      array_spread: HashSet::new(),

      arrays: HashSet::new(),
      closed_objects: HashMap::new(),
      capability_uncertain: HashSet::new(),
      stmt_site: HashMap::new(),
      events_by_block: HashMap::new(),
      control_events_by_block: HashMap::new(),
      pause_events_by_block: HashMap::new(),
      init_span: HashMap::new(),
      value_write_roots: HashSet::new(),
      member_write_roots: HashSet::new(),

      stop_offsets: Timeline::new(),
      producer_call_offsets: Timeline::new(),
      allowed_offsets: Timeline::new(),
      functions: HashMap::new(),
      function_by_node: HashMap::new(),
      effect_calls: HashMap::new(),
      watch_getters: HashMap::new(),
      options: options::OptionsIndex::default(),
      call_results: HashMap::new(),
      arg_uses: HashMap::new(),
      ident_calls: HashMap::new(),
      handle_member_calls: HashMap::new(),
      root_members: HashMap::new(),
      inactivity_by_handle_block: HashMap::new(),
      atoms: HashMap::new(),
      array_elements: HashMap::new(),
      members: HashMap::new(),
      mixed_value_owners: HashSet::new(),
      mixed_member_owners: HashSet::new(),
      value_write_owner: HashMap::new(),
      member_write_owner: HashMap::new(),
      member_call_by_span: HashMap::new(),
      stops_by_region: HashMap::new(),
      barriers_by_region: HashMap::new(),
      closed_keys: HashMap::new(),
      owners: HashMap::new(),
      global_this_aliases: HashSet::new(),

      terminations_by_callable: HashMap::new(),
      object_literals: HashSet::new(),
      news: HashMap::new(),

      map_init_keys: HashMap::new(),
      member_calls: HashMap::new(),
      member_call_by_node: HashMap::new(),
      map_ops: HashMap::new(),
      map_gets: HashMap::new(),
      proxy_wrapper: HashMap::new(),
      wrappers_of_alloc: HashMap::new(),
      skip_marker_objects: HashSet::new(),
      skip_written: HashSet::new(),

      region_start: HashMap::new(),
      init_offset: HashMap::new(),
      known_truthy: HashMap::new(),
      known_nullish: HashMap::new(),
      helper_escaped: HashSet::new(),
      pending_map_key_args: Vec::new(),
      wrapper_origin: HashMap::new(),
      pending_inline_writes: Vec::new(),
      pending_inline_unknown: Vec::new(),
      canonical_of: HashMap::new(),
      unresolved_origin_touch: false,
      alloc_capability: HashMap::new(),
      await_index: AwaitIndexes::default(),

      provides_by_key: HashMap::new(),
      injects_by_key: HashMap::new(),
      script_kind: kind,

      path_calls: HashMap::new(),
      path_reads: HashMap::new(),
      path_value_writes: HashMap::new(),
      value_object_alias: HashMap::new(),
      nested_writes: HashMap::new(),
      work,
    };
    indexes.build_owners(semantic);
    indexes.precompute_aliases(semantic);
    indexes.note_ctor_shadows(semantic);
    indexes.record_region_starts(semantic, line_index, sfc_source, script_offset);
    indexes.scan(semantic, line_index, sfc_source, script_offset, kind);
    indexes.finish_practice_indexes(semantic, line_index, sfc_source, script_offset, kind);
    indexes.finish_aliases_and_roles(semantic);
    indexes.remap_symbol_maps();
    indexes.summarize_root_members();
    indexes.build_alias_members();

    indexes.summarize_writes();
    indexes.summarize_until_closed_sources();
    indexes.build_until_await_lookups();
    indexes.precompute_closed_objects();
    indexes.summarize_closed_keys();
    indexes.summarize_inactivity();
    indexes.sort_finish_indexes();
    indexes.finish_pending_map_key_args(semantic);
    indexes.finish_map_indexes();

    indexes
  }

  fn sort_finish_indexes(&mut self) {
    {
      let work = &self.work;
      for events in self.events_by_block.values_mut() {
        events.sort(work);
      }
      for offsets in self.terminations_by_callable.values_mut() {
        offsets.sort(work);
      }
      for barriers in self.barriers_by_region.values_mut() {
        barriers.sort(work);
      }
      for events in self.control_events_by_block.values_mut() {
        events.sort(work);
      }
      for events in self.pause_events_by_block.values_mut() {
        events.sort(work);
      }
    }
    for stops in self.stops_by_region.values_mut() {
      stops.sort_by_key(|stop| stop.head.offset);
    }
    for writes in self.member_writes.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    for writes in self.value_writes.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    for writes in self.value_write_events.values_mut() {
      writes.sort_by_key(|write| write.offset);
    }
    self.index_vueuse_value_writes();
    for uses in self.value_reads.values_mut() {
      uses.sort_by_key(|use_site| use_site.head.offset);
    }
    self.reads.sort(&self.work);
    for awaits in self.awaits_by_callable.values_mut() {
      self.work.add_queries(awaits.len() as u64);
      awaits.sort_by_key(|site| site.head.offset);
    }
    self.work.add_queries(self.await_index.awaits.len() as u64);
    self.await_index.awaits.sort_by_key(|site| site.head.offset);
    for uses in self.await_index.await_method_calls.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.head.offset);
    }
    for uses in self.await_index.result_method_calls.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.head.offset);
    }
    for sites in self.await_index.escapes.values_mut() {
      sites.sort_by_key(|site| site.offset);
    }
    for sites in self.await_index.awaits_by_region.values_mut() {
      sites.sort_by_key(|site| site.head.offset);
    }
    for sites in self.await_index.await_by_bound.values_mut() {
      sites.sort_by_key(|site| site.head.offset);
    }
    for disposals in self.disposals_by_callable.values_mut() {
      self.work.add_queries(disposals.len() as u64);
      disposals.sort_by_key(|site| site.offset);
    }
    for watches in self.watches_by_source.values_mut() {
      self.work.add_queries(watches.len() as u64);
      watches.sort_by_key(|site| site.offset);
    }
    for uses in self.member_reads_by_root.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.head.offset);
    }
    for uses in self.chained_value_by_root.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.head.offset);
    }
    for uses in self.member_calls_by_root.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.head.offset);
    }
    for sites in self.provides_by_key.values_mut() {
      sites.sort_by_key(|site| site.head.offset);
    }
    for sites in self.injects_by_key.values_mut() {
      sites.sort_by_key(|site| site.head.offset);
    }
    for uses in self.identifier_calls.values_mut() {
      uses.sort_by_key(|use_site| use_site.head.offset);
    }
    for uses in self.result_demands.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.head.offset);
    }
    for uses in self.value_demands.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.head.offset);
    }
    self.stop_offsets.sort(&self.work);
    self.producer_call_offsets.sort(&self.work);
    self.finish_allowed_offsets();
    for uses in self.arg_uses.values_mut() {
      uses.sort_by_key(|use_site| use_site.offset);
    }
    for calls in self.ident_calls.values_mut() {
      calls.sort_by_key(|call| call.offset);
    }
    for calls in self.handle_member_calls.values_mut() {
      calls.sort_by_key(|call| call.offset);
    }

    for writes in self.nested_writes.values_mut() {
      writes.sort_by_key(|(_, write)| write.offset);
    }
    for calls in self.path_calls.values_mut() {
      calls.sort_by_key(|call| call.site.head.offset);
    }
    for reads in self.path_reads.values_mut() {
      reads.sort_by_key(|read| read.site.head.offset);
    }
    for writes in self.path_value_writes.values_mut() {
      writes.sort_by_key(|(_, write)| write.offset);
    }
  }

  pub(super) const fn work_counter(&self) -> &WorkCounter {
    &self.work
  }

  fn finish_allowed_offsets(&mut self) {
    let demand_len = self
      .result_demands
      .values()
      .map(Vec::len)
      .chain(self.value_demands.values().map(Vec::len))
      .sum::<usize>();
    let mut allowed = Timeline::new();
    allowed.reserve(
      self
        .producer_call_offsets
        .len()
        .saturating_add(self.stop_offsets.len())
        .saturating_add(demand_len),
    );
    allowed.extend_from(&self.producer_call_offsets);
    allowed.extend_from(&self.stop_offsets);
    for uses in self.result_demands.values() {
      for demand in uses {
        allowed.push(demand.site.head.offset);
      }
    }
    for uses in self.value_demands.values() {
      for demand in uses {
        allowed.push(demand.site.head.offset);
      }
    }
    self.work.add_queries(u64::try_from(allowed.len()).unwrap_or(u64::MAX));
    allowed.sort(&self.work);
    self.allowed_offsets = allowed;
  }

  fn build_owners(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    for (node_id, node) in semantic.nodes().iter_enumerated() {
      self.work.add_owners(1);
      let parent = semantic.nodes().parent_id(node_id);
      let inherited = self.owners.get(&parent).copied().unwrap_or(Owner {
        callable: None,
        block: None,
        region: None,
      });
      let mut callable = inherited.callable;
      let mut block = inherited.block;
      let mut region = inherited.region;
      match node.kind() {
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => callable = Some(node_id),
        AstKind::Program(_) | AstKind::FunctionBody(_) => {
          block = Some(node_id);
          region = Some(node_id);
        }
        AstKind::BlockStatement(_) => block = Some(node_id),
        _ => {}
      }
      self.owners.insert(node_id, Owner { callable, block, region });
    }
  }

  fn build_alias_members(&mut self) {
    for (local, root) in &self.alias_root {
      self.note_query();
      if *local == *root {
        continue;
      }
      self.aliases_of.entry(*root).or_default().push(*local);
    }
    let mut aliases = std::mem::take(&mut self.aliases_of);
    for members in aliases.values_mut() {
      members.sort_by(|left, right| {
        self.note_query();
        left.cmp(right)
      });
      let before = members.len();
      members.dedup();
      self.work.add_queries(before as u64);
    }
    self.aliases_of = aliases;
  }

  fn summarize_writes(&mut self) {
    for (root, writes) in &self.value_writes {
      let Some(first) = writes.first() else {
        continue;
      };
      self.work.add_writes(1);
      let mixed = writes.iter().skip(1).any(|write| {
        self.work.add_writes(1);
        write.callable != first.callable || write.block != first.block
      });
      if mixed {
        self.mixed_value_owners.insert(*root);
      } else {
        self.value_write_owner.insert(*root, (first.callable, first.block));
      }
    }
    for (key, writes) in &self.member_writes {
      let Some(first) = writes.first() else {
        continue;
      };
      self.work.add_writes(1);
      let mixed = writes.iter().skip(1).any(|write| {
        self.work.add_writes(1);
        write.callable != first.callable || write.block != first.block
      });
      if mixed {
        self.mixed_member_owners.insert(key.clone());
      } else {
        self.member_write_owner.insert(key.clone(), (first.callable, first.block));
      }
    }
  }

  fn precompute_closed_objects(&mut self) {
    for (key, entries) in &self.object_index.objects {
      self.note_query();
      let mut closed = true;
      for entry in entries {
        self.work.add_object_entries(1);
        match entry {
          ObjectEntry::Spread | ObjectEntry::Computed | ObjectEntry::Accessor { .. } => {
            closed = false;
            break;
          }
          ObjectEntry::Data { name, .. } | ObjectEntry::Method { name, .. }
            if is_capability_key(name) =>
          {
            closed = false;
            break;
          }
          ObjectEntry::Data { .. } | ObjectEntry::Method { .. } => {}
        }
      }
      self.closed_objects.insert(*key, closed);
    }
  }

  fn summarize_inactivity(&mut self) {
    for (root, calls) in &self.ident_calls {
      for call in calls {
        self.note_query();
        self.inactivity_by_handle_block.entry((*root, call.block)).or_default().push(*call);
      }
    }
    for ((root, property), calls) in &self.handle_member_calls {
      if property != "stop" && property != "pause" {
        continue;
      }
      for call in calls {
        self.note_query();
        self.inactivity_by_handle_block.entry((*root, call.block)).or_default().push(*call);
      }
    }
    for events in self.inactivity_by_handle_block.values_mut() {
      events.sort_by_key(|event| event.offset);
    }
  }

  pub(super) fn owner(&self, node_id: NodeId) -> Owner {
    self.owners.get(&node_id).copied().unwrap_or(Owner {
      callable: None,
      block: None,
      region: None,
    })
  }

  fn remap_symbol_maps(&mut self) {
    let ident_calls = std::mem::take(&mut self.ident_calls);
    for (symbol_id, mut calls) in ident_calls {
      self.note_query();
      self.ident_calls.entry(self.root_of(symbol_id)).or_default().append(&mut calls);
    }
    let arg_uses = std::mem::take(&mut self.arg_uses);
    for (symbol_id, mut uses) in arg_uses {
      self.note_query();
      self.arg_uses.entry(self.root_of(symbol_id)).or_default().append(&mut uses);
    }
    let value_reads = std::mem::take(&mut self.value_reads);
    for (symbol_id, mut reads) in value_reads {
      self.note_query();
      self.value_reads.entry(self.root_of(symbol_id)).or_default().append(&mut reads);
    }
    let mut lanes = std::mem::take(&mut self.reads);
    lanes.remap_roots(|symbol_id| self.root_of(symbol_id), || self.note_query());
    self.reads = lanes;
    let watches_by_source = std::mem::take(&mut self.watches_by_source);
    for (symbol_id, mut watches) in watches_by_source {
      self.note_query();
      self.watches_by_source.entry(self.root_of(symbol_id)).or_default().append(&mut watches);
    }
    let handle_member_calls = std::mem::take(&mut self.handle_member_calls);
    for ((symbol_id, property), mut calls) in handle_member_calls {
      self.note_query();
      self
        .handle_member_calls
        .entry((self.root_of(symbol_id), property))
        .or_default()
        .append(&mut calls);
    }
  }

  fn summarize_root_members(&mut self) {
    for (alias, target) in &self.alias_root {
      self.note_query();
      let bucket = self.root_members.entry(*target).or_default();
      if bucket.is_empty() {
        bucket.push(*target);
        self.work.add_writes(1);
      }
      if *alias != *target {
        bucket.push(*alias);
        self.work.add_writes(1);
      }
    }
  }

  pub(super) fn owner_callable_block(&self, node_id: NodeId) -> (Option<NodeId>, Option<NodeId>) {
    self.note_query();
    let owner = self.owner(node_id);
    (owner.callable, owner.block)
  }

  pub(super) fn note_node(&self) {
    self.work.add_nodes(1);
  }

  pub(super) fn note_query(&self) {
    self.work.add_queries(1);
  }

  pub(super) fn note_object_entries(&self, n: u64) {
    self.work.add_object_entries(n);
  }

  pub(super) fn note_entry(&self) {
    self.work.add_object_entries(1);
  }

  pub(super) fn note_identity(&self) {
    self.work.add_key_lookups(1);
  }

  pub(super) fn note_mutation(&self) {
    self.work.add_writes(1);
  }

  pub(super) fn add_references(&self, n: u64) {
    self.work.add_references(n);
  }

  pub(super) fn add_writes(&self, n: u64) {
    self.work.add_writes(n);
  }

  pub(super) fn add_object_entries(&self, n: u64) {
    self.work.add_object_entries(n);
  }

  pub(super) fn add_queries(&self, n: u64) {
    self.work.add_queries(n);
  }

  pub(super) const fn work(&self) -> &WorkCounter {
    &self.work
  }

  /// `for...in` / `for...of` assignment heads write through `ForStatementLeft`.
  /// Variable-declaration heads keep binding semantics and are not mutations.
  fn note_loop_assignment_poison(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    left: &ForStatementLeft<'_>,
  ) {
    self.note_query();
    let Some(target) = left.as_assignment_target() else {
      return;
    };
    self.work.add_writes(1);
    if assignment_poisons_clone_intrinsic(semantic, target, &self.work) {
      self.native_index.clone_intrinsic_poisoned = true;
    }
    self.taint_assignment_target(semantic, target);
    self.mark_pattern_uncertain(semantic, target);
  }
}

fn summarize_object_props(
  entries: &[ObjectEntry],
  work: &WorkCounter,
) -> HashMap<String, ObjectProp> {
  struct Track {
    last_data: Option<(usize, Span)>,
    accessor: bool,
  }
  let mut tracks: HashMap<String, Track> = HashMap::new();
  let mut last_uncertain = None;
  for (index, entry) in entries.iter().enumerate() {
    work.add_object_entries(1);
    match entry {
      ObjectEntry::Spread | ObjectEntry::Computed => last_uncertain = Some(index),
      ObjectEntry::Accessor { name } => {
        last_uncertain = Some(index);
        if let Some(name) = name {
          tracks
            .entry(name.clone())
            .or_insert(Track { last_data: None, accessor: false })
            .accessor = true;
        }
      }
      ObjectEntry::Data { name, value, .. } | ObjectEntry::Method { name, value, .. } => {
        tracks
          .entry(name.clone())
          .or_insert(Track { last_data: None, accessor: false })
          .last_data = Some((index, *value));
      }
    }
  }
  let mut props = HashMap::new();
  for (name, track) in tracks {
    work.add_object_entries(1);
    if track.accessor {
      props.insert(name, ObjectProp::Unknown);
      continue;
    }
    let Some((index, span)) = track.last_data else {
      props.insert(name, ObjectProp::Unknown);
      continue;
    };
    work.add_object_entries(1);
    if last_uncertain.is_some_and(|event| event > index) {
      props.insert(name, ObjectProp::Unknown);
    } else {
      props.insert(name, ObjectProp::Value(span));
    }
  }
  props
}

const CAPABILITY_KEYS: &[&str] = &[
  "__v_isRef",
  "__v_skip",
  "__v_isReadonly",
  "__v_raw",
  "__v_isReactive",
  "__v_isShallow",
  "__proto__",
  "prototype",
];

fn is_capability_key(name: &str) -> bool {
  CAPABILITY_KEYS.contains(&name)
}

const TOREF_CAPABILITY_KEY: &str = "__v_isRef";

/// Exact span identity for a callee / tag / member object, including the
/// inner expression after TS and parenthesis wrappers.
fn callee_span_matches(expression: &Expression<'_>, current_span: Span) -> bool {
  expression.span() == current_span || expression.get_inner_expression().span() == current_span
}

fn callee_is_pause(call: &CallExpression<'_>) -> bool {
  matches!(
    call.callee.get_inner_expression(),
    Expression::StaticMemberExpression(member) if member.property.name.as_str() == "pause"
  )
}

fn is_known_constructor_or_watch(api: Option<&str>) -> bool {
  matches!(api, Some("reactive" | "shallowReactive" | "readonly" | "shallowReadonly" | "watch"))
}

fn known_static_member_object_role(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  work: &WorkCounter,
) -> bool {
  let ident_span = semantic.nodes().kind(node_id).span();
  work.add_queries(1);
  let parent = skip_ts_parent(semantic, node_id, work);
  match semantic.nodes().kind(parent) {
    AstKind::StaticMemberExpression(member) => {
      let object = member.object.get_inner_expression().span();
      object == ident_span || member.object.span() == ident_span
    }
    AstKind::ComputedMemberExpression(member) => {
      if !matches!(member.expression.get_inner_expression(), Expression::StringLiteral(_)) {
        return false;
      }
      let object = member.object.get_inner_expression().span();
      object == ident_span || member.object.span() == ident_span
    }
    _ => false,
  }
}

fn known_class_ctor_role(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  classes: &HashMap<SymbolId, ClassRecord>,
  root: SymbolId,
) -> bool {
  if !classes.contains_key(&root) {
    return false;
  }
  let oxc_ast::AstKind::NewExpression(expression) = semantic.nodes().parent_kind(node_id) else {
    return false;
  };
  let ident_span = semantic.nodes().kind(node_id).span();
  let callee = expression.callee.get_inner_expression().span();
  callee == ident_span
}

fn member_call_object_key<'a>(callee: &'a Expression<'a>) -> Option<(&'a Expression<'a>, &'a str)> {
  match callee {
    Expression::StaticMemberExpression(member) => {
      Some((&member.object, member.property.name.as_str()))
    }
    Expression::ComputedMemberExpression(member) => {
      let Expression::StringLiteral(literal) = member.expression.get_inner_expression() else {
        return None;
      };
      Some((&member.object, literal.value.as_str()))
    }
    _ => None,
  }
}

fn known_injection_key_role(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  calls: &HashMap<u64, CallInfo>,
  work: &WorkCounter,
) -> bool {
  let ident_span = semantic.nodes().kind(node_id).span();
  work.add_queries(1);
  let Some((_, call)) = enclosing_call(semantic, node_id, work) else {
    return false;
  };
  work.add_queries(1);
  let Some(info) = calls.get(&span_key(call.span)) else {
    return false;
  };
  if !matches!(info.api, Some("provide" | "inject")) || info.has_spread {
    return false;
  }
  let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
    return false;
  };
  let inner = first.get_inner_expression().span();
  inner == ident_span || first.span() == ident_span
}

fn boolean_literal(expression: &Expression<'_>) -> Option<bool> {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(literal.value),
    _ => None,
  }
}

fn known_const_alias_role(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(node_id) else {
    return false;
  };
  matches!(declarator.id, oxc_ast::ast::BindingPattern::BindingIdentifier(ref binding) if {
    binding.symbol_id.get().is_some_and(|symbol_id| {
      semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable)
    })
  })
}

fn closed_key_use_is_known(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  flags: ReferenceFlags,
  member_payload: bool,
  calls: &HashMap<u64, CallInfo>,
  work: &WorkCounter,
) -> bool {
  if known_torefs_borrow_role(semantic, node_id, calls, work) {
    return true;
  }
  if known_const_alias_role(semantic, node_id) {
    return true;
  }
  if member_payload {
    return known_static_member_read_role(semantic, node_id, work);
  }
  flags.is_write()
}

fn known_torefs_borrow_role(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  calls: &HashMap<u64, CallInfo>,
  work: &WorkCounter,
) -> bool {
  let ident_span = semantic.nodes().kind(node_id).span();
  let mut current = node_id;
  for _ in 0..ANCESTOR_BUDGET {
    work.add_queries(1);
    let parent = semantic.nodes().parent_id(current);
    match semantic.nodes().kind(parent) {
      wrapper if is_ts_wrapper(wrapper) => {
        current = parent;
      }
      AstKind::CallExpression(call) => {
        work.add_queries(1);
        let Some(info) = calls.get(&span_key(call.span)) else {
          return false;
        };
        if info.api != Some("toRefs") || info.has_spread {
          return false;
        }
        let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
          return false;
        };
        let inner = first.get_inner_expression().span();
        return inner == ident_span || first.span() == ident_span;
      }
      _ => return false,
    }
  }
  false
}

fn known_static_member_read_role(
  semantic: &oxc_semantic::Semantic<'_>,
  ident_node: NodeId,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  let parent = semantic.nodes().parent_id(ident_node);
  if !matches!(semantic.nodes().kind(parent), AstKind::StaticMemberExpression(_)) {
    return false;
  }
  let mut current = parent;
  for _ in 0..ANCESTOR_BUDGET {
    work.add_queries(1);
    let grand = semantic.nodes().parent_id(current);
    match semantic.nodes().kind(grand) {
      wrapper if is_ts_wrapper(wrapper) => {
        current = grand;
      }
      AstKind::CallExpression(_)
      | AstKind::NewExpression(_)
      | AstKind::TaggedTemplateExpression(_)
      | AstKind::UnaryExpression(_)
      | AstKind::UpdateExpression(_) => return false,
      _ => return true,
    }
  }
  false
}

pub(super) fn chain_optional(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
  work: &WorkCounter,
) -> bool {
  for _ in 0..8 {
    work.add_queries(1);
    let parent = semantic.nodes().parent_id(node_id);
    match semantic.nodes().kind(parent) {
      AstKind::ChainExpression(_) => return true,
      wrapper if is_ts_wrapper(wrapper) => node_id = parent,
      _ => return false,
    }
  }
  true
}

fn region_of(owner: Owner, node_id: NodeId) -> NodeId {
  owner.region.or(owner.block).unwrap_or(node_id)
}

fn object_entries(object: &oxc_ast::ast::ObjectExpression<'_>) -> Vec<ObjectEntry> {
  let mut entries = Vec::new();
  for property_kind in &object.properties {
    match property_kind {
      ObjectPropertyKind::SpreadProperty(_) => entries.push(ObjectEntry::Spread),
      ObjectPropertyKind::ObjectProperty(prop) => {
        if prop.kind != PropertyKind::Init {
          entries.push(ObjectEntry::Accessor {
            name: prop.key.static_name().map(|name| name.to_string()),
          });
          continue;
        }
        match prop.key.static_name() {
          Some(name) if prop.method => entries.push(ObjectEntry::Method {
            name: name.to_string(),
            key: prop.key.span(),
            value: prop.value.span(),
          }),
          Some(name) => entries.push(ObjectEntry::Data {
            name: name.to_string(),
            key: prop.key.span(),
            value: prop.value.span(),
          }),
          None => entries.push(ObjectEntry::Computed),
        }
      }
    }
  }
  entries
}

fn block_is_straight_line(semantic: &oxc_semantic::Semantic<'_>, block_id: NodeId) -> bool {
  match semantic.nodes().kind(block_id) {
    AstKind::Program(_) | AstKind::FunctionBody(_) => true,
    AstKind::BlockStatement(_) => matches!(
      semantic.nodes().parent_kind(block_id),
      AstKind::Function(_)
        | AstKind::FunctionBody(_)
        | AstKind::ArrowFunctionExpression(_)
        | AstKind::Program(_)
    ),
    _ => false,
  }
}

fn is_unresolved_global(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> bool {
  reference_symbol(semantic, identifier).is_none()
}

#[derive(Clone, Copy)]
enum CloneKeyProof {
  Definite,
  Possible,
}

fn is_native_structured_clone(
  callee: &Expression<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
  expression_matches_clone_intrinsic(semantic, callee, CloneKeyProof::Definite)
}

fn callee_has_actual_proxy_origin(
  callee: &Expression<'_>,
  vue_imports: &HashMap<SymbolId, VueImport>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> bool {
  let inner = callee.get_inner_expression();
  if let Some(identifier) = inner.get_identifier_reference() {
    return indexed_import_is_actual_proxy(vue_imports, semantic, identifier, work);
  }
  let Expression::StaticMemberExpression(member) = inner else {
    return false;
  };
  let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  indexed_import_is_actual_proxy(vue_imports, semantic, object, work)
}

fn indexed_import_is_actual_proxy(
  vue_imports: &HashMap<SymbolId, VueImport>,
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
  work: &WorkCounter,
) -> bool {
  let Some(symbol_id) = reference_symbol(semantic, identifier) else {
    return false;
  };
  if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::Import) {
    return false;
  }
  work.add_import_source_steps(1);
  vue_imports.get(&symbol_id).is_some_and(|import| is_actual_proxy_runtime_source(import.source()))
}

/// Oxc 0.142 flattened `AssignmentTarget` (inherits `SimpleAssignmentTarget` +
/// `AssignmentTargetPattern`): identifier; static/computed/private members;
/// object/array patterns; TS as / satisfies / non-null / assertion.
/// Nested default and rest targets recurse here. `for...in` / `for...of`
/// assignment heads share this `Possible` classifier; declaration heads do not.
fn assignment_poisons_clone_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  left: &AssignmentTarget<'_>,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  match left {
    AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
      identifier_is_unresolved_clone(semantic, identifier)
    }
    AssignmentTarget::StaticMemberExpression(member) => unresolved_global_this_clone_member(
      semantic,
      &member.object,
      Some(member.property.name.as_str()),
      None,
      CloneKeyProof::Possible,
    ),
    AssignmentTarget::ComputedMemberExpression(member) => unresolved_global_this_clone_member(
      semantic,
      &member.object,
      None,
      Some(&member.expression),
      CloneKeyProof::Possible,
    ),
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      object.properties.iter().any(|property| match property {
        AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
          maybe_default_poisons_clone_intrinsic(semantic, &property.binding, work)
        }
        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
          identifier_is_unresolved_clone(semantic, &property.binding)
        }
      }) || object
        .rest
        .as_ref()
        .is_some_and(|rest| assignment_poisons_clone_intrinsic(semantic, &rest.target, work))
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      array
        .elements
        .iter()
        .flatten()
        .any(|element| maybe_default_poisons_clone_intrinsic(semantic, element, work))
        || array
          .rest
          .as_ref()
          .is_some_and(|rest| assignment_poisons_clone_intrinsic(semantic, &rest.target, work))
    }
    AssignmentTarget::TSAsExpression(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    AssignmentTarget::PrivateFieldExpression(_) => false,
  }
}

fn maybe_default_poisons_clone_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTargetMaybeDefault<'_>,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
      assignment_poisons_clone_intrinsic(semantic, &with_default.binding, work)
    }
    other => other
      .as_assignment_target()
      .is_some_and(|assignment| assignment_poisons_clone_intrinsic(semantic, assignment, work)),
  }
}

fn simple_target_poisons_clone_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &SimpleAssignmentTarget<'_>,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  match target {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
      identifier_is_unresolved_clone(semantic, identifier)
    }
    SimpleAssignmentTarget::StaticMemberExpression(member) => unresolved_global_this_clone_member(
      semantic,
      &member.object,
      Some(member.property.name.as_str()),
      None,
      CloneKeyProof::Possible,
    ),
    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
      unresolved_global_this_clone_member(
        semantic,
        &member.object,
        None,
        Some(&member.expression),
        CloneKeyProof::Possible,
      )
    }
    SimpleAssignmentTarget::TSAsExpression(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    SimpleAssignmentTarget::TSNonNullExpression(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    SimpleAssignmentTarget::TSTypeAssertion(inner) => {
      expression_poisons_clone_intrinsic(semantic, &inner.expression, work)
    }
    SimpleAssignmentTarget::PrivateFieldExpression(_) => false,
  }
}

fn expression_poisons_clone_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  work: &WorkCounter,
) -> bool {
  work.add_queries(1);
  expression_matches_clone_intrinsic(semantic, expression, CloneKeyProof::Possible)
}

fn expression_matches_clone_intrinsic(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  proof: CloneKeyProof,
) -> bool {
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) => identifier_is_unresolved_clone(semantic, identifier),
    Expression::StaticMemberExpression(member) => unresolved_global_this_clone_member(
      semantic,
      &member.object,
      Some(member.property.name.as_str()),
      None,
      proof,
    ),
    Expression::ComputedMemberExpression(member) => unresolved_global_this_clone_member(
      semantic,
      &member.object,
      None,
      Some(&member.expression),
      proof,
    ),
    _ => false,
  }
}

fn identifier_is_unresolved_clone(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> bool {
  identifier.name.as_str() == "structuredClone" && is_unresolved_global(semantic, identifier)
}

fn unresolved_global_this_clone_member(
  semantic: &oxc_semantic::Semantic<'_>,
  object: &Expression<'_>,
  static_key: Option<&str>,
  computed_key: Option<&Expression<'_>>,
  proof: CloneKeyProof,
) -> bool {
  let Some(object) = object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  if object.name.as_str() != "globalThis" || !is_unresolved_global(semantic, object) {
    return false;
  }
  if let Some(name) = static_key {
    return name == "structuredClone";
  }
  let Some(key) = computed_key else {
    return false;
  };
  match static_string_key(key) {
    Some("structuredClone") => true,
    Some(_) => false,
    None => matches!(proof, CloneKeyProof::Possible),
  }
}

fn static_string_key<'a>(expression: &'a Expression<'a>) -> Option<&'a str> {
  match expression.get_inner_expression() {
    Expression::StringLiteral(literal) => Some(literal.value.as_str()),
    Expression::TemplateLiteral(literal)
      if literal.expressions.is_empty() && literal.quasis.len() == 1 =>
    {
      let quasi = literal.quasis.first()?;
      quasi.value.cooked.as_deref().or(Some(quasi.value.raw.as_str()))
    }
    _ => None,
  }
}

fn reference_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

fn intern_unresolved_global_name(name: &str) -> Option<&'static str> {
  intern_native_ctor(name).or_else(|| (name == "globalThis").then_some("globalThis"))
}

fn simple_binding_symbol(pattern: &BindingPattern<'_>) -> Option<SymbolId> {
  match pattern {
    BindingPattern::BindingIdentifier(identifier) => identifier.symbol_id.get(),
    BindingPattern::AssignmentPattern(assignment) => simple_binding_symbol(&assignment.left),
    _ => None,
  }
}

fn write_literal(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> WriteLiteral {
  match expression.get_inner_expression() {
    Expression::NumericLiteral(literal) => WriteLiteral::Number(literal.value.to_bits()),
    Expression::BooleanLiteral(literal) => WriteLiteral::Bool(literal.value),
    Expression::Identifier(identifier)
      if identifier.name.as_str() == "undefined"
        && reference_symbol(semantic, identifier).is_none() =>
    {
      WriteLiteral::Undefined
    }
    _ => WriteLiteral::Other,
  }
}

fn parse_watch_options(call: &CallExpression<'_>, api: Option<&str>) -> WatchConsumerOptions {
  let mut options = WatchConsumerOptions::default_for(api);
  let Some(index) = watch_options_index(api) else {
    return options;
  };
  if call.arguments.iter().take(index.saturating_add(1)).any(Argument::is_spread) {
    options.flush = FlushKind::Unknown;
    options.immediate = OptionFlag::Unknown;
    options.once = OptionFlag::Unknown;
    return options;
  }
  let Some(argument) = call.arguments.get(index).and_then(Argument::as_expression) else {
    return options;
  };
  let Expression::ObjectExpression(object) = argument.get_inner_expression() else {
    options.flush = FlushKind::Unknown;
    options.immediate = OptionFlag::Unknown;
    options.once = OptionFlag::Unknown;
    return options;
  };
  let mut keys_closed = true;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => {
        keys_closed = false;
      }
      ObjectPropertyKind::ObjectProperty(property) => {
        let Some(name) = property.key.static_name() else {
          keys_closed = false;
          continue;
        };
        if name == "flush" {
          options.flush = match property.value.get_inner_expression() {
            Expression::StringLiteral(literal) if literal.value.as_str() == "pre" => FlushKind::Pre,
            Expression::StringLiteral(literal) if literal.value.as_str() == "post" => {
              FlushKind::Post
            }
            Expression::StringLiteral(literal) if literal.value.as_str() == "sync" => {
              FlushKind::Sync
            }
            _ => FlushKind::Unknown,
          };
        } else if name == "immediate" {
          options.immediate = bool_option_flag(&property.value);
        } else if name == "once" {
          options.once = bool_option_flag(&property.value);
        } else {
          keys_closed = false;
        }
      }
    }
  }
  if !keys_closed {
    options.flush = FlushKind::Unknown;
    options.immediate = OptionFlag::Unknown;
    options.once = OptionFlag::Unknown;
  }
  if api == Some("watchPostEffect") {
    options.flush = FlushKind::Post;
  }
  if api == Some("watchSyncEffect") {
    options.flush = FlushKind::Sync;
  }
  options
}

fn watch_options_index(api: Option<&str>) -> Option<usize> {
  match api {
    Some("watch") => Some(2),
    Some("watchEffect" | "watchPostEffect" | "watchSyncEffect" | "effect") => Some(1),
    _ => None,
  }
}

fn bool_option_flag(expression: &Expression<'_>) -> OptionFlag {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) if literal.value => OptionFlag::On,
    Expression::BooleanLiteral(literal) if !literal.value => OptionFlag::Off,
    _ => OptionFlag::Unknown,
  }
}

fn member_is_write_context(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  span: Span,
) -> bool {
  match semantic.nodes().parent_kind(node_id) {
    AstKind::UpdateExpression(_) => true,
    AstKind::AssignmentExpression(assignment) => {
      assignment.left.span().start <= span.start && span.end <= assignment.left.span().end
    }
    AstKind::CallExpression(call) => {
      call.callee.span().start <= span.start && span.end <= call.callee.span().end
    }
    _ => false,
  }
}

fn mapped(
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  span: Span,
) -> SourceSpan {
  source_span(line_index, sfc_source, script_offset, span)
}

fn is_retained_vueuse_source_arg(info: Option<CallInfo>, index: usize) -> bool {
  index == 0
    && info.is_some_and(|call| {
      matches!(call.vueuse, Some("computedWithControl" | "controlledComputed")) && !call.has_spread
    })
}

fn array_is_controlled_source(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
  calls: &HashMap<u64, CallInfo>,
  work: &WorkCounter,
) -> bool {
  let array_span = semantic.nodes().kind(node_id).span();
  for _ in 0..ANCESTOR_BUDGET {
    work.add_queries(1);
    let parent = semantic.nodes().parent_id(node_id);
    match semantic.nodes().kind(parent) {
      wrapper if is_ts_wrapper(wrapper) => {
        node_id = parent;
      }
      AstKind::CallExpression(call) => {
        work.add_queries(1);
        let Some(info) = calls.get(&span_key(call.span)).copied() else {
          return false;
        };
        if !is_retained_vueuse_source_arg(Some(info), 0) {
          return false;
        }
        let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
          return false;
        };
        let inner = first.get_inner_expression().span();
        return inner == array_span || first.span() == array_span;
      }
      _ => return false,
    }
  }
  false
}

fn expression_static_key<'a>(expression: &'a Expression<'a>) -> Option<&'a str> {
  match expression.get_inner_expression() {
    Expression::StringLiteral(literal) => Some(literal.value.as_str()),
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
      literal.quasis.first().and_then(|quasi| quasi.value.cooked.as_deref())
    }
    _ => None,
  }
}

fn unresolved_native_ctor(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> bool {
  let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  matches!(
    identifier.name.as_str(),
    "Number" | "String" | "Boolean" | "BigInt" | "Object" | "Date" | "JSON"
  ) && reference_symbol(semantic, identifier).is_none()
}

fn peel_static_chain<'a>(
  expression: &'a Expression<'a>,
  work: &WorkCounter,
) -> Option<(&'a IdentifierReference<'a>, Vec<&'a str>)> {
  let mut current = expression.get_inner_expression();
  let mut keys = Vec::new();
  loop {
    work.add_queries(1);
    match current {
      Expression::StaticMemberExpression(member) => {
        if member.optional {
          return None;
        }
        work.add_queries(1);
        keys.push(member.property.name.as_str());
        current = member.object.get_inner_expression();
      }
      Expression::Identifier(identifier) => {
        work.add_queries(u64::try_from(keys.len()).unwrap_or(u64::MAX));
        keys.reverse();
        return Some((identifier, keys));
      }
      _ => return None,
    }
  }
}

fn peel_member_chain<'a>(
  expression: &'a Expression<'a>,
  work: &WorkCounter,
) -> Option<(&'a IdentifierReference<'a>, Vec<String>)> {
  let mut current = expression.get_inner_expression();
  let mut keys = Vec::new();
  loop {
    work.add_queries(1);
    match current {
      Expression::StaticMemberExpression(member) => {
        if member.optional {
          return None;
        }
        work.add_queries(1);
        keys.push(member.property.name.to_string());
        current = member.object.get_inner_expression();
      }
      Expression::ComputedMemberExpression(member) => {
        if member.optional {
          return None;
        }
        let key = computed_literal_key(&member.expression)?;
        work.add_queries(1);
        keys.push(key);
        current = member.object.get_inner_expression();
      }
      Expression::Identifier(identifier) => {
        work.add_queries(u64::try_from(keys.len()).unwrap_or(u64::MAX));
        keys.reverse();
        return Some((identifier, keys));
      }
      _ => return None,
    }
  }
}

fn poisons_json(object: &Expression<'_>, property: &str) -> bool {
  let Some(ident) = object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  ident.name.as_str() == "JSON" && matches!(property, "parse" | "stringify")
}

fn poisons_date_tojson(
  object: &Expression<'_>,
  property: &str,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  let Some(ident) = object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  ident.name.as_str() == "Date" && property == "toJSON" && symbol_of(ident).is_none()
}

fn poisons_string_capability(
  object: &Expression<'_>,
  property: &str,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  if !matches!(property, "getTime" | "getUTCFullYear") {
    return false;
  }
  let inner = object.get_inner_expression();
  if let Some(ident) = inner.get_identifier_reference() {
    return ident.name.as_str() == "String"
      && symbol_of(ident).is_none()
      && property == "prototype";
  }
  let Expression::StaticMemberExpression(member) = inner else {
    return false;
  };
  if member.property.name.as_str() != "prototype" {
    return false;
  }
  let Some(ident) = member.object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  ident.name.as_str() == "String" && symbol_of(ident).is_none()
}

fn poisons_native_date(
  object: &Expression<'_>,
  property: &str,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  let inner = object.get_inner_expression();
  if let Some(ident) = inner.get_identifier_reference() {
    return ident.name.as_str() == "Date" && symbol_of(ident).is_none() && property == "prototype";
  }
  let Expression::StaticMemberExpression(member) = inner else {
    return false;
  };
  if member.property.name.as_str() != "prototype" {
    return false;
  }
  let Some(ident) = member.object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  ident.name.as_str() == "Date" && symbol_of(ident).is_none()
}

fn computed_literal_key(expression: &Expression<'_>) -> Option<String> {
  match expression.get_inner_expression() {
    Expression::StringLiteral(literal) => Some(literal.value.to_string()),
    Expression::NumericLiteral(literal)
      if literal.value.fract() == 0.0 && literal.value >= 0.0 && literal.value < 1_000_000.0 =>
    {
      #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "array index keys are small non-negative integers"
      )]
      Some((literal.value as u64).to_string())
    }
    _ => None,
  }
}

fn prototype_receiver_is_native_ctor(
  semantic: &oxc_semantic::Semantic<'_>,
  object: &Expression<'_>,
  last_key: Option<&str>,
) -> bool {
  let inner = object.get_inner_expression();
  match inner {
    Expression::StaticMemberExpression(member) => {
      member.property.name.as_str() == "prototype"
        && unresolved_native_ctor(semantic, &member.object)
    }
    Expression::ComputedMemberExpression(member) => {
      expression_static_key(&member.expression) == Some("prototype")
        && unresolved_native_ctor(semantic, &member.object)
    }
    Expression::Identifier(_) => {
      last_key == Some("prototype") && unresolved_native_ctor(semantic, inner)
    }
    _ => false,
  }
}

fn is_object_define_property(callee: &Expression<'_>) -> bool {
  let Expression::StaticMemberExpression(member) = callee.get_inner_expression() else {
    return false;
  };
  if member.property.name.as_str() != "defineProperty" {
    return false;
  }
  member
    .object
    .get_inner_expression()
    .get_identifier_reference()
    .is_some_and(|identifier| identifier.name.as_str() == "Object")
}

fn nth_call_expr<'a>(call: &'a CallExpression<'a>, index: usize) -> Option<&'a Expression<'a>> {
  call.arguments.get(index).and_then(Argument::as_expression)
}

fn function_node(
  expression: &Expression<'_>,
  lookup: impl Fn(Span) -> Option<NodeId>,
) -> Option<NodeId> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => lookup(arrow.span),
    Expression::FunctionExpression(function) => lookup(function.span),
    _ => None,
  }
}

fn watch_option_flags(options: Option<&Expression<'_>>) -> (Option<bool>, Option<bool>, bool) {
  let Some(options) = options else {
    return (None, None, false);
  };
  let Expression::ObjectExpression(object) = options.get_inner_expression() else {
    return (None, None, true);
  };
  let mut once = None;
  let mut immediate = None;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return (None, None, true),
      ObjectPropertyKind::ObjectProperty(prop) => {
        if prop.kind != PropertyKind::Init || prop.method || prop.shorthand || prop.computed {
          return (None, None, true);
        }
        let Some(name) = prop.key.static_name() else {
          return (None, None, true);
        };
        match name.as_ref() {
          "once" => {
            if once.is_some() {
              return (None, None, true);
            }
            let Some(value) = bool_literal_value(&prop.value) else {
              return (None, None, true);
            };
            once = Some(value);
          }
          "immediate" => {
            if immediate.is_some() {
              return (None, None, true);
            }
            let Some(value) = bool_literal_value(&prop.value) else {
              return (None, None, true);
            };
            immediate = Some(value);
          }
          "flush" => {
            let Expression::StringLiteral(literal) = prop.value.get_inner_expression() else {
              return (None, None, true);
            };
            if !matches!(literal.value.as_str(), "pre" | "sync" | "post") {
              return (None, None, true);
            }
          }
          _ => return (None, None, true),
        }
      }
    }
  }
  (once, immediate, false)
}

fn bool_literal_value(expression: &Expression<'_>) -> Option<bool> {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(literal.value),
    _ => None,
  }
}

fn assigned_const_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
) -> Option<SymbolId> {
  let mut current = node_id;
  for _ in 0..6 {
    let parent_id = semantic.nodes().parent_id(current);
    match semantic.nodes().kind(parent_id) {
      wrapper if is_ts_wrapper(wrapper) => current = parent_id,
      AstKind::VariableDeclarator(declarator) => {
        let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          return None;
        };
        let symbol_id = binding.symbol_id.get()?;
        if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
          return None;
        }
        return Some(symbol_id);
      }
      _ => return None,
    }
  }
  None
}

fn stopped_child_symbol(
  _semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> Option<SymbolId> {
  let inner = expression.get_inner_expression();
  let body = match inner {
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.r#async || arrow.params.rest.is_some() || !arrow.params.items.is_empty() {
        return None;
      }
      &arrow.body
    }
    Expression::FunctionExpression(function) => {
      if function.r#async
        || function.generator
        || function.params.rest.is_some()
        || !function.params.items.is_empty()
      {
        return None;
      }
      function.body.as_ref()?
    }
    _ => return None,
  };
  if body.statements.len() != 1 {
    return None;
  }
  let oxc_ast::ast::Statement::ExpressionStatement(stmt) = body.statements.first()? else {
    return None;
  };
  let Expression::CallExpression(call) = stmt.expression.get_inner_expression() else {
    return None;
  };
  let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
    return None;
  };
  if member.property.name.as_str() != "stop" {
    return None;
  }
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  symbol_of(object)
}

fn intern_scope_method(name: &str) -> Option<&'static str> {
  match name {
    "pause" => Some("pause"),
    "resume" => Some("resume"),
    "stop" => Some("stop"),
    "run" => Some("run"),
    _ => None,
  }
}

mod map_index;
pub(super) use map_index::proxy_flavor;
use map_index::{
  array_is_map_entry, array_is_map_iterable, assignment_poisons_map_intrinsic,
  capability_mutating_callee, expression_poisons_map_intrinsic, known_map_constructor_key_role,
  literal_nullish, literal_truthy, object_has_skip_marker, simple_target_poisons_map_intrinsic,
  vue_wrapper_skips_capability,
};
