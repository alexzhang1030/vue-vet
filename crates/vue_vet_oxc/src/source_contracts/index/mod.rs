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

use super::atom::{PrimitiveAtom, atom_of_expression};
use super::class::{
  ClassNewInfo, ClassRecord, MemberRecord, analyze_class, class_binding_symbol, class_new_info,
};
use super::proof::{
  ANCESTOR_BUDGET, DemandOrigin, DemandRole, Reach, classify_reach, classify_reach_except_chain,
  classify_role, is_custom_prototype_key,
};
use super::shape::{
  CollectionCtor, Literal, PrimitiveAtom as ShapePrimitiveAtom, Scalar, ShapeHint, VueImport,
  VueUseImport, hint_of, intern_extractable_method, intern_native_ctor,
  is_actual_proxy_runtime_source, is_fresh_allocation, is_known_receiver_method,
  is_proxy_allocating_api, is_unresolved_date, literal_of, primitive_atom, resolve_vue_api,
  resolve_vueuse_api, scalar_of, span_key, unresolved_collection_kind,
};
use super::stats::WorkCounter;
use crate::facts::source_span;

const MAX_ALIAS_DEPTH: u8 = 8;
const MAX_ROLE_ANCESTORS: u8 = 8;
/// Structured logical/conditional/sequence/aggregate walk. Exhaustion does
/// not trust the leftover node; raising this bound only moves the defect.
const MAX_ESCAPE_DEPTH: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StmtSite {
  pub block: NodeId,
  pub callable: Option<NodeId>,
  pub expr_offset: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WriteLiteral {
  Number(u64),
  Bool(bool),
  Undefined,
  Other,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ValueWrite {
  pub offset: usize,
  pub callable: Option<NodeId>,
  pub block: NodeId,
  pub span: Span,
  pub rhs: Span,
  pub literal: WriteLiteral,
  pub simple_assign: bool,
  pub fresh_alloc: bool,
  pub node_id: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MemberWrite {
  pub offset: usize,
  pub callable: Option<NodeId>,
  pub block: NodeId,
  pub rhs: Span,
  pub simple_assign: bool,
  pub fresh_alloc: bool,
}

/// Direct `obj.prop = rhs` only. Assignment patterns restore generic
/// uncertainty on the member object.
#[derive(Clone, Copy)]
struct DirectMemberWrite<'a> {
  offset: usize,
  callable: Option<NodeId>,
  block: NodeId,
  right: &'a Expression<'a>,
  simple: bool,
  fresh: bool,
  span: Span,
  node_id: NodeId,
}

#[derive(Clone, Debug)]
pub(super) struct MemberInfo {
  pub object: Span,
  pub property: String,
  pub span: Span,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CallInfo {
  pub span: Span,
  pub api: Option<&'static str>,
  pub vueuse: Option<&'static str>,
  pub first_arg: Option<Span>,
  pub second_arg: Option<Span>,
  pub has_spread: bool,
  pub native_structured_clone: bool,
  /// True when a resolved proxy-allocating Vue API is imported from a Vue 3
  /// runtime that actually allocates a Proxy. Indexed from `vue_imports`;
  /// local/unknown calls never walk declaration ancestors.
  pub actual_proxy_origin: bool,
  pub arg_count: u8,
  pub third_arg: Option<Span>,
}

#[derive(Clone, Debug)]
pub(super) struct PathCall {
  pub keys: Vec<String>,
  pub method: String,
  pub site: MemberUse,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NestedWrite {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub rhs: Span,
  pub simple_assign: bool,
}

#[derive(Clone, Debug)]
pub(super) struct PathRead {
  pub keys: Vec<String>,
  pub site: MemberUse,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PathWrite {
  pub offset: usize,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub simple_assign: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SnapshotCall {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub reach: Reach,
  pub optional: bool,
}

impl SnapshotCall {
  pub(super) const fn from_call_use(call: CallUse) -> Self {
    Self {
      offset: call.offset,
      span: call.span,
      callable: call.callable,
      region: call.region,
      reach: call.reach,
      optional: call.optional,
    }
  }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct InjectionSite {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub block: NodeId,
  pub reach: Reach,
  pub optional: bool,
  pub argc: u8,
  pub has_spread: bool,
  pub payload: Option<Span>,
  pub factory: Option<bool>,
  pub node_id: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NativeSymbol {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum ProxyFlavor {
  Deep,
  Shallow,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MapKeyRef {
  pub span: Span,
  pub symbol: Option<SymbolId>,
  pub proxy_of: Option<SymbolId>,
  pub actual_proxy: bool,
  pub proxy_flavor: Option<ProxyFlavor>,
  pub is_literal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExecKind {
  Always,
  Never,
  Maybe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChainPlace {
  Inside,
  Outside,
  Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WrapperOrigin {
  None,
  Known(SymbolId),
  Unknown,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NewInfo {
  pub ctor: Option<&'static str>,
  #[expect(dead_code, reason = "constructor arity is recorded with key extraction")]
  pub first_arg: Option<Span>,
  #[expect(dead_code, reason = "constructor arity is recorded with key extraction")]
  pub has_spread: bool,
  #[expect(dead_code, reason = "constructor arity is recorded with key extraction")]
  pub second_arg: Option<Span>,
  #[expect(dead_code, reason = "constructor arity is recorded with key extraction")]
  pub arg_count: u8,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MemberCallSite {
  pub span: Span,
  pub node_id: NodeId,
  pub block: NodeId,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub offset: usize,
  pub optional: bool,
  pub has_spread: bool,
  pub arg_count: u8,
  pub first_key: Option<MapKeyRef>,
  pub first_key_unknown: bool,
  pub exec: ExecKind,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MapOp {
  pub method: Option<&'static str>,
  pub site: MemberCallSite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CtorKeyState {
  Unknown,
  Known(usize),
}

struct PendingMapKeyArg {
  receiver: SymbolId,
  method: &'static str,
  arg: SymbolId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ExtractedMethod {
  pub collection: SymbolId,
  pub method: &'static str,
  pub extract_span: Span,
  pub extract_offset: usize,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MemberUse {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub optional: bool,
  pub call_optional: bool,
  pub reach: Reach,
  pub role: DemandRole,
}

#[derive(Clone, Debug)]
pub(super) struct NamedUse {
  pub key: String,
  pub site: MemberUse,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CallUse {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub block: NodeId,
  pub optional: bool,
  pub reach: Reach,
  pub argc: u8,
  pub has_spread: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ResultDemand {
  pub member: String,
  pub site: MemberUse,
  pub inner: CallUse,
  pub block: NodeId,
}

#[derive(Clone, Debug)]
pub(super) struct ValueDemand {
  pub member: String,
  pub site: MemberUse,
  pub value_read: MemberUse,
  pub block: NodeId,
}

#[derive(Clone, Debug)]
pub(super) enum ObjectEntry {
  Spread,
  Computed,
  Accessor { name: Option<String> },
  Data { name: String, key: Span, value: Span },
  Method { name: String, key: Span, value: Span },
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ObjectProp {
  Unknown,
  Value(Span),
}

#[derive(Clone, Copy)]
pub(super) struct Owner {
  pub callable: Option<NodeId>,
  pub block: Option<NodeId>,
  pub region: Option<NodeId>,
}

#[derive(Clone, Debug)]
pub(super) struct FunctionInfo {
  #[expect(dead_code, reason = "node identity is stored for callback owner joins")]
  pub node_id: NodeId,
  pub params: Vec<Option<SymbolId>>,
  pub has_rest: bool,
  #[expect(dead_code, reason = "arrow expression flag is stored with the function index")]
  pub expression_arrow: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FlushKind {
  Pre,
  Post,
  Sync,
  Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OptionFlag {
  Default,
  On,
  Off,
  Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WatchConsumerOptions {
  pub flush: FlushKind,
  pub immediate: OptionFlag,
  pub once: OptionFlag,
}

impl WatchConsumerOptions {
  pub(super) fn default_for(api: Option<&str>) -> Self {
    match api {
      Some("watchPostEffect") => {
        Self { flush: FlushKind::Post, immediate: OptionFlag::Default, once: OptionFlag::Default }
      }
      Some("watchSyncEffect") => {
        Self { flush: FlushKind::Sync, immediate: OptionFlag::Default, once: OptionFlag::Default }
      }
      _ => {
        Self { flush: FlushKind::Pre, immediate: OptionFlag::Default, once: OptionFlag::Default }
      }
    }
  }

  /// `Some(true)` when the API is proven subscribed at the call site.
  /// `Some(false)` when the first run is post-flush or the watcher is already stopped.
  /// `None` when options are unknown.
  pub(super) fn immediately_active(self, api: Option<&str>) -> Option<bool> {
    if matches!(self.flush, FlushKind::Unknown)
      || matches!(self.immediate, OptionFlag::Unknown)
      || matches!(self.once, OptionFlag::Unknown)
    {
      return None;
    }
    let once = matches!(self.once, OptionFlag::On);
    let immediate = matches!(self.immediate, OptionFlag::On);
    match api {
      Some("watchPostEffect") => Some(false),
      Some("watchEffect" | "effect") => Some(!matches!(self.flush, FlushKind::Post)),
      Some("watchSyncEffect") => Some(true),
      Some("watch") => Some(!(once && immediate)),
      _ => None,
    }
  }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ArgUse {
  pub api: Option<&'static str>,
  pub index: u32,
  pub call_span: Span,
  pub offset: usize,
  pub node_id: NodeId,
  #[expect(dead_code, reason = "callable owner is stored for same-block consumer proof")]
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct IdentCall {
  #[expect(dead_code, reason = "call span is stored for stop/pause site identity")]
  pub span: Span,
  pub offset: usize,
  #[expect(dead_code, reason = "callable owner is stored for stop/pause ordering")]
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ValueRead {
  pub node_id: NodeId,
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

/// `.value` reads split by contract predicate. One static-member visit fills
/// the lanes; each predicate stays the one that lane had before they shared a visit.
#[derive(Default)]
struct ValueReadLanes {
  derivation: HashMap<SymbolId, Vec<ValueRead>>,
  scheduling: HashMap<SymbolId, Vec<ValueRead>>,
  custom_ref: HashMap<SymbolId, Vec<ValueRead>>,
  scheduling_calls: HashMap<SymbolId, Vec<MemberCall>>,
}

impl ValueReadLanes {
  fn record_custom_ref(&mut self, symbol_id: SymbolId, read: ValueRead) {
    self.custom_ref.entry(symbol_id).or_default().push(read);
  }

  fn record_derivation(&mut self, root: SymbolId, read: ValueRead) {
    self.derivation.entry(root).or_default().push(read);
  }

  fn record_scheduling(&mut self, root: SymbolId, read: ValueRead) {
    self.scheduling.entry(root).or_default().push(read);
  }

  fn record_scheduling_call(&mut self, root: SymbolId, call: MemberCall) {
    self.scheduling_calls.entry(root).or_default().push(call);
  }

  fn sort(&mut self, work: &WorkCounter) {
    let mut derivation = std::mem::take(&mut self.derivation);
    for bucket in derivation.values_mut() {
      bucket.sort_by(|left, right| {
        work.add_queries(1);
        left.offset.cmp(&right.offset)
      });
    }
    self.derivation = derivation;
    for reads in self.scheduling.values_mut() {
      work.add_queries(reads.len() as u64);
      reads.sort_by_key(|read| read.offset);
    }
    for calls in self.scheduling_calls.values_mut() {
      work.add_queries(calls.len() as u64);
      calls.sort_by_key(|call| call.offset);
    }
    for reads in self.custom_ref.values_mut() {
      reads.sort_by_key(|read| read.offset);
    }
  }

  fn remap_roots(&mut self, mut root_of: impl FnMut(SymbolId) -> SymbolId, mut note: impl FnMut()) {
    remap_symbol_vec_map(&mut self.custom_ref, &mut root_of, &mut note);
    remap_symbol_vec_map(&mut self.derivation, &mut root_of, &mut note);
    remap_symbol_vec_map(&mut self.scheduling, &mut root_of, &mut note);
    remap_symbol_vec_map(&mut self.scheduling_calls, &mut root_of, &mut note);
  }
}

fn remap_symbol_vec_map<T>(
  map: &mut HashMap<SymbolId, Vec<T>>,
  root_of: &mut impl FnMut(SymbolId) -> SymbolId,
  note: &mut impl FnMut(),
) {
  let taken = std::mem::take(map);
  for (symbol_id, mut values) in taken {
    note();
    map.entry(root_of(symbol_id)).or_default().append(&mut values);
  }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MemberCall {
  pub offset: usize,
  pub method: &'static str,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AwaitSite {
  pub offset: usize,
  pub callee_api: Option<&'static str>,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

/// Until-demand await site: operand span, bound symbol, region, and reach.
#[derive(Clone, Copy, Debug)]
pub(super) struct UntilAwaitSite {
  pub offset: usize,
  pub end: usize,
  pub span: Span,
  pub argument: Span,
  pub bound: Option<SymbolId>,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub reach: Reach,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct UntilEscapeSite {
  pub offset: usize,
  pub until_borrow: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct UntilClosedSource {
  pub init_span: Span,
  pub closed: bool,
}

#[derive(Default)]
struct UntilIndexes {
  await_method_calls: HashMap<u64, Vec<NamedUse>>,
  result_method_calls: HashMap<SymbolId, Vec<NamedUse>>,
  results_by_await: HashMap<u64, SymbolId>,
  awaits: Vec<UntilAwaitSite>,
  await_by_argument: HashMap<u64, UntilAwaitSite>,
  await_by_bound: HashMap<SymbolId, Vec<UntilAwaitSite>>,
  awaits_by_region: HashMap<(Option<NodeId>, NodeId), Vec<UntilAwaitSite>>,
  escapes: HashMap<SymbolId, Vec<UntilEscapeSite>>,
  closed_sources: HashMap<SymbolId, UntilClosedSource>,
  scalars: HashMap<u64, Scalar>,
  uncertain_value_writes: HashSet<SymbolId>,
  pending_borrow: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DisposeSite {
  pub offset: usize,
  pub span: Span,
  pub child: Option<SymbolId>,
  pub callable: Option<NodeId>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WatchConsumer {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub source: SymbolId,
  pub once: Option<bool>,
  pub immediate: Option<bool>,
  pub options_unknown: bool,
  pub handle: Option<SymbolId>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct EffectCallback {
  pub offset: usize,
  pub api: &'static str,
}

#[expect(
  clippy::struct_excessive_bools,
  reason = "clone/map intrinsic poison, prototype mutation, and unresolved origin touch are independent whole-file proofs"
)]
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
  pub tainted_ctors: HashSet<&'static str>,
  pub extracted_methods: HashMap<SymbolId, ExtractedMethod>,
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
  pub awaits: Vec<AwaitSite>,
  pub awaits_by_callable: HashMap<Option<NodeId>, Vec<AwaitSite>>,
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
  pub objects: HashMap<u64, Vec<ObjectEntry>>,
  pub object_props: HashMap<u64, HashMap<String, ObjectProp>>,
  /// Inner object/array span for a (possibly asserted) expression span.
  pub literal_span: HashMap<u64, Span>,
  /// Array expression spans whose elements include a spread.
  pub array_spread: HashSet<u64>,
  pub collections: HashMap<u64, CollectionCtor>,
  pub clone_intrinsic_poisoned: bool,
  pub arrays: HashSet<u64>,
  pub closed_objects: HashMap<u64, bool>,
  pub capability_uncertain: HashSet<SymbolId>,
  pub stmt_site: HashMap<NodeId, StmtSite>,
  pub events_by_block: HashMap<NodeId, Vec<usize>>,
  pub control_events_by_block: HashMap<NodeId, Vec<usize>>,
  pub pause_events_by_block: HashMap<NodeId, Vec<usize>>,
  pub init_span: HashMap<SymbolId, Span>,
  pub value_write_roots: HashSet<SymbolId>,
  pub member_write_roots: HashSet<SymbolId>,
  pub prototype_mutated: bool,
  pub shadowed_ctors: HashSet<&'static str>,
  pub stop_offsets: Vec<usize>,
  pub producer_call_offsets: Vec<usize>,
  pub allowed_offsets: Vec<usize>,
  pub functions: HashMap<u64, FunctionInfo>,
  pub function_by_node: HashMap<NodeId, Span>,
  pub effect_calls: HashMap<u64, ArgUse>,
  pub watch_getters: HashMap<u64, ArgUse>,
  pub watch_options: HashMap<u64, WatchConsumerOptions>,
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
  barriers_by_region: HashMap<NodeId, Vec<usize>>,
  closed_keys: HashMap<u64, HashSet<String>>,
  owners: HashMap<NodeId, Owner>,
  global_this_aliases: HashSet<SymbolId>,
  native_ctor_aliases: HashMap<SymbolId, &'static str>,
  terminations_by_callable: HashMap<Option<NodeId>, Vec<usize>>,
  pub object_literals: HashSet<u64>,
  pub news: HashMap<u64, NewInfo>,
  pub classes: HashMap<SymbolId, ClassRecord>,
  pub class_news: HashMap<u64, ClassNewInfo>,
  pub prototype_touch: HashSet<SymbolId>,
  pub map_init_keys: HashMap<u64, Option<Vec<MapKeyRef>>>,
  pub member_calls: HashMap<(SymbolId, String), Vec<MemberCallSite>>,
  pub member_call_by_node: HashMap<NodeId, MemberCallSite>,
  pub map_ops: HashMap<SymbolId, Vec<MapOp>>,
  pub map_gets: HashMap<SymbolId, Vec<MemberCallSite>>,
  pub proxy_wrapper: HashMap<SymbolId, Span>,
  wrappers_of_alloc: HashMap<SymbolId, Vec<SymbolId>>,
  pub skip_marker_objects: HashSet<u64>,
  pub skip_written: HashSet<SymbolId>,
  pub map_intrinsic_poisoned: bool,
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
  until: UntilIndexes,
  native_symbols: HashMap<SymbolId, NativeSymbol>,
  provides_by_key: HashMap<SymbolId, Vec<InjectionSite>>,
  injects_by_key: HashMap<SymbolId, Vec<InjectionSite>>,
  script_kind: ScriptKind,
  pub(super) dates: HashSet<u64>,
  pub(super) date_poisoned: bool,
  pub(super) json_poisoned: bool,
  pub(super) string_capability_poisoned: bool,
  literals: HashMap<u64, Literal>,
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
      tainted_ctors: HashSet::new(),
      extracted_methods: HashMap::new(),
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
      awaits: Vec::new(),
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
      objects: HashMap::new(),
      object_props: HashMap::new(),
      literal_span: HashMap::new(),
      array_spread: HashSet::new(),
      collections: HashMap::new(),
      clone_intrinsic_poisoned: false,
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
      prototype_mutated: false,
      shadowed_ctors: HashSet::new(),
      stop_offsets: Vec::new(),
      producer_call_offsets: Vec::new(),
      allowed_offsets: Vec::new(),
      functions: HashMap::new(),
      function_by_node: HashMap::new(),
      effect_calls: HashMap::new(),
      watch_getters: HashMap::new(),
      watch_options: HashMap::new(),
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
      native_ctor_aliases: HashMap::new(),
      terminations_by_callable: HashMap::new(),
      object_literals: HashSet::new(),
      news: HashMap::new(),
      classes: HashMap::new(),
      class_news: HashMap::new(),
      prototype_touch: HashSet::new(),
      map_init_keys: HashMap::new(),
      member_calls: HashMap::new(),
      member_call_by_node: HashMap::new(),
      map_ops: HashMap::new(),
      map_gets: HashMap::new(),
      proxy_wrapper: HashMap::new(),
      wrappers_of_alloc: HashMap::new(),
      skip_marker_objects: HashSet::new(),
      skip_written: HashSet::new(),
      map_intrinsic_poisoned: false,
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
      until: UntilIndexes::default(),
      native_symbols: HashMap::new(),
      provides_by_key: HashMap::new(),
      injects_by_key: HashMap::new(),
      script_kind: kind,
      dates: HashSet::new(),
      date_poisoned: false,
      json_poisoned: false,
      string_capability_poisoned: false,
      literals: HashMap::new(),
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
        work.sort_by_key(events, |offset| *offset);
        events.dedup();
      }
    }
    for offsets in self.terminations_by_callable.values_mut() {
      offsets.sort_unstable();
    }
    for barriers in self.barriers_by_region.values_mut() {
      barriers.sort_unstable();
    }
    for stops in self.stops_by_region.values_mut() {
      stops.sort_by_key(|stop| stop.offset);
    }
    for events in self.control_events_by_block.values_mut() {
      events.sort_unstable();
    }
    for events in self.pause_events_by_block.values_mut() {
      events.sort_unstable();
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
      uses.sort_by_key(|use_site| use_site.offset);
    }
    self.reads.sort(&self.work);
    for awaits in self.awaits_by_callable.values_mut() {
      self.work.add_queries(awaits.len() as u64);
      awaits.sort_by_key(|site| site.offset);
    }
    self.work.add_queries(self.awaits.len() as u64);
    self.awaits.sort_by_key(|site| site.offset);
    self.until.awaits.sort_by_key(|site| site.offset);
    for uses in self.until.await_method_calls.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.offset);
    }
    for uses in self.until.result_method_calls.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.offset);
    }
    for sites in self.until.escapes.values_mut() {
      sites.sort_by_key(|site| site.offset);
    }
    for sites in self.until.awaits_by_region.values_mut() {
      sites.sort_by_key(|site| site.offset);
    }
    for sites in self.until.await_by_bound.values_mut() {
      sites.sort_by_key(|site| site.offset);
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
      uses.sort_by_key(|use_site| use_site.site.offset);
    }
    for uses in self.chained_value_by_root.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.offset);
    }
    for uses in self.member_calls_by_root.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.offset);
    }
    for sites in self.provides_by_key.values_mut() {
      sites.sort_by_key(|site| site.offset);
    }
    for sites in self.injects_by_key.values_mut() {
      sites.sort_by_key(|site| site.offset);
    }
    for uses in self.identifier_calls.values_mut() {
      uses.sort_by_key(|use_site| use_site.offset);
    }
    for uses in self.result_demands.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.offset);
    }
    for uses in self.value_demands.values_mut() {
      uses.sort_by_key(|use_site| use_site.site.offset);
    }
    self.work.sort_by_key(&mut self.stop_offsets, |offset| *offset);
    self.stop_offsets.dedup();
    self.work.sort_by_key(&mut self.producer_call_offsets, |offset| *offset);
    self.producer_call_offsets.dedup();
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
      calls.sort_by_key(|call| call.site.offset);
    }
    for reads in self.path_reads.values_mut() {
      reads.sort_by_key(|read| read.site.offset);
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
    let mut allowed = Vec::with_capacity(
      self
        .producer_call_offsets
        .len()
        .saturating_add(self.stop_offsets.len())
        .saturating_add(demand_len),
    );
    allowed.extend_from_slice(&self.producer_call_offsets);
    allowed.extend_from_slice(&self.stop_offsets);
    for uses in self.result_demands.values() {
      for demand in uses {
        allowed.push(demand.site.offset);
      }
    }
    for uses in self.value_demands.values() {
      for demand in uses {
        allowed.push(demand.site.offset);
      }
    }
    self.work.add_queries(u64::try_from(allowed.len()).unwrap_or(u64::MAX));
    self.work.sort_by_key(&mut allowed, |offset| *offset);
    allowed.dedup();
    self.allowed_offsets = allowed;
  }

  pub(super) fn has_foreign_event_between(&self, block: NodeId, start: usize, end: usize) -> bool {
    let Some(events) = self.events_by_block.get(&block) else {
      self.work.add_queries(1);
      return false;
    };
    let event_count = self.work.exclusive_offsets(events, start, end).len();
    self.work.add_queries(1);
    let allowed_count = self.work.exclusive_offsets(&self.allowed_offsets, start, end).len();
    self.work.add_queries(1);
    event_count > allowed_count
  }

  pub(super) fn sort_timeline<T, K, F>(&self, items: &mut [T], key: F)
  where
    K: Ord,
    F: FnMut(&T) -> K,
  {
    self.work.sort_by_key(items, key);
  }

  pub(super) fn identifier_calls_on(&self, root: SymbolId) -> &[CallUse] {
    self.work.add_queries(1);
    self.identifier_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn result_demands_on(&self, root: SymbolId) -> &[ResultDemand] {
    self.work.add_queries(1);
    self.result_demands.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn value_demands_on(&self, root: SymbolId) -> &[ValueDemand] {
    self.work.add_queries(1);
    self.value_demands.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn value_writes_on(&self, root: SymbolId) -> &[ValueWrite] {
    self.value_writes_of(root)
  }

  pub(super) fn value_reads_on(&self, root: SymbolId) -> &[MemberUse] {
    self.work.add_queries(1);
    self.value_reads.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn class_identity_intact(&self, class: SymbolId) -> bool {
    self.work.add_queries(1);
    let Some(record) = self.classes.get(&class) else {
      return false;
    };
    record.ordinary
      && !self.reassigned.contains(&class)
      && !self.escaped.contains(&class)
      && !self.prototype_touch.contains(&class)
      && !self.unknown_member_touch.contains(&class)
      && !self.capability_touch.contains(&class)
  }

  pub(super) fn class_declared_before(&self, class: SymbolId, new_span: Span) -> bool {
    self.work.add_queries(1);
    self.classes.get(&class).is_some_and(|record| record.span.start < new_span.start)
  }

  pub(super) fn class_member(&self, class: SymbolId, name: &str) -> Option<&MemberRecord> {
    self.work.add_queries(1);
    self.work.add_key_lookups(1);
    self.classes.get(&class).and_then(|record| record.members.get(name))
  }

  pub(super) fn new_at(&self, span: Span) -> Option<ClassNewInfo> {
    self.work.add_queries(1);
    self.class_news.get(&span_key(span)).copied()
  }

  pub(super) fn is_optional_chain(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
  ) -> bool {
    chain_optional(semantic, node_id, &self.work)
  }

  pub(super) fn ctor_shadowed(&self, name: &str) -> bool {
    self.work.add_queries(1);
    self.shadowed_ctors.contains(name)
  }

  pub(super) fn native_capability_intact(&self) -> bool {
    self.work.add_queries(1);
    !self.prototype_mutated
      && !self.ctor_shadowed("String")
      && !self.ctor_shadowed("Number")
      && !self.ctor_shadowed("Boolean")
      && !self.ctor_shadowed("BigInt")
      && !self.ctor_shadowed("Object")
      && !self.ctor_shadowed("Symbol")
  }

  pub(super) fn setup_lane(&self) -> bool {
    self.work.add_queries(1);
    self.script_kind == ScriptKind::Setup
  }

  /// Optional member (`count?.toFixed`) guards only a nullish fallback.
  /// Optional *call* (`toFixed?.()`) always guards.
  pub(super) fn injection_demand_from(
    &self,
    site: &MemberUse,
    origin: DemandOrigin,
    fallback: super::shape::PrimitiveKind,
  ) -> bool {
    self.work.add_queries(1);
    if !site.reach.is_straight() || site.call_optional {
      return false;
    }
    if site.optional && fallback == super::shape::PrimitiveKind::Nullish {
      return false;
    }
    site.callable == origin.callable
      && site.region == origin.region
      && self.until_interval_open(origin.callable, origin.region, origin.offset, site.offset)
  }

  pub(super) fn inject_site(
    &self,
    key: SymbolId,
    offset: usize,
    node_id: NodeId,
  ) -> Option<InjectionSite> {
    let sites = self.injects_on(key);
    let index = self.work.partition_point(sites, |site| site.offset < offset);
    self.work.add_queries(1);
    sites.get(index).copied().filter(|site| site.node_id == node_id)
  }

  pub(super) fn native_symbol(&self, root: SymbolId) -> Option<NativeSymbol> {
    self.work.add_queries(1);
    self.native_symbols.get(&root).copied()
  }

  pub(super) fn provides_on(&self, root: SymbolId) -> &[InjectionSite] {
    self.work.add_queries(1);
    self.provides_by_key.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn injects_on(&self, root: SymbolId) -> &[InjectionSite] {
    self.work.add_queries(1);
    self.injects_by_key.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn result_binding_intact(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    !self.reassigned.contains(&root)
      && !self.escaped.contains(&root)
      && !self.unknown_member_touch.contains(&root)
      && !self.capability_touch.contains(&root)
  }

  pub(super) fn function(&self, span: Span) -> Option<&FunctionInfo> {
    self.work.add_queries(1);
    self.functions.get(&span_key(span))
  }

  pub(super) fn arg_uses_of(&self, root: SymbolId) -> &[ArgUse] {
    self.work.add_queries(1);
    self.arg_uses.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn value_reads_of(&self, root: SymbolId) -> &[ValueRead] {
    self.work.add_queries(1);
    self.reads.custom_ref.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn value_writes_of(&self, root: SymbolId) -> &[ValueWrite] {
    self.work.add_queries(1);
    self.value_writes.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn value_write_events(&self, root: SymbolId) -> &[ValueWrite] {
    self.work.add_queries(1);
    self.value_write_events.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn value_writes_for(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
  ) -> Option<&[ValueWrite]> {
    self.work.add_queries(1);
    self.value_writes_by_callable.get(&(root, callable)).map(Vec::as_slice)
  }

  pub(super) fn last_value_write_in(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    before: usize,
  ) -> Option<ValueWrite> {
    let writes = self.value_writes_for(root, callable)?;
    let end = self.work.partition_point(writes, |write| write.offset < before);
    self.work.add_queries(1);
    end.checked_sub(1).and_then(|index| writes.get(index)).copied()
  }

  pub(super) fn has_simple_value_write_between(
    &self,
    root: SymbolId,
    start: usize,
    end: usize,
  ) -> bool {
    let Some(writes) = self.value_writes.get(&root) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(writes, |write| write.offset <= start);
    writes.get(index..).is_some_and(|rest| {
      rest.iter().any(|write| {
        self.work.add_queries(1);
        write.offset < end && write.simple_assign
      })
    })
  }

  pub(super) fn call_result(&self, call_span: Span) -> Option<SymbolId> {
    self.work.add_queries(1);
    self.call_results.get(&span_key(call_span)).copied()
  }

  pub(super) fn watch_options_of(
    &self,
    call_span: Span,
    api: Option<&str>,
  ) -> WatchConsumerOptions {
    self.work.add_queries(1);
    self
      .watch_options
      .get(&span_key(call_span))
      .copied()
      .unwrap_or_else(|| WatchConsumerOptions::default_for(api))
  }

  pub(super) fn symbols_of_root(&self, root: SymbolId) -> &[SymbolId] {
    self.work.add_queries(1);
    self.root_members.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn first_inactivity_after(
    &self,
    handle_root: SymbolId,
    block: NodeId,
    after: usize,
  ) -> Option<usize> {
    let Some(events) = self.inactivity_by_handle_block.get(&(handle_root, block)) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(events, |event| event.offset <= after);
    self.work.add_queries(1);
    events.get(index).map(|event| event.offset)
  }

  pub(super) fn has_member_mutation(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.member_write_roots.contains(&root) || self.unknown_member_touch.contains(&root)
  }

  pub(super) const fn stats(&self) -> super::stats::SourceContractStats {
    self.work.snapshot()
  }

  pub(super) fn root_of(&self, symbol_id: SymbolId) -> SymbolId {
    self.alias_root.get(&symbol_id).copied().unwrap_or(symbol_id)
  }

  pub(super) fn alias_members(&self, root: SymbolId) -> &[SymbolId] {
    self.work.add_queries(1);
    self.aliases_of.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn payload_uncertain(&self, symbol_id: SymbolId) -> bool {
    let root = self.root_of(symbol_id);
    self.uncertain.contains(&root)
      || self.escaped.contains(&root)
      || self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
  }

  pub(super) fn toref_identity_unproven(&self, symbol_id: SymbolId) -> bool {
    let root = self.root_of(symbol_id);
    self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
      || self.toref_identity_uncertain.contains(&root)
      || self.toref_helper_escape.contains(&root)
  }

  pub(super) fn has_pause_between(&self, block: NodeId, start: usize, end: usize) -> bool {
    let Some(events) = self.pause_events_by_block.get(&block) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(events, |offset| *offset <= start);
    self.work.add_queries(1);
    events.get(index).is_some_and(|offset| *offset < end)
  }

  pub(super) fn next_control_after(&self, block: NodeId, offset: usize) -> usize {
    let Some(events) = self.control_events_by_block.get(&block) else {
      self.work.add_queries(1);
      return usize::MAX;
    };
    let index = self.work.partition_point(events, |event| *event <= offset);
    self.work.add_queries(1);
    events.get(index).copied().unwrap_or(usize::MAX)
  }

  pub(super) fn has_control_event_between(&self, block: NodeId, start: usize, end: usize) -> bool {
    let Some(events) = self.control_events_by_block.get(&block) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(events, |offset| *offset <= start);
    self.work.add_queries(1);
    events.get(index).is_some_and(|offset| *offset < end)
  }

  pub(super) fn has_event_between(&self, block: NodeId, start: usize, end: usize) -> bool {
    let Some(events) = self.events_by_block.get(&block) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(events, |offset| *offset <= start);
    self.work.add_queries(1);
    events.get(index).is_some_and(|offset| *offset < end)
  }

  pub(super) fn first_value_write_after(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueWrite> {
    if self.mixed_value_owners.contains(&root) {
      self.work.add_queries(1);
      return None;
    }
    let Some(writes) = self.value_writes.get(&root) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(writes, |write| write.offset <= offset);
    self.work.add_queries(1);
    let next = writes.get(index).copied()?;
    (next.simple_assign && next.fresh_alloc && next.callable == callable && next.block == block)
      .then_some(next)
  }

  pub(super) fn last_value_write_before(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueWrite> {
    if self.mixed_value_owners.contains(&root) {
      self.work.add_queries(1);
      return None;
    }
    let Some(writes) = self.value_writes.get(&root) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(writes, |write| write.offset < offset);
    self.work.add_queries(1);
    let prior = index.checked_sub(1).and_then(|index| writes.get(index)).copied()?;
    (prior.callable == callable && prior.block == block).then_some(prior)
  }

  pub(super) fn value_writes_mixed(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.mixed_value_owners.contains(&root)
  }

  pub(super) fn first_value_read_after(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueRead> {
    self.work.add_queries(1);
    let reads = self.reads.derivation.get(&root).map_or(&[][..], Vec::as_slice);
    let index = self.work.partition_point(reads, |read| read.offset <= offset);
    reads.get(index..).into_iter().flatten().copied().find(|read| {
      self.work.add_queries(1);
      read.callable == callable && read.block == block
    })
  }

  pub(super) fn site_owner(&self, node_id: NodeId) -> (Option<NodeId>, Option<NodeId>) {
    self.work.add_queries(1);
    let owner = self.owner(node_id);
    (owner.callable, owner.block)
  }

  pub(super) fn primitive_at(&self, span: Span) -> Option<ShapePrimitiveAtom> {
    self.work.add_queries(1);
    self.primitives.get(&span_key(span)).copied()
  }

  pub(super) fn atoms_object_is(
    &self,
    left: ShapePrimitiveAtom,
    right: ShapePrimitiveAtom,
  ) -> bool {
    self.work.add_queries(1);
    left.object_is(right, &self.interned)
  }

  pub(super) fn atoms_js_strict_eq(
    &self,
    left: ShapePrimitiveAtom,
    right: ShapePrimitiveAtom,
  ) -> bool {
    self.work.add_queries(1);
    left.js_strict_eq(right, &self.interned)
  }

  pub(super) fn scheduling_value_reads_of(&self, root: SymbolId) -> &[ValueRead] {
    self.work.add_queries(1);
    self.reads.scheduling.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn member_calls_of(&self, root: SymbolId) -> &[MemberCall] {
    self.work.add_queries(1);
    self.reads.scheduling_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn awaits_of(&self, callable: Option<NodeId>) -> &[AwaitSite] {
    self.work.add_queries(1);
    self.awaits_by_callable.get(&callable).map_or(&[], Vec::as_slice)
  }

  /// Straight-line awaits inside `callable`. Ignore-window proof uses this
  /// list, not `has_barrier_between`, so an updater `await` stays a signal
  /// rather than a stack-wide barrier that would hide the later write.
  pub(super) fn straight_awaits_in(
    &self,
    callable: Option<NodeId>,
  ) -> impl Iterator<Item = UntilAwaitSite> + '_ {
    self.work.add_queries(1);
    self.until.awaits.iter().copied().filter(move |site| {
      self.work.add_queries(1);
      site.callable == callable && site.reach.is_straight()
    })
  }

  pub(super) fn function_id(&self, span: Span) -> Option<NodeId> {
    self.work.add_queries(1);
    self.callables.get(&span_key(span)).copied()
  }

  pub(super) fn scalar(&self, span: Span) -> Option<Scalar> {
    self.work.add_queries(1);
    self.until.scalars.get(&span_key(span)).copied()
  }

  pub(super) fn call_bindings(&self, call_span: Span) -> &[(SymbolId, String)] {
    self.work.add_queries(1);
    self.call_destructure.get(&span_key(call_span)).map_or(&[], Vec::as_slice)
  }

  pub(super) fn call_info(&self, span: Span) -> Option<CallInfo> {
    self.work.add_queries(1);
    self.calls.get(&span_key(span)).copied()
  }

  pub(super) fn until_scalar(&self, span: Span) -> Option<Scalar> {
    self.work.add_queries(1);
    self.until.scalars.get(&span_key(span)).copied()
  }

  pub(super) fn until_closed_source(&self, root: SymbolId) -> Option<UntilClosedSource> {
    self.work.add_queries(1);
    self.until.closed_sources.get(&root).copied()
  }

  pub(super) fn result_of_call(&self, span: Span) -> Option<SymbolId> {
    self.work.add_queries(1);
    self.call_results.get(&span_key(span)).copied()
  }

  pub(super) fn path_calls_on(&self, root: SymbolId) -> &[PathCall] {
    self.work.add_queries(1);
    self.path_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn path_reads_on(&self, root: SymbolId) -> &[PathRead] {
    self.work.add_queries(1);
    self.path_reads.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn path_value_writes_on(&self, root: SymbolId) -> &[(Vec<String>, PathWrite)] {
    self.work.add_queries(1);
    self.path_value_writes.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn nested_writes_on(&self, root: SymbolId) -> &[(String, NestedWrite)] {
    self.work.add_queries(1);
    self.nested_writes.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn literal_at(&self, span: Span) -> Option<Literal> {
    self.work.add_queries(1);
    self.literals.get(&span_key(span)).copied()
  }

  pub(super) fn object_is_closed_data(&self, span: Span) -> bool {
    self.work.add_queries(1);
    let Some(entries) = self.objects.get(&span_key(span)) else {
      return false;
    };
    entries.iter().all(|entry| {
      self.work.add_object_entries(1);
      matches!(entry, ObjectEntry::Data { .. })
    })
  }

  pub(super) fn object_has_key_named(&self, span: Span, name: &str) -> bool {
    self.work.add_queries(1);
    let Some(entries) = self.objects.get(&span_key(span)) else {
      return false;
    };
    entries.iter().any(|entry| {
      self.work.add_object_entries(1);
      match entry {
        ObjectEntry::Data { name: key, .. } | ObjectEntry::Accessor { name: Some(key) } => {
          key == name
        }
        _ => false,
      }
    })
  }

  #[expect(dead_code, reason = "array literals remain indexed for closed-object proofs")]
  pub(super) fn array_elements(&self, span: Span) -> Option<&[Span]> {
    self.work.add_queries(1);
    self.array_elements.get(&span_key(span)).map(Vec::as_slice)
  }

  pub(super) fn is_native_date(&self, span: Span) -> bool {
    self.work.add_queries(1);
    !self.date_poisoned && self.dates.contains(&span_key(span))
  }

  pub(super) fn ident_demand(&self, site: &SnapshotCall, origin: DemandOrigin) -> bool {
    self.work.add_queries(1);
    site.reach.is_straight()
      && !site.optional
      && site.callable == origin.callable
      && site.region == origin.region
      && !self.has_barrier_between(origin.region, origin.offset, site.offset)
  }

  pub(super) fn nested_demand(&self, write: &NestedWrite, origin: DemandOrigin) -> bool {
    self.work.add_queries(1);
    write.simple_assign
      && write.callable == origin.callable
      && write.region == origin.region
      && write.offset > origin.offset
      && !self.has_barrier_between(origin.region, origin.offset, write.offset)
  }

  pub(super) fn until_result_of_await(&self, span: Span) -> Option<SymbolId> {
    self.work.add_queries(1);
    self.until.results_by_await.get(&span_key(span)).copied()
  }

  pub(super) fn until_await_for_argument(&self, argument: Span) -> Option<UntilAwaitSite> {
    self.work.add_queries(1);
    self.until.await_by_argument.get(&span_key(argument)).copied()
  }

  pub(super) fn until_awaits_for_bound(&self, root: SymbolId) -> &[UntilAwaitSite] {
    self.work.add_queries(1);
    self.until.await_by_bound.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn until_awaits_by_region(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
  ) -> &[UntilAwaitSite] {
    self.work.add_queries(1);
    self.until.awaits_by_region.get(&(callable, region)).map_or(&[], Vec::as_slice)
  }

  pub(super) fn until_has_await_between(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    let sites = self.until_awaits_by_region(callable, region);
    let from = self.work.partition_point(sites, |site| site.offset <= start);
    sites.get(from..).is_some_and(|rest| rest.iter().any(|site| site.offset < end))
  }

  /// Later `await` expressions are stack-wide barriers. An until timeout
  /// result stays the same value across a later await, so those offsets
  /// must not hide a demand on an earlier settle.
  pub(super) fn until_demand_from(&self, site: &MemberUse, origin: DemandOrigin) -> bool {
    self.demand_ok(site)
      && site.callable == origin.callable
      && site.region == origin.region
      && self.until_interval_open(origin.callable, origin.region, origin.offset, site.offset)
  }

  pub(super) fn until_interval_open(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    if !self.has_barrier_between(region, start, end) {
      return true;
    }
    self.until_has_await_between(callable, region, start, end)
      && !self.until_non_await_barrier_between(callable, region, start, end)
  }

  pub(super) fn until_non_await_barrier_between(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    if end <= start {
      self.work.add_queries(1);
      return false;
    }
    let Some(barriers) = self.barriers_by_region.get(&region) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(barriers, |offset| *offset <= start);
    let Some(rest) = barriers.get(index..) else {
      return false;
    };
    rest.iter().any(|offset| {
      self.work.add_queries(1);
      *offset < end && !self.until_is_await_offset(callable, region, *offset)
    })
  }

  fn until_is_await_offset(&self, callable: Option<NodeId>, region: NodeId, offset: usize) -> bool {
    let sites = self.until_awaits_by_region(callable, region);
    sites.iter().any(|site| {
      self.work.add_queries(1);
      site.offset == offset
    })
  }

  pub(super) fn until_await_method_calls_on(&self, await_span: Span) -> &[NamedUse] {
    self.work.add_queries(1);
    self.until.await_method_calls.get(&span_key(await_span)).map_or(&[], Vec::as_slice)
  }

  pub(super) fn until_result_method_calls_on(&self, root: SymbolId) -> &[NamedUse] {
    self.work.add_queries(1);
    self.until.result_method_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn result_reassigned(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.reassigned.contains(&root)
  }

  pub(super) fn object_has_spread(&self, object_span: Span) -> bool {
    self.work.add_queries(1);
    self.objects.get(&span_key(object_span)).is_some_and(|entries| {
      entries.iter().any(|entry| {
        self.work.add_object_entries(1);
        matches!(entry, ObjectEntry::Spread)
      })
    })
  }

  pub(super) fn object_entries(&self, object_span: Span) -> &[ObjectEntry] {
    self.work.add_queries(1);
    self.objects.get(&span_key(object_span)).map_or(&[], Vec::as_slice)
  }

  pub(super) fn until_writes_in(
    &self,
    root: SymbolId,
    start: usize,
    end: usize,
  ) -> impl Iterator<Item = ValueWrite> + '_ {
    let writes = self.value_writes.get(&root).map_or(&[][..], Vec::as_slice);
    let from = self.work.partition_point(writes, |write| write.offset <= start);
    let to = self.work.partition_point(writes, |write| write.offset <= end);
    writes.get(from..to).unwrap_or(&[]).iter().copied()
  }

  pub(super) fn disposals_of(&self, callable: Option<NodeId>) -> &[DisposeSite] {
    self.work.add_queries(1);
    self.disposals_by_callable.get(&callable).map_or(&[], Vec::as_slice)
  }

  pub(super) fn watch_consumers_of(&self, root: SymbolId) -> &[WatchConsumer] {
    self.work.add_queries(1);
    self.watches_by_source.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn effect_callback(&self, callable: NodeId) -> Option<EffectCallback> {
    self.work.add_queries(1);
    self.effect_callbacks.get(&callable).copied()
  }

  pub(super) fn run_scope_of(&self, callback: NodeId) -> Option<SymbolId> {
    self.work.add_queries(1);
    self.run_callback_scope.get(&callback).copied()
  }

  pub(super) fn is_async_callable(&self, callable: NodeId) -> bool {
    self.work.add_queries(1);
    self.async_callables.contains(&callable)
  }

  pub(super) fn block_of(&self, node_id: NodeId) -> Option<NodeId> {
    self.work.add_queries(1);
    self.owner(node_id).block
  }

  pub(super) fn call_node(&self, span: Span) -> Option<NodeId> {
    self.work.add_queries(1);
    self.call_nodes.get(&span_key(span)).copied()
  }

  pub(super) fn callable_of(&self, node_id: NodeId) -> Option<NodeId> {
    self.work.add_queries(1);
    self.owner(node_id).callable
  }

  pub(super) fn symbols_for_root(&self, root: SymbolId) -> Vec<SymbolId> {
    self.work.add_queries(1);
    let mut symbols = vec![root];
    if let Some(aliases) = self.aliases_of.get(&root) {
      self.work.add_queries(aliases.len() as u64);
      symbols.extend(aliases.iter().copied());
    }
    symbols
  }

  pub(super) fn callable_node(&self, span: Span) -> Option<NodeId> {
    self.work.add_queries(1);
    self.callables.get(&span_key(span)).copied()
  }

  pub(super) fn value_write_owner_mismatch(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> bool {
    self.work.add_queries(1);
    self.value_write_owner.get(&root).is_some_and(|owner| *owner != (callable, block))
  }

  pub(super) fn member_writes_mixed(&self, root: SymbolId, property: &str) -> bool {
    self.work.add_queries(1);
    self.mixed_member_owners.contains(&(root, property.to_string()))
  }

  pub(super) fn member_write_owner_mismatch(
    &self,
    root: SymbolId,
    property: &str,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> bool {
    self.work.add_queries(1);
    self
      .member_write_owner
      .get(&(root, property.to_string()))
      .is_some_and(|owner| *owner != (callable, block))
  }

  pub(super) fn last_member_write_before(
    &self,
    root: SymbolId,
    property: &str,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<MemberWrite> {
    if self.mixed_member_owners.contains(&(root, property.to_string())) {
      self.work.add_queries(1);
      return None;
    }
    let Some(writes) = self.member_writes.get(&(root, property.to_string())) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(writes, |write| write.offset < offset);
    self.work.add_queries(1);
    let prior = index.checked_sub(1).and_then(|index| writes.get(index)).copied()?;
    (prior.callable == callable && prior.block == block).then_some(prior)
  }

  pub(super) fn first_member_write_after(
    &self,
    root: SymbolId,
    property: &str,
    block: NodeId,
    offset: usize,
  ) -> Option<MemberWrite> {
    let Some(writes) = self.member_writes.get(&(root, property.to_string())) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(writes, |write| write.offset <= offset);
    self.work.add_queries(1);
    let next = writes.get(index).copied()?;
    (next.simple_assign && next.fresh_alloc && next.block == block).then_some(next)
  }

  pub(super) fn object_prop(&self, object_span: Span, property: &str) -> Option<ObjectProp> {
    self.work.add_queries(1);
    self.object_props.get(&span_key(object_span)).and_then(|props| props.get(property)).copied()
  }

  pub(super) fn extracted_method(&self, symbol_id: SymbolId) -> Option<ExtractedMethod> {
    self.work.add_queries(1);
    self.extracted_methods.get(&self.root_of(symbol_id)).copied()
  }

  pub(super) fn capability_poisoned(&self, symbol_id: SymbolId) -> bool {
    self.work.add_queries(1);
    let root = self.root_of(symbol_id);
    self.capability_poisoned.contains(&root) || self.skip_written.contains(&root)
  }

  pub(super) fn collection_capability_invalid(&self, symbol_id: SymbolId) -> bool {
    self.capability_poisoned(symbol_id)
  }

  pub(super) fn extracted_call_eligible(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    extracted: ExtractedMethod,
    call_offset: usize,
  ) -> bool {
    let owner = self.owner(node_id);
    if owner.callable != extracted.callable {
      self.work.add_queries(1);
      return false;
    }
    if call_offset <= extracted.extract_offset {
      self.work.add_queries(1);
      return false;
    }
    if self.has_callable_termination_between(
      extracted.callable,
      extracted.extract_offset,
      call_offset,
    ) {
      return false;
    }
    self.call_reach_is_straight(semantic, node_id)
  }

  pub(super) fn ctor_tainted(&self, name: &str) -> bool {
    self.work.add_queries(1);
    intern_native_ctor(name).is_some_and(|ctor| self.tainted_ctors.contains(ctor))
  }

  pub(super) fn collection_ctor(&self, span: Span) -> Option<CollectionCtor> {
    self.work.add_queries(1);
    self.collections.get(&span_key(span)).copied()
  }

  pub(super) fn is_array_span(&self, span: Span) -> bool {
    self.work.add_queries(1);
    self.arrays.contains(&span_key(span))
  }

  /// Proven closed object literal, or `None` when `span` is not an object.
  /// Precomputed once per object span; lookup charges a query, not a rescan.
  pub(super) fn closed_object_literal(&self, span: Span) -> Option<bool> {
    self.work.add_queries(1);
    self.closed_objects.get(&span_key(span)).copied()
  }

  pub(super) fn is_array_literal(&self, span: Span) -> bool {
    self.work.add_queries(1);
    self.arrays.contains(&span_key(span))
  }

  /// Capability-changing mutation or unknown helper use of a construction /
  /// watched root. Ordinary `state.n` writes stay ordinary member writes.
  /// Unknown flow uses the dedicated `capability_uncertain` role index:
  /// storage, return, unknown call, spread, sequence, receiver, dynamic target.
  /// Generic `escaped` / `uncertain` remain watch/reactive-argument facts.
  pub(super) fn construction_mutated(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
      || self.capability_uncertain.contains(&root)
      || self.has_capability_member_write(root)
  }

  pub(super) fn has_capability_member_write(&self, root: SymbolId) -> bool {
    CAPABILITY_KEYS.iter().any(|key| {
      self.work.add_queries(1);
      self.member_writes.contains_key(&(root, (*key).to_string()))
    })
  }

  pub(super) fn has_closed_keys(&self, object_span: Span) -> bool {
    self.work.add_queries(1);
    self.closed_keys.contains_key(&span_key(object_span))
  }

  pub(super) fn closed_object_has_key(&self, object_span: Span, key: &str) -> Option<bool> {
    self.work.add_queries(1);
    let keys = self.closed_keys.get(&span_key(object_span))?;
    self.work.add_key_lookups(1);
    Some(keys.contains(key))
  }

  pub(super) fn closed_object_only_keys(&self, object_span: Span, allowed: &[&str]) -> bool {
    self.work.add_queries(1);
    let Some(keys) = self.closed_keys.get(&span_key(object_span)) else {
      return false;
    };
    keys.iter().all(|key| {
      self.work.add_key_lookups(1);
      allowed.contains(&key.as_str())
    })
  }

  pub(super) fn keys_closed(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    !self.reassigned.contains(&root)
      && !self.unknown_member_touch.contains(&root)
      && !self.capability_touch.contains(&root)
      && !self.closed_key_unknown.contains(&root)
  }

  pub(super) fn demand_ok(&self, site: &MemberUse) -> bool {
    self.work.add_queries(1);
    site.reach.is_straight() && !site.optional
  }

  pub(super) fn origin_for(&self, node_id: NodeId, offset: usize) -> DemandOrigin {
    let owner = self.owner(node_id);
    DemandOrigin { callable: owner.callable, region: region_of(owner, node_id), offset }
  }

  pub(super) fn has_barrier_between(&self, region: NodeId, start: usize, end: usize) -> bool {
    if end <= start {
      self.work.add_queries(1);
      return false;
    }
    let Some(barriers) = self.barriers_by_region.get(&region) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(barriers, |offset| *offset <= start);
    self.work.add_queries(1);
    barriers.get(index).is_some_and(|offset| *offset < end)
  }

  pub(super) fn demand_from(&self, site: &MemberUse, origin: DemandOrigin) -> bool {
    self.demand_ok(site)
      && site.callable == origin.callable
      && site.region == origin.region
      && !self.has_barrier_between(origin.region, origin.offset, site.offset)
  }

  pub(super) fn member_call_at(&self, span: Span) -> Option<&NamedUse> {
    self.work.add_queries(1);
    self.member_call_by_span.get(&span_key(span))
  }

  pub(super) fn last_stop_before(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    region: NodeId,
    offset: usize,
  ) -> Option<MemberUse> {
    let Some(stops) = self.stops_by_region.get(&(root, callable, region)) else {
      self.work.add_queries(1);
      return None;
    };
    let index = self.work.partition_point(stops, |stop| stop.offset < offset);
    self.work.add_queries(1);
    index.checked_sub(1).and_then(|index| stops.get(index)).copied()
  }

  pub(super) fn capability_intact(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    !self.payload_uncertain(root) && !self.capability_touch.contains(&root)
  }

  pub(super) fn key_mutated(&self, root: SymbolId, key: &str) -> bool {
    self.work.add_queries(1);
    if self.unknown_member_touch.contains(&root) {
      return true;
    }
    self.work.add_key_copies(1);
    self.work.add_key_lookups(1);
    self.member_writes.contains_key(&(root, key.to_string()))
  }

  pub(super) fn copy_key(&self, key: &str) -> String {
    self.work.add_key_copies(1);
    key.to_string()
  }

  pub(super) fn first_straight_value(
    &self,
    root: SymbolId,
    needs: impl Fn(DemandRole) -> bool,
    origin: DemandOrigin,
  ) -> Option<MemberUse> {
    let Some(uses) = self.value_reads.get(&root) else {
      self.work.add_queries(1);
      return None;
    };
    uses.iter().copied().find(|site| {
      self.work.add_queries(1);
      self.demand_from(site, origin) && needs(site.role)
    })
  }

  pub(super) fn has_straight_value_before(
    &self,
    root: SymbolId,
    needs: impl Fn(DemandRole) -> bool,
    origin: DemandOrigin,
    before: usize,
  ) -> bool {
    let Some(uses) = self.value_reads.get(&root) else {
      self.work.add_queries(1);
      return false;
    };
    let end = self.work.partition_point(uses, |site| site.offset < before);
    uses.get(..end).is_some_and(|prior| {
      prior.iter().any(|site| {
        self.work.add_queries(1);
        self.demand_from(site, origin) && needs(site.role)
      })
    })
  }

  pub(super) fn member_calls_on(&self, root: SymbolId) -> &[NamedUse] {
    self.work.add_queries(1);
    self.member_calls_by_root.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn member_reads_on(&self, root: SymbolId) -> &[NamedUse] {
    self.work.add_queries(1);
    self.member_reads_by_root.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn chained_values_on(&self, root: SymbolId) -> &[NamedUse] {
    self.work.add_queries(1);
    self.chained_value_by_root.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(super) fn destructures_of(&self, root: SymbolId) -> &[(SymbolId, String)] {
    self.work.add_queries(1);
    self.destructure_by_object.get(&root).map_or(&[], Vec::as_slice)
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
      self.work.add_queries(1);
      if *local == *root {
        continue;
      }
      self.aliases_of.entry(*root).or_default().push(*local);
    }
    let mut aliases = std::mem::take(&mut self.aliases_of);
    for members in aliases.values_mut() {
      members.sort_by(|left, right| {
        self.work.add_queries(1);
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
    for (key, entries) in &self.objects {
      self.work.add_queries(1);
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
        self.work.add_queries(1);
        self.inactivity_by_handle_block.entry((*root, call.block)).or_default().push(*call);
      }
    }
    for ((root, property), calls) in &self.handle_member_calls {
      if property != "stop" && property != "pause" {
        continue;
      }
      for call in calls {
        self.work.add_queries(1);
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
      self.work.add_queries(1);
      self.ident_calls.entry(self.root_of(symbol_id)).or_default().append(&mut calls);
    }
    let arg_uses = std::mem::take(&mut self.arg_uses);
    for (symbol_id, mut uses) in arg_uses {
      self.work.add_queries(1);
      self.arg_uses.entry(self.root_of(symbol_id)).or_default().append(&mut uses);
    }
    let value_reads = std::mem::take(&mut self.value_reads);
    for (symbol_id, mut reads) in value_reads {
      self.work.add_queries(1);
      self.value_reads.entry(self.root_of(symbol_id)).or_default().append(&mut reads);
    }
    let mut lanes = std::mem::take(&mut self.reads);
    lanes.remap_roots(|symbol_id| self.root_of(symbol_id), || self.work.add_queries(1));
    self.reads = lanes;
    let watches_by_source = std::mem::take(&mut self.watches_by_source);
    for (symbol_id, mut watches) in watches_by_source {
      self.work.add_queries(1);
      self.watches_by_source.entry(self.root_of(symbol_id)).or_default().append(&mut watches);
    }
    let handle_member_calls = std::mem::take(&mut self.handle_member_calls);
    for ((symbol_id, property), mut calls) in handle_member_calls {
      self.work.add_queries(1);
      self
        .handle_member_calls
        .entry((self.root_of(symbol_id), property))
        .or_default()
        .append(&mut calls);
    }
  }

  fn summarize_root_members(&mut self) {
    for (alias, target) in &self.alias_root {
      self.work.add_queries(1);
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
    self.work.add_queries(1);
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
    self.work.add_queries(1);
    let Some(target) = left.as_assignment_target() else {
      return;
    };
    self.work.add_writes(1);
    if assignment_poisons_clone_intrinsic(semantic, target, &self.work) {
      self.clone_intrinsic_poisoned = true;
    }
    self.taint_assignment_target(semantic, target);
    self.mark_pattern_uncertain(semantic, target);
  }

  #[expect(clippy::too_many_lines, reason = "one-pass statement kind dispatch")]
  fn scan(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
  ) {
    for (node_id, node) in semantic.nodes().iter_enumerated() {
      self.work.add_nodes(1);
      match node.kind() {
        AstKind::Class(class) => self.record_class(semantic, node_id, class),
        AstKind::StaticMemberExpression(member) => {
          self.record_static_member(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            member,
          );
          let scheduling_read = self.member_value_is_read(semantic, node_id, member.span);
          self.record_lane_value_reads(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            member,
            scheduling_read,
          );
          self.index_static_member(semantic, kind, member);
        }
        AstKind::VariableDeclarator(declarator) => {
          if matches!(
            semantic.nodes().parent_kind(semantic.nodes().parent_id(node_id)),
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
          ) && let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
            && let Some(symbol_id) = binding.symbol_id.get()
          {
            let root = self.root_of(symbol_id);
            self.escape_root(root, true);
            self.closed_key_unknown.insert(root);
            self.capability_poisoned.insert(root);
          }
          if let Some(init) = &declarator.init {
            self.record_expr(semantic, kind, init);
            if let Some(ident) = init.get_inner_expression().get_identifier_reference()
              && let Some(target) = reference_symbol(semantic, ident)
            {
              match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(binding) => {
                  if let Some(local) = binding.symbol_id.get() {
                    let root = self.root_of(target);
                    if semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
                      self.alias_root.insert(local, root);
                    } else {
                      self.escape_root(root, true);
                      self.capability_poisoned.insert(root);
                    }
                  }
                }
                BindingPattern::ObjectPattern(_) => {
                  self.escape_root(self.root_of(target), true);
                }
                _ => {
                  let root = self.root_of(target);
                  self.escape_root(root, true);
                  self.capability_poisoned.insert(root);
                }
              }
            }
          }
          if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
            && let Some(symbol_id) = binding.symbol_id.get()
            && let Some(init) = &declarator.init
          {
            self.init_span.insert(symbol_id, init.span());
            self.record_native_symbol(
              semantic,
              line_index,
              sfc_source,
              script_offset,
              node_id,
              symbol_id,
              init,
            );
            self
              .init_offset
              .insert(symbol_id, mapped(line_index, sfc_source, script_offset, init.span()).offset);
            match init.get_inner_expression() {
              Expression::CallExpression(call) => {
                self.call_results.insert(span_key(call.span), symbol_id);
              }
              Expression::AwaitExpression(awaited) => {
                self.until.results_by_await.insert(span_key(awaited.span), symbol_id);
              }
              _ => {}
            }
            if semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable)
              && let Some((ident, keys)) = peel_static_chain(init, &self.work)
              && keys.as_slice() == ["value"]
              && let Some(target) = reference_symbol(semantic, ident)
            {
              self.value_object_alias.insert(symbol_id, self.root_of(target));
            }
          }
          if let Some(init) = &declarator.init {
            self.record_destructure(semantic, &declarator.id, init);
          }
          self.record_method_extraction(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            declarator,
          );
        }
        AstKind::AssignmentExpression(assignment) => {
          self.record_expr(semantic, kind, &assignment.right);
          self.mark_escape_expr(semantic, &assignment.right, true);
          self.poison_expr(semantic, &assignment.right);
          self.taint_assignment_target(semantic, &assignment.left);
          if assignment_poisons_map_intrinsic(semantic, &assignment.left, &self.work) {
            self.map_intrinsic_poisoned = true;
          }
          let offset = mapped(line_index, sfc_source, script_offset, assignment.span).offset;
          self.index_assignment(
            semantic,
            node_id,
            offset,
            assignment.operator,
            &assignment.left,
            &assignment.right,
          );
          self.record_wrapper_assignment(
            semantic,
            kind,
            node_id,
            offset,
            assignment.operator,
            &assignment.left,
            &assignment.right,
          );
          self.record_stmt_site(semantic, line_index, sfc_source, script_offset, node_id);
        }
        AstKind::ForInStatement(statement) => {
          self.note_loop_assignment_poison(semantic, &statement.left);
          self.note_loop_map_poison(semantic, &statement.left);
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ForOfStatement(statement) => {
          self.note_loop_assignment_poison(semantic, &statement.left);
          self.note_loop_map_poison(semantic, &statement.left);
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::UpdateExpression(update) => {
          self.poison_simple_target(semantic, &update.argument);
          if simple_target_poisons_map_intrinsic(semantic, &update.argument, &self.work) {
            self.map_intrinsic_poisoned = true;
          }
          if simple_target_poisons_clone_intrinsic(semantic, &update.argument, &self.work) {
            self.clone_intrinsic_poisoned = true;
          }
          match &update.argument {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
              if let Some(symbol_id) = reference_symbol(semantic, identifier) {
                self.reassigned.insert(self.root_of(symbol_id));
              }
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
              if member.property.name.as_str() == "value"
                && let Some(object) =
                  member.object.get_inner_expression().get_identifier_reference()
                && let Some(symbol_id) = reference_symbol(semantic, object)
              {
                let root = self.root_of(symbol_id);
                let owner = self.owner(node_id);
                let offset = mapped(line_index, sfc_source, script_offset, update.span).offset;
                self.value_write_roots.insert(root);
                let event = ValueWrite {
                  offset,
                  callable: owner.callable,
                  block: owner.block.unwrap_or(node_id),
                  span: update.span,
                  rhs: update.span,
                  literal: WriteLiteral::Other,
                  simple_assign: false,
                  fresh_alloc: false,
                  node_id,
                };
                self.value_writes.entry(root).or_default().push(event);
                self.value_write_events.entry(root).or_default().push(event);
              }
            }
            _ => {}
          }
        }
        AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
          self.poison_member_expression(semantic, &unary.argument);
          self.note_delete(semantic, &unary.argument);
          if expression_poisons_map_intrinsic(semantic, &unary.argument, &self.work) {
            self.map_intrinsic_poisoned = true;
          }
        }
        AstKind::Function(function) => {
          self.record_function(node_id, function.span, &function.params, false);
          if function.r#async {
            self.async_callables.insert(node_id);
          }
        }
        AstKind::ArrowFunctionExpression(arrow) => {
          self.record_function(node_id, arrow.span, &arrow.params, arrow.expression);
          if arrow.r#async {
            self.async_callables.insert(node_id);
          }
        }
        AstKind::CallExpression(call) => {
          self.record_call(semantic, kind, call);
          self.call_nodes.insert(span_key(call.span), node_id);
          self.record_map_member_call(
            semantic,
            kind,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            call,
          );
          self.record_call_uses(semantic, line_index, sfc_source, script_offset, node_id, call);
          let info = self.calls.get(&span_key(call.span)).copied();
          if info.is_some_and(|call_info| call_info.vueuse.is_some()) {
            self
              .producer_call_offsets
              .push(mapped(line_index, sfc_source, script_offset, call.span).offset);
          }
          let api = info.and_then(|call_info| call_info.api);
          for (index, argument) in call.arguments.iter().enumerate() {
            if let Some(expression) = argument.as_expression() {
              self.record_expr(semantic, kind, expression);
              let until_borrow = index == 0
                && info.is_some_and(|call_info| {
                  call_info.vueuse == Some("until") && !call_info.has_spread
                });
              if !is_retained_vueuse_source_arg(info, index) {
                self.until.pending_borrow = until_borrow;
                let skip_injection_key = matches!(api, Some("provide" | "inject")) && index == 0;
                if !skip_injection_key {
                  self.mark_escape_expr(
                    semantic,
                    expression,
                    !(api == Some("toRef") && index == 0),
                  );
                }
                self.until.pending_borrow = false;
              }
              if capability_mutating_callee(semantic, &call.callee) {
                self.poison_expr(semantic, expression);
                self.mark_helper_escape_expr(semantic, expression);
              } else if api.is_none() {
                if let Some(pending) =
                  self.pending_native_map_key_arg(semantic, call, index, expression)
                {
                  self.pending_map_key_args.push(pending);
                } else if !is_retained_vueuse_source_arg(info, index) {
                  self.poison_expr(semantic, expression);
                  self.mark_helper_escape_expr(semantic, expression);
                }
              } else if !(vue_wrapper_skips_capability(call, index, &self.calls)
                || matches!(api, Some("provide" | "inject")) && index == 0)
              {
                self.mark_helper_escape_expr(semantic, expression);
              }
            }
          }
          if matches!(api, Some("provide" | "inject")) {
            self.record_injection(semantic, line_index, sfc_source, script_offset, node_id, call);
          }
          self.note_receiver_use(semantic, &call.callee);
          self.record_stmt_site(semantic, line_index, sfc_source, script_offset, node_id);
          self.record_event(semantic, line_index, sfc_source, script_offset, node_id, call.span);
          self.record_member_call(semantic, line_index, sfc_source, script_offset, node_id, call);
          self.record_identifier_call(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            call,
          );
          self.record_result_demand(semantic, line_index, sfc_source, script_offset, node_id, call);
          if callee_is_pause(call) {
            self.record_pause_event(node_id, line_index, sfc_source, script_offset, call.span);
          }
          self.record_scheduling_member_call(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            call,
          );
        }
        AstKind::NewExpression(expression) => {
          self.record_expr(semantic, kind, &expression.callee);
          self.note_receiver_use(semantic, &expression.callee);
          self.record_map_new(semantic, kind, expression);
          let skip_map_args =
            self.news.get(&span_key(expression.span)).is_some_and(|info| info.ctor == Some("Map"));
          for argument in &expression.arguments {
            if let Some(arg) = argument.as_expression() {
              self.record_expr(semantic, kind, arg);
              if !skip_map_args {
                self.mark_escape_expr(semantic, arg, true);
                self.poison_expr(semantic, arg);
              }
            }
          }
          if let Some(ctor) = unresolved_collection_kind(&expression.callee, |ident| {
            reference_symbol(semantic, ident)
          }) {
            self.collections.insert(span_key(expression.span), ctor);
          }
          if is_unresolved_date(&expression.callee, |ident| reference_symbol(semantic, ident)) {
            self.dates.insert(span_key(expression.span));
          }
          self.record_class_new(semantic, expression);
        }
        AstKind::TaggedTemplateExpression(tagged) => {
          self.note_receiver_use(semantic, &tagged.tag);
          for expression in &tagged.quasi.expressions {
            self.mark_escape_expr(semantic, expression, true);
            self.poison_expr(semantic, expression);
          }
        }
        AstKind::ObjectExpression(object) => {
          self.literal_span.insert(span_key(object.span), object.span);
          self.object_literals.insert(span_key(object.span));
          if object_has_skip_marker(object) {
            self.skip_marker_objects.insert(span_key(object.span));
          }
          let entries = object_entries(object);
          let props = summarize_object_props(&entries, &self.work);
          self.object_props.insert(span_key(object.span), props);
          self.objects.insert(span_key(object.span), entries);
          for property in &object.properties {
            if let ObjectPropertyKind::ObjectProperty(property) = property {
              self.record_expr(semantic, kind, &property.value);
              self.mark_escape_expr(semantic, &property.value, true);
              self.poison_expr(semantic, &property.value);
            }
          }
        }
        AstKind::ArrayExpression(array) => {
          self.index_array_expression(semantic, kind, node_id, array);
        }
        AstKind::ReturnStatement(statement) => {
          if let Some(argument) = &statement.argument {
            self.record_expr(semantic, kind, argument);
            self.mark_escape_expr(semantic, argument, true);
            self.poison_expr(semantic, argument);
          }
          self.record_termination(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier_end(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ThrowStatement(statement) => {
          self.record_expr(semantic, kind, &statement.argument);
          self.mark_escape_expr(semantic, &statement.argument, true);
          self.poison_expr(semantic, &statement.argument);
          self.record_termination(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier_end(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::SpreadElement(spread) => {
          self.mark_escape_expr(semantic, &spread.argument, true);
          self.poison_expr(semantic, &spread.argument);
        }
        AstKind::IfStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ForStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::WhileStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::SwitchStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::TryStatement(statement) => {
          self.record_flow(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            statement.span,
          );
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::StringLiteral(literal) => {
          self.store_interned_atom(literal.span, literal.value.as_str(), false);
        }
        AstKind::BigIntLiteral(literal) => {
          if let Some(raw) = literal.raw.as_deref() {
            self.store_interned_atom(literal.span, raw, true);
          }
        }
        AstKind::TemplateLiteral(literal) if literal.expressions.is_empty() => {
          if let Some(cooked) =
            literal.quasis.first().and_then(|quasi| quasi.value.cooked.as_deref())
          {
            self.store_interned_atom(literal.span, cooked, false);
          }
        }
        AstKind::BooleanLiteral(literal) => {
          self.store_atom(literal.span, ShapePrimitiveAtom::Bool(literal.value));
        }
        AstKind::NumericLiteral(literal) => {
          self.store_atom(
            literal.span,
            ShapePrimitiveAtom::Number {
              bits: literal.value.to_bits(),
              nan: literal.value.is_nan(),
            },
          );
        }
        AstKind::NullLiteral(literal) => self.store_atom(literal.span, ShapePrimitiveAtom::Null),
        AstKind::IdentifierReference(identifier)
          if identifier.name.as_str() == "undefined"
            && reference_symbol(semantic, identifier).is_none() =>
        {
          self.store_atom(identifier.span, ShapePrimitiveAtom::Undefined);
        }
        AstKind::UnaryExpression(unary)
          if matches!(unary.operator, UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus) =>
        {
          match unary.operator {
            UnaryOperator::UnaryNegation => {
              if let Some(ShapePrimitiveAtom::Number { bits, nan: false }) =
                primitive_atom(&unary.argument)
              {
                self.store_atom(
                  unary.span,
                  ShapePrimitiveAtom::Number {
                    bits: (-f64::from_bits(bits)).to_bits(),
                    nan: false,
                  },
                );
              }
            }
            UnaryOperator::UnaryPlus => {
              if let Some(atom) = primitive_atom(&unary.argument) {
                self.store_atom(unary.span, atom);
              }
            }
            _ => {}
          }
        }
        AstKind::AwaitExpression(expression) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, expression.span);
          self.record_await(
            semantic,
            kind,
            line_index,
            sfc_source,
            script_offset,
            node_id,
            expression.span,
            &expression.argument,
          );
        }
        AstKind::YieldExpression(expression) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, expression.span);
        }
        AstKind::BreakStatement(statement) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ContinueStatement(statement) => {
          self.record_barrier(line_index, sfc_source, script_offset, node_id, statement.span);
        }
        AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
          if let AstKind::VariableDeclaration(_) = semantic.nodes().parent_kind(node_id) {
            // handled via BindingIdentifier export flags below
          }
        }
        AstKind::IdentifierReference(identifier) => {
          self.index_unresolved_global_ref(semantic, identifier);
        }
        _ => {}
      }
    }
  }

  fn finish_aliases_and_roles(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let leftover = self.merged_unresolved_escape_ranges();
    for symbol_id in semantic.scoping().symbol_ids() {
      for reference in semantic.symbol_references(symbol_id) {
        self.work.add_references(1);
        let root = self.root_of(symbol_id);
        if !leftover.is_empty() {
          let node_span = semantic.nodes().kind(reference.node_id()).span();
          if self.unresolved_escape_covers(&leftover, node_span) {
            self.capability_poisoned.insert(root);
            self.work.add_queries(1);
            if self.global_this_aliases.contains(&root) {
              self.taint_all_native_ctors();
            }
            if let Some(ctor) = self.native_ctor_kind(root) {
              self.tainted_ctors.insert(ctor);
            }
          }
        }
        let flags = reference.flags();
        let node_id = reference.node_id();
        let member_payload = flags.intersects(ReferenceFlags::MemberWriteTarget)
          || known_static_member_object_role(semantic, node_id, &self.work);
        if !closed_key_use_is_known(
          semantic,
          node_id,
          flags,
          member_payload,
          &self.calls,
          &self.work,
        ) {
          self.closed_key_unknown.insert(root);
        }
        if member_payload {
          if flags.is_write() && !self.has_simple_value_write(root) && !self.has_member_write(root)
          {
            self.uncertain.insert(root);
          }
          if known_static_member_object_role(semantic, reference.node_id(), &self.work)
            && self.static_member_chain_receiver_uncertain(semantic, reference.node_id())
          {
            self.toref_identity_uncertain.insert(root);
            self.capability_uncertain.insert(root);
            self.closed_key_unknown.insert(root);
          }
          continue;
        }
        if flags.is_write() {
          self.reassigned.insert(root);
          continue;
        }
        if known_const_alias_role(semantic, reference.node_id()) {
          continue;
        }
        if known_injection_key_role(semantic, reference.node_id(), &self.calls, &self.work) {
          continue;
        }
        if known_class_ctor_role(semantic, reference.node_id(), &self.classes, root) {
          continue;
        }
        if known_map_constructor_key_role(semantic, reference.node_id()) {
          continue;
        }
        self.uncertain.insert(root);
        if !self.known_toref_source_argument_role(semantic, reference.node_id()) {
          self.toref_identity_uncertain.insert(root);
        }
        if !self.known_vue_source_argument_role(semantic, reference.node_id()) {
          self.capability_uncertain.insert(root);
        }
      }
    }
    if !leftover.is_empty() {
      self.apply_unresolved_global_escape_coverage(&leftover);
    }
  }

  /// First argument of a proven `toRef` call. Other identifier uses do not prove `__v_isRef`.
  fn known_toref_source_argument_role(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
  ) -> bool {
    self.work.add_queries(1);
    let ident_span = semantic.nodes().kind(node_id).span();
    let mut current = node_id;
    for _ in 0..MAX_ROLE_ANCESTORS {
      let parent_id = semantic.nodes().parent_id(current);
      self.work.add_queries(1);
      match semantic.nodes().kind(parent_id) {
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::ChainExpression(_) => {
          current = parent_id;
        }
        AstKind::CallExpression(call) => {
          let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
            return false;
          };
          if info.api != Some("toRef") {
            return false;
          }
          let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
            return false;
          };
          let inner = first.get_inner_expression();
          return inner.span() == ident_span || first.span() == ident_span;
        }
        _ => return false,
      }
    }
    false
  }

  /// Receiver call / `new` / tagged template / unknown member-chain of a static
  /// member object, including TypeScript instantiation wrappers. Ordinary
  /// assigned-property reads and writes stay outside this set. Import sources,
  /// JSX member tags, and decorator expressions stay on the default (proven)
  /// branch; those positions bind no JS `this` receiver. Exhausted ancestor
  /// budgets stay unproven.
  fn static_member_chain_receiver_uncertain(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
  ) -> bool {
    let mut current = node_id;
    let mut current_span = semantic.nodes().kind(node_id).span();
    for _ in 0..MAX_ROLE_ANCESTORS {
      let parent_id = semantic.nodes().parent_id(current);
      self.work.add_queries(1);
      match semantic.nodes().kind(parent_id) {
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::TSInstantiationExpression(_)
        | AstKind::ChainExpression(_) => {
          current = parent_id;
          current_span = semantic.nodes().kind(parent_id).span();
        }
        AstKind::StaticMemberExpression(member) => {
          if !callee_span_matches(&member.object, current_span) {
            return false;
          }
          current = parent_id;
          current_span = member.span();
        }
        AstKind::ComputedMemberExpression(member) => {
          return callee_span_matches(&member.object, current_span);
        }
        AstKind::CallExpression(call) => {
          return callee_span_matches(&call.callee, current_span);
        }
        AstKind::NewExpression(expression) => {
          return callee_span_matches(&expression.callee, current_span);
        }
        AstKind::TaggedTemplateExpression(tagged) => {
          return callee_span_matches(&tagged.tag, current_span);
        }
        _ => return false,
      }
    }
    true
  }

  fn known_vue_source_argument_role(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
  ) -> bool {
    self.work.add_queries(1);
    let ident_span = semantic.nodes().kind(node_id).span();
    let mut current = node_id;
    for _ in 0..MAX_ROLE_ANCESTORS {
      let parent_id = semantic.nodes().parent_id(current);
      self.work.add_queries(1);
      match semantic.nodes().kind(parent_id) {
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::ChainExpression(_) => {
          current = parent_id;
        }
        AstKind::CallExpression(call) => {
          let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
            return false;
          };
          if !is_known_constructor_or_watch(info.api) {
            return false;
          }
          let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
            return false;
          };
          let inner = first.get_inner_expression();
          return inner.span() == ident_span || first.span() == ident_span;
        }
        _ => return false,
      }
    }
    false
  }

  fn has_simple_value_write(&self, root: SymbolId) -> bool {
    self.value_write_roots.contains(&root)
  }

  fn has_member_write(&self, root: SymbolId) -> bool {
    self.member_write_roots.contains(&root)
  }

  fn index_static_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    member: &StaticMemberExpression<'_>,
  ) {
    self.record_expr(semantic, kind, &member.object);
    self.members.insert(
      span_key(member.span),
      MemberInfo {
        object: member.object.get_inner_expression().span(),
        property: member.property.name.to_string(),
        span: member.span,
      },
    );
  }

  fn record_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    expression: &Expression<'_>,
  ) {
    let inner = expression.get_inner_expression();
    self
      .hints
      .insert(span_key(inner.span()), hint_of(inner, |ident| reference_symbol(semantic, ident)));
    if let Some(atom) = atom_of_expression(inner, &self.work, super::MAX_DEPTH) {
      self.atoms.insert(span_key(inner.span()), atom);
    } else if let Expression::Identifier(identifier) = inner
      && reference_symbol(semantic, identifier).is_none()
      && let Some(atom) = PrimitiveAtom::unresolved_global(identifier.name.as_str())
    {
      self.work.add_queries(1);
      self.atoms.insert(span_key(inner.span()), atom);
    }
    self.hints.insert(
      span_key(expression.span()),
      hint_of(inner, |ident| reference_symbol(semantic, ident)),
    );
    if matches!(inner, Expression::ObjectExpression(_) | Expression::ArrayExpression(_)) {
      self.literal_span.insert(span_key(expression.span()), inner.span());
      self.literal_span.insert(span_key(inner.span()), inner.span());
    }
    if let Some(truthy) = literal_truthy(semantic, inner) {
      self.known_truthy.insert(span_key(inner.span()), truthy);
      self.known_truthy.insert(span_key(expression.span()), truthy);
    }
    if let Some(nullish) = literal_nullish(semantic, inner) {
      self.known_nullish.insert(span_key(inner.span()), nullish);
      self.known_nullish.insert(span_key(expression.span()), nullish);
    }
    if matches!(inner, Expression::ObjectExpression(_)) {
      self.object_literals.insert(span_key(inner.span()));
      self.object_literals.insert(span_key(expression.span()));
    }
    if let Some(atom) = primitive_atom(inner) {
      self.store_atom(inner.span(), atom);
      self.store_atom(expression.span(), atom);
    }
    if let Some(scalar) = scalar_of(inner) {
      self.until.scalars.insert(span_key(inner.span()), scalar);
      self.until.scalars.insert(span_key(expression.span()), scalar);
    }
    if let Some(literal) = literal_of(inner) {
      self.literals.insert(span_key(inner.span()), literal);
      self.literals.insert(span_key(expression.span()), literal);
    }
    self.intern_expr(semantic, inner);
    if let Expression::CallExpression(call) = inner {
      self.record_call(semantic, kind, call);
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "one static-member visit fills custom-ref, derivation, and scheduling lanes"
  )]
  fn record_lane_value_reads(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    member: &oxc_ast::ast::StaticMemberExpression<'_>,
    scheduling_read: bool,
  ) {
    if member.property.name.as_str() != "value" {
      return;
    }
    let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let owner = self.owner(node_id);
    let read = ValueRead {
      node_id,
      offset: mapped(line_index, sfc_source, script_offset, member.span).offset,
      span: member.span,
      callable: owner.callable,
      block: owner.block.unwrap_or(node_id),
    };
    // customRef keeps the pre-alias symbol; remap_roots canonicalizes later.
    // A write context is not a customRef read. Derivation skips every assignment
    // and update parent, including a read on the right-hand side. Scheduling uses
    // the caller-computed `member_value_is_read` predicate.
    if !member_is_write_context(semantic, node_id, member.span) {
      self.reads.record_custom_ref(symbol_id, read);
    }
    match semantic.nodes().parent_kind(node_id) {
      AstKind::AssignmentExpression(_) | AstKind::UpdateExpression(_) => {}
      _ => {
        self.work.add_writes(1);
        let root = self.root_of(symbol_id);
        self.reads.record_derivation(root, read);
      }
    }
    if scheduling_read {
      self.work.add_writes(1);
      let root = self.root_of(symbol_id);
      self.reads.record_scheduling(root, read);
    }
  }

  fn intern(&mut self, value: &str) -> u32 {
    self.work.add_queries(1);
    if let Some(index) = self.interned.iter().position(|existing| {
      self.work.add_queries(1);
      existing == value
    }) {
      return u32::try_from(index).unwrap_or(u32::MAX);
    }
    let id = u32::try_from(self.interned.len()).unwrap_or(u32::MAX);
    self.interned.push(value.to_string());
    self.work.add_queries(1);
    id
  }

  fn store_atom(&mut self, span: Span, atom: ShapePrimitiveAtom) {
    self.work.add_queries(1);
    self.primitives.insert(span_key(span), atom);
  }

  fn store_interned_atom(&mut self, span: Span, value: &str, bigint: bool) {
    let id = self.intern(value);
    self.store_atom(
      span,
      if bigint { ShapePrimitiveAtom::BigInt(id) } else { ShapePrimitiveAtom::Str(id) },
    );
  }

  fn intern_expr(&mut self, semantic: &oxc_semantic::Semantic<'_>, expression: &Expression<'_>) {
    self.work.add_queries(1);
    match expression.get_inner_expression() {
      Expression::StringLiteral(literal) => {
        self.store_interned_atom(literal.span, literal.value.as_str(), false);
        self.store_interned_atom(expression.span(), literal.value.as_str(), false);
      }
      Expression::BigIntLiteral(literal) => {
        if let Some(raw) = literal.raw.as_deref() {
          self.store_interned_atom(literal.span, raw, true);
          self.store_interned_atom(expression.span(), raw, true);
        }
      }
      Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
        if let Some(cooked) = literal.quasis.first().and_then(|quasi| quasi.value.cooked.as_deref())
        {
          self.store_interned_atom(literal.span, cooked, false);
          self.store_interned_atom(expression.span(), cooked, false);
        }
      }
      Expression::Identifier(identifier)
        if identifier.name.as_str() == "undefined"
          && reference_symbol(semantic, identifier).is_none() =>
      {
        self.store_atom(identifier.span, ShapePrimitiveAtom::Undefined);
        self.store_atom(expression.span(), ShapePrimitiveAtom::Undefined);
      }
      Expression::UnaryExpression(unary)
        if matches!(unary.operator, UnaryOperator::UnaryNegation) =>
      {
        self.intern_expr(semantic, &unary.argument);
        if let Some(ShapePrimitiveAtom::Number { bits, nan: false }) =
          self.primitives.get(&span_key(unary.argument.span())).copied()
        {
          self.store_atom(
            unary.span,
            ShapePrimitiveAtom::Number { bits: (-f64::from_bits(bits)).to_bits(), nan: false },
          );
        }
      }
      Expression::UnaryExpression(unary) if matches!(unary.operator, UnaryOperator::UnaryPlus) => {
        self.intern_expr(semantic, &unary.argument);
        if let Some(atom) = self.primitives.get(&span_key(unary.argument.span())).copied() {
          self.store_atom(unary.span, atom);
          self.store_atom(expression.span(), atom);
        }
      }
      _ => {}
    }
  }

  fn member_value_is_read(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    member_span: Span,
  ) -> bool {
    self.work.add_queries(1);
    match semantic.nodes().parent_kind(node_id) {
      AstKind::UpdateExpression(_) => false,
      AstKind::AssignmentExpression(assignment) => {
        let right = assignment.right.span();
        if right.start <= member_span.start && member_span.end <= right.end {
          return true;
        }
        assignment.operator != AssignmentOperator::Assign
          && assignment.left.span().start <= member_span.start
          && member_span.end <= assignment.left.span().end
      }
      _ => true,
    }
  }

  fn record_scheduling_member_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return;
    };
    let Some(method) = intern_scope_method(member.property.name.as_str()) else {
      return;
    };
    let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let owner = self.owner(node_id);
    self.work.add_writes(1);
    let root = self.root_of(symbol_id);
    self.reads.record_scheduling_call(
      root,
      MemberCall {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        method,
        span: call.span,
        callable: owner.callable,
        block: owner.block.unwrap_or(node_id),
      },
    );
  }

  #[expect(clippy::too_many_arguments, reason = "await indexing needs owner, span, and callee")]
  fn record_await(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
    argument: &Expression<'_>,
  ) {
    self.record_event(semantic, line_index, sfc_source, script_offset, node_id, span);
    let inner = argument.get_inner_expression();
    let callee_api = match inner {
      Expression::CallExpression(call) => resolve_vue_api(
        &call.callee,
        &self.vue_imports,
        |ident| reference_symbol(semantic, ident),
        kind,
      ),
      _ => None,
    };
    let owner = self.owner(node_id);
    self.work.add_writes(1);
    let site = AwaitSite {
      offset: mapped(line_index, sfc_source, script_offset, span).offset,
      callee_api,
      callable: owner.callable,
      block: owner.block.unwrap_or(node_id),
    };
    self.awaits.push(site);
    self.work.add_writes(1);
    self.awaits_by_callable.entry(owner.callable).or_default().push(site);
    let await_span = mapped(line_index, sfc_source, script_offset, span);
    let argument = argument.get_inner_expression();
    let bound =
      argument.get_identifier_reference().and_then(|ident| reference_symbol(semantic, ident));
    let region = region_of(owner, node_id);
    self.until.awaits.push(UntilAwaitSite {
      offset: await_span.offset,
      end: await_span.offset.saturating_add(await_span.length),
      span,
      argument: argument.span(),
      bound,
      callable: owner.callable,
      region,
      reach: classify_reach(semantic, node_id, &self.work),
    });
  }

  fn finish_practice_indexes(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
  ) {
    let node_ids: Vec<NodeId> = self.call_nodes.values().copied().collect();
    self.work.add_queries(node_ids.len() as u64);
    for node_id in node_ids {
      let AstKind::CallExpression(call) = semantic.nodes().kind(node_id) else {
        continue;
      };
      self.work.add_queries(1);
      let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
        continue;
      };
      self.bind_practice_call(
        semantic,
        line_index,
        sfc_source,
        script_offset,
        kind,
        node_id,
        call,
        info,
      );
    }
  }

  #[expect(clippy::too_many_arguments, reason = "practice index bind needs owner, span, and call")]
  fn bind_practice_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    kind: ScriptKind,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    let owner = self.owner(node_id);
    let offset = mapped(line_index, sfc_source, script_offset, call.span).offset;
    match info.api {
      Some("onScopeDispose") => {
        let child = nth_call_expr(call, 0).and_then(|expression| {
          stopped_child_symbol(semantic, expression, |ident| reference_symbol(semantic, ident))
        });
        let site = DisposeSite { offset, span: call.span, child, callable: owner.callable };
        self.work.add_writes(1);
        self.disposals_by_callable.entry(owner.callable).or_default().push(site);
      }
      Some("watch") => {
        let Some(source_expr) = nth_call_expr(call, 0) else {
          return;
        };
        let Some(ident) = source_expr.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(symbol_id) = reference_symbol(semantic, ident) else {
          return;
        };
        let (once, immediate, options_unknown) = watch_option_flags(nth_call_expr(call, 2));
        let handle = assigned_const_symbol(semantic, node_id);
        let consumer = WatchConsumer {
          offset,
          span: call.span,
          callable: owner.callable,
          source: self.root_of(symbol_id),
          once,
          immediate,
          options_unknown,
          handle,
        };
        self.work.add_writes(1);
        self.watches_by_source.entry(consumer.source).or_default().push(consumer);
        if let Some(callback) = nth_call_expr(call, 1)
          && let Some(callback_id) = function_node(callback, |span| {
            self.work.add_queries(1);
            self.callables.get(&span_key(span)).copied()
          })
        {
          self.work.add_writes(1);
          self.effect_callbacks.insert(callback_id, EffectCallback { offset, api: "watch" });
        }
      }
      Some("watchEffect" | "watchPostEffect" | "watchSyncEffect") => {
        let Some(api) = info.api else {
          return;
        };
        if let Some(callback) = nth_call_expr(call, 0)
          && let Some(callback_id) = function_node(callback, |span| {
            self.work.add_queries(1);
            self.callables.get(&span_key(span)).copied()
          })
        {
          self.work.add_writes(1);
          self.effect_callbacks.insert(callback_id, EffectCallback { offset, api });
        }
      }
      _ => {}
    }
    let _ = kind;
    if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
      && member.property.name.as_str() == "run"
      && let Some(object) = member.object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, object)
      && let Some(callback) = nth_call_expr(call, 0)
      && let Some(callback_id) = function_node(callback, |span| {
        self.work.add_queries(1);
        self.callables.get(&span_key(span)).copied()
      })
    {
      self.work.add_writes(1);
      self.run_callback_scope.insert(callback_id, self.root_of(symbol_id));
    }
  }

  fn record_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    call: &CallExpression<'_>,
  ) {
    let has_spread = call.arguments.iter().any(Argument::is_spread);
    let api = resolve_vue_api(
      &call.callee,
      &self.vue_imports,
      |ident| reference_symbol(semantic, ident),
      kind,
    );
    let vueuse = resolve_vueuse_api(&call.callee, &self.vueuse_imports, |ident| {
      reference_symbol(semantic, ident)
    });
    let first_arg = if has_spread {
      None
    } else {
      call.arguments.first().and_then(Argument::as_expression).map(GetSpan::span)
    };
    let second_arg = if has_spread {
      None
    } else {
      call.arguments.get(1).and_then(Argument::as_expression).map(GetSpan::span)
    };
    let native_structured_clone =
      !call.optional && is_native_structured_clone(&call.callee, semantic);
    let actual_proxy_origin = api.is_some_and(is_proxy_allocating_api)
      && callee_has_actual_proxy_origin(&call.callee, &self.vue_imports, semantic, &self.work);
    let arg_count = u8::try_from(call.arguments.len()).unwrap_or(u8::MAX);
    let third_arg = if has_spread {
      None
    } else {
      call.arguments.get(2).and_then(Argument::as_expression).map(GetSpan::span)
    };
    self.calls.insert(
      span_key(call.span),
      CallInfo {
        span: call.span,
        api,
        vueuse,
        first_arg,
        second_arg,
        has_spread,
        native_structured_clone,
        actual_proxy_origin,
        arg_count,
        third_arg,
      },
    );
    if is_object_define_property(&call.callee) {
      self.prototype_mutated = true;
    }
  }

  fn note_delete(&mut self, semantic: &oxc_semantic::Semantic<'_>, argument: &Expression<'_>) {
    if expression_poisons_clone_intrinsic(semantic, argument, &self.work) {
      self.clone_intrinsic_poisoned = true;
    }
    self.mark_delete_target(semantic, argument);
  }

  fn index_array_expression(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    kind: ScriptKind,
    node_id: NodeId,
    array: &oxc_ast::ast::ArrayExpression<'_>,
  ) {
    self.arrays.insert(span_key(array.span));
    self.literal_span.insert(span_key(array.span), array.span);
    if array
      .elements
      .iter()
      .any(|element| matches!(element, oxc_ast::ast::ArrayExpressionElement::SpreadElement(_)))
    {
      self.array_spread.insert(span_key(array.span));
    }
    let map_entry = array_is_map_entry(semantic, node_id);
    let map_iterable = array_is_map_iterable(semantic, node_id);
    let retain = array_is_controlled_source(semantic, node_id, &self.calls, &self.work);
    let mut elements = Vec::new();
    let mut closed = true;
    for (index, element) in array.elements.iter().enumerate() {
      self.work.add_object_entries(1);
      let Some(expression) = element.as_expression() else {
        closed = false;
        continue;
      };
      self.record_expr(semantic, kind, expression);
      if closed {
        elements.push(expression.get_inner_expression().span());
      }
      if map_entry && index == 0 {
        continue;
      }
      if map_iterable || retain {
        continue;
      }
      self.mark_escape_expr(semantic, expression, true);
      self.poison_expr(semantic, expression);
    }
    if closed {
      self.array_elements.insert(span_key(array.span), elements);
    }
  }

  fn mark_delete_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    argument: &Expression<'_>,
  ) {
    match argument.get_inner_expression() {
      Expression::StaticMemberExpression(member) => {
        let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(symbol_id) = reference_symbol(semantic, object) else {
          return;
        };
        if member.property.name.as_str() == TOREF_CAPABILITY_KEY {
          self.toref_identity_uncertain.insert(self.root_of(symbol_id));
        }
      }
      Expression::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          self.toref_identity_uncertain.insert(self.root_of(symbol_id));
        }
      }
      _ => {}
    }
  }

  fn record_function(
    &mut self,
    node_id: NodeId,
    span: Span,
    params: &FormalParameters<'_>,
    expression_arrow: bool,
  ) {
    let mut simple = Vec::with_capacity(params.items.len());
    let mut has_pattern = false;
    for item in &params.items {
      if let Some(symbol_id) = simple_binding_symbol(&item.pattern) {
        simple.push(Some(symbol_id));
      } else {
        has_pattern = true;
        simple.push(None);
      }
    }
    self.callables.insert(span_key(span), node_id);
    self.function_by_node.insert(node_id, span);
    self.functions.insert(
      span_key(span),
      FunctionInfo {
        node_id,
        params: simple,
        has_rest: params.rest.is_some() || has_pattern,
        expression_arrow,
      },
    );
  }

  fn record_call_uses(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let owner = self.owner(node_id);
    let block = owner.block.unwrap_or(node_id);
    let offset = mapped(line_index, sfc_source, script_offset, call.span).offset;
    let api = self.calls.get(&span_key(call.span)).and_then(|info| info.api);
    if super::shape::is_watch_effect_api(api.unwrap_or(""))
      && let Some(first) = call.arguments.first().and_then(Argument::as_expression)
    {
      let inner = first.get_inner_expression();
      self.effect_calls.insert(
        span_key(inner.span()),
        ArgUse {
          api,
          index: 0,
          call_span: call.span,
          offset,
          node_id,
          callable: owner.callable,
          block,
        },
      );
    }
    if api == Some("watch")
      && let Some(first) = call.arguments.first().and_then(Argument::as_expression)
    {
      let inner = first.get_inner_expression();
      if matches!(inner, Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
      {
        self.watch_getters.insert(
          span_key(inner.span()),
          ArgUse {
            api,
            index: 0,
            call_span: call.span,
            offset,
            node_id,
            callable: owner.callable,
            block,
          },
        );
      }
    }
    if api.is_some_and(|name| {
      matches!(name, "watch" | "watchEffect" | "watchPostEffect" | "watchSyncEffect" | "effect")
    }) {
      self.watch_options.insert(span_key(call.span), parse_watch_options(call, api));
    }
    if let Some(identifier) = call.callee.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.ident_calls.entry(symbol_id).or_default().push(IdentCall {
        span: call.span,
        offset,
        callable: owner.callable,
        block,
      });
    }
    if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
      && let Some(object) = member.object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, object)
    {
      self
        .handle_member_calls
        .entry((symbol_id, member.property.name.to_string()))
        .or_default()
        .push(IdentCall { span: call.span, offset, callable: owner.callable, block });
    }
    for (index, argument) in call.arguments.iter().enumerate() {
      let Some(expression) = argument.as_expression() else {
        continue;
      };
      self.record_arg_ident(
        semantic,
        line_index,
        sfc_source,
        script_offset,
        expression,
        api,
        index,
        call.span,
        offset,
        node_id,
        owner.callable,
        block,
      );
      if index == 0
        && api == Some("watch")
        && let Expression::ArrayExpression(array) = expression.get_inner_expression()
      {
        for element in &array.elements {
          let Some(inner) = element.as_expression() else {
            continue;
          };
          self.record_arg_ident(
            semantic,
            line_index,
            sfc_source,
            script_offset,
            inner,
            api,
            0,
            call.span,
            offset,
            node_id,
            owner.callable,
            block,
          );
        }
      }
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "call-arg indexing needs the mapped call site plus argument identity"
  )]
  fn record_arg_ident(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    expression: &Expression<'_>,
    api: Option<&'static str>,
    index: usize,
    call_span: Span,
    call_offset: usize,
    node_id: NodeId,
    callable: Option<NodeId>,
    block: NodeId,
  ) {
    let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, identifier) else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, identifier.span).offset;
    self.arg_uses.entry(symbol_id).or_default().push(ArgUse {
      api,
      index: u32::try_from(index).unwrap_or(u32::MAX),
      call_span,
      offset: if offset == 0 { call_offset } else { offset },
      node_id,
      callable,
      block,
    });
  }

  fn mark_escape_expr(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    for_toref: bool,
  ) {
    if let Some(identifier) = expression.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.escape_root(self.root_of(symbol_id), for_toref);
    }
  }

  fn escape_root(&mut self, root: SymbolId, for_toref: bool) {
    self.escaped.insert(root);
    if for_toref {
      self.toref_helper_escape.insert(root);
    }
    self
      .until
      .escapes
      .entry(root)
      .or_default()
      .push(UntilEscapeSite { offset: 0, until_borrow: self.until.pending_borrow });
  }

  fn index_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    offset: usize,
    operator: AssignmentOperator,
    left: &AssignmentTarget<'_>,
    right: &Expression<'_>,
  ) {
    if assignment_poisons_clone_intrinsic(semantic, left, &self.work) {
      self.clone_intrinsic_poisoned = true;
    }
    self.note_native_prototype_assignment(semantic, left);
    let simple = operator == AssignmentOperator::Assign;
    let fresh = simple && is_fresh_allocation(right, |ident| reference_symbol(semantic, ident));
    let owner = self.owner(node_id);
    let callable = owner.callable;
    let block = owner.block.unwrap_or(node_id);
    match left {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
      }
      AssignmentTarget::StaticMemberExpression(member) => {
        self.record_static_member_assignment(
          semantic,
          &member.object,
          member.property.name.as_str(),
          DirectMemberWrite {
            offset,
            callable,
            block,
            right,
            simple,
            fresh,
            span: member.span,
            node_id,
          },
        );
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          let root = self.root_of(symbol_id);
          self.unknown_member_touch.insert(root);
          self.capability_touch.insert(root);
          self.capability_poisoned.insert(root);
        }
      }
      AssignmentTarget::ObjectAssignmentTarget(_) | AssignmentTarget::ArrayAssignmentTarget(_) => {
        self.mark_pattern_uncertain(semantic, left);
      }
      _ => {}
    }
    self.record_snapshot_assignment(semantic, node_id, offset, simple, left, right);
  }

  fn record_static_member_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
    property: &str,
    write: DirectMemberWrite<'_>,
  ) {
    let Some(object) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let root = self.root_of(symbol_id);
    self.capability_poisoned.insert(root);
    if property == TOREF_CAPABILITY_KEY {
      self.toref_identity_uncertain.insert(root);
    }
    if property != "value" {
      self.capability_touch.insert(root);
    }
    if property == "value" {
      let event = ValueWrite {
        offset: write.offset,
        callable: write.callable,
        block: write.block,
        span: write.span,
        rhs: write.right.span(),
        literal: write_literal(semantic, write.right),
        simple_assign: write.simple,
        fresh_alloc: write.fresh,
        node_id: write.node_id,
      };
      self.value_write_events.entry(root).or_default().push(event);
      if write.simple {
        self.value_write_roots.insert(root);
        self.value_writes.entry(root).or_default().push(event);
      } else {
        self.uncertain.insert(root);
        self.until.uncertain_value_writes.insert(root);
      }
    } else {
      self.member_write_roots.insert(root);
      self.member_writes.entry((root, property.to_string())).or_default().push(MemberWrite {
        offset: write.offset,
        callable: write.callable,
        block: write.block,
        rhs: write.right.span(),
        simple_assign: write.simple,
        fresh_alloc: write.fresh,
      });
    }
  }

  fn record_snapshot_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    offset: usize,
    simple: bool,
    left: &AssignmentTarget<'_>,
    right: &Expression<'_>,
  ) {
    let owner = self.owner(node_id);
    let callable = owner.callable;
    match left {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => match identifier.name.as_str() {
        "Date" => self.date_poisoned = true,
        "JSON" => self.json_poisoned = true,
        "String" => self.string_capability_poisoned = true,
        _ => {}
      },
      AssignmentTarget::StaticMemberExpression(member) => {
        let property = member.property.name.as_str();
        if poisons_native_date(&member.object, property, |ident| reference_symbol(semantic, ident))
        {
          self.date_poisoned = true;
        }
        if poisons_date_tojson(&member.object, property, |ident| reference_symbol(semantic, ident))
        {
          self.date_poisoned = true;
        }
        if poisons_json(&member.object, property) {
          self.json_poisoned = true;
        }
        if poisons_string_capability(&member.object, property, |ident| {
          reference_symbol(semantic, ident)
        }) {
          self.string_capability_poisoned = true;
        }
        if property == "value"
          && let Some((ident, keys)) = peel_member_chain(&member.object, &self.work)
          && let Some(symbol_id) = reference_symbol(semantic, ident)
          && !keys.is_empty()
        {
          let region = region_of(owner, node_id);
          self
            .path_value_writes
            .entry(self.root_of(symbol_id))
            .or_default()
            .push((keys, PathWrite { offset, callable, region, simple_assign: simple }));
        }
        if let Some((ident, keys)) = peel_static_chain(&member.object, &self.work)
          && keys.as_slice() == ["value"]
          && let Some(symbol_id) = reference_symbol(semantic, ident)
        {
          self.push_nested_write(
            self.root_of(symbol_id),
            member.property.name.as_str().to_string(),
            NestedWrite {
              offset,
              span: member.span,
              callable,
              region: region_of(owner, node_id),
              rhs: right.span(),
              simple_assign: simple,
            },
          );
        }
        if let Some(ident) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, ident)
          && let Some(root) = self.value_object_alias.get(&self.root_of(symbol_id)).copied()
        {
          self.push_nested_write(
            root,
            member.property.name.as_str().to_string(),
            NestedWrite {
              offset,
              span: member.span,
              callable,
              region: region_of(owner, node_id),
              rhs: right.span(),
              simple_assign: simple,
            },
          );
        }
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        if let Some(key) = computed_literal_key(&member.expression) {
          if poisons_json(&member.object, &key) {
            self.json_poisoned = true;
          }
          if poisons_string_capability(&member.object, &key, |ident| {
            reference_symbol(semantic, ident)
          }) {
            self.string_capability_poisoned = true;
          }
          if poisons_date_tojson(&member.object, &key, |ident| reference_symbol(semantic, ident)) {
            self.date_poisoned = true;
          }
        }
        if let Some((ident, keys)) = peel_static_chain(&member.object, &self.work)
          && keys.as_slice() == ["value"]
          && let Some(symbol_id) = reference_symbol(semantic, ident)
          && let Some(key) = computed_literal_key(&member.expression)
        {
          self.push_nested_write(
            self.root_of(symbol_id),
            key,
            NestedWrite {
              offset,
              span: member.span,
              callable,
              region: region_of(owner, node_id),
              rhs: right.span(),
              simple_assign: simple,
            },
          );
        }
      }
      _ => {}
    }
  }

  fn push_nested_write(&mut self, root: SymbolId, property: String, write: NestedWrite) {
    self.work.add_writes(1);
    self.nested_writes.entry(root).or_default().push((property, write));
  }

  fn mark_pattern_uncertain(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTarget<'_>,
  ) {
    match target {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
      }
      AssignmentTarget::StaticMemberExpression(member) => {
        self.record_pattern_static_member(semantic, &member.object, member.property.name.as_str());
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        self.record_pattern_dynamic_member(semantic, &member.object);
      }
      AssignmentTarget::ObjectAssignmentTarget(object) => {
        for property in &object.properties {
          match property {
            AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
              self.mark_maybe_default_uncertain(semantic, &property.binding);
            }
            AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
              if let Some(symbol_id) = reference_symbol(semantic, &property.binding) {
                self.reassigned.insert(self.root_of(symbol_id));
              }
            }
          }
        }
        if let Some(rest) = &object.rest {
          self.mark_pattern_uncertain(semantic, &rest.target);
        }
      }
      AssignmentTarget::ArrayAssignmentTarget(array) => {
        for element in array.elements.iter().flatten() {
          self.mark_maybe_default_uncertain(semantic, element);
        }
        if let Some(rest) = &array.rest {
          self.mark_pattern_uncertain(semantic, &rest.target);
        }
      }
      AssignmentTarget::TSAsExpression(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::TSSatisfiesExpression(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::TSNonNullExpression(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::TSTypeAssertion(inner) => {
        self.mark_expression_target_uncertain(semantic, &inner.expression);
      }
      AssignmentTarget::PrivateFieldExpression(_) => {}
    }
  }

  /// Patterns restore generic uncertainty on the member object. Ordinary
  /// `state.n` keeps generic uncertainty only; `__v_isRef` and dynamic targets
  /// also mark toRef identity, and capability keys mark the dedicated
  /// capability set.
  fn record_pattern_static_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
    property: &str,
  ) {
    let Some(object) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let root = self.root_of(symbol_id);
    self.uncertain.insert(root);
    if property == "value" {
      self.until.uncertain_value_writes.insert(root);
    }
    self.capability_poisoned.insert(root);
    if property == TOREF_CAPABILITY_KEY {
      self.toref_identity_uncertain.insert(root);
    }
    if is_capability_key(property) {
      self.capability_uncertain.insert(root);
    }
  }

  fn record_pattern_dynamic_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
  ) {
    let Some(object) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, object) else {
      return;
    };
    let root = self.root_of(symbol_id);
    self.unknown_member_touch.insert(root);
    self.uncertain.insert(root);
    self.toref_identity_uncertain.insert(root);
    self.capability_uncertain.insert(root);
    self.capability_touch.insert(root);
    self.capability_poisoned.insert(root);
  }

  fn mark_expression_target_uncertain(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
      }
      Expression::StaticMemberExpression(member) => {
        self.record_pattern_static_member(semantic, &member.object, member.property.name.as_str());
      }
      Expression::ComputedMemberExpression(member) => {
        self.record_pattern_dynamic_member(semantic, &member.object);
      }
      _ => {}
    }
  }

  fn mark_maybe_default_uncertain(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTargetMaybeDefault<'_>,
  ) {
    match target {
      AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
        self.mark_pattern_uncertain(semantic, &with_default.binding);
      }
      other => {
        if let Some(assignment_target) = other.as_assignment_target() {
          self.mark_pattern_uncertain(semantic, assignment_target);
        }
      }
    }
  }

  fn record_static_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    member: &StaticMemberExpression<'_>,
  ) {
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let use_site = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, member.span).offset,
      span: member.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, node_id, &self.work),
      role: classify_role(semantic, node_id, &self.work),
    };
    let property = member.property.name.as_str();
    let object = member.object.get_inner_expression();
    if let Some(ident) = object.get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      let root = self.root_of(symbol_id);
      if property == "prototype" {
        self.prototype_touch.insert(root);
      }
      if property == "value" {
        if use_site.role.needs_get() || use_site.role.needs_set() {
          self.value_reads.entry(root).or_default().push(use_site);
        }
      } else {
        self
          .member_reads_by_root
          .entry(root)
          .or_default()
          .push(NamedUse { key: property.to_string(), site: use_site });
      }
    }
    if property == "value"
      && let Expression::StaticMemberExpression(inner) = object
      && let Some(ident) = inner.object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      let root = self.root_of(symbol_id);
      let key = inner.property.name.as_str().to_string();
      self.chained_value_by_root.entry(root).or_default().push(NamedUse { key, site: use_site });
    }
    if !use_site.optional
      && !member.optional
      && let Some((ident, mut keys)) = peel_member_chain(&member.object, &self.work)
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      self.work.add_queries(1);
      keys.push(property.to_string());
      self
        .path_reads
        .entry(self.root_of(symbol_id))
        .or_default()
        .push(PathRead { keys, site: use_site });
    }
  }

  fn record_member_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Some((object, key)) = member_call_object_key(call.callee.get_inner_expression()) else {
      return;
    };
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let object = object.get_inner_expression();
    if let Some(ident) = object.get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, ident)
    {
      let use_site = MemberUse {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
        optional,
        call_optional: call.optional,
        reach: classify_reach(semantic, node_id, &self.work),
        role: DemandRole::Other,
      };
      let named = NamedUse { key: key.to_string(), site: use_site };
      let root = self.root_of(symbol_id);
      if named.key == "stop" && self.demand_ok(&use_site) {
        self.stops_by_region.entry((root, use_site.callable, region)).or_default().push(use_site);
        self.stop_offsets.push(use_site.offset);
      }
      self.member_call_by_span.insert(span_key(call.span), named.clone());
      self.until.result_method_calls.entry(root).or_default().push(NamedUse {
        key: named.key.clone(),
        site: MemberUse {
          reach: classify_reach_except_chain(semantic, node_id, &self.work),
          ..use_site
        },
      });
      self.member_calls_by_root.entry(root).or_default().push(NamedUse {
        key: named.key,
        site: MemberUse {
          reach: classify_reach_except_chain(semantic, node_id, &self.work),
          call_optional: call.optional,
          ..use_site
        },
      });
      return;
    }
    if let Expression::AwaitExpression(awaited) = object {
      let use_site = MemberUse {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
        optional,
        call_optional: call.optional,
        reach: classify_reach_except_chain(semantic, node_id, &self.work),
        role: DemandRole::Other,
      };
      let named = NamedUse { key: key.to_string(), site: use_site };
      self.member_call_by_span.insert(span_key(call.span), named.clone());
      self.until.await_method_calls.entry(span_key(awaited.span)).or_default().push(named);
    }
    if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() {
      let use_site = MemberUse {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
        optional,
        call_optional: call.optional,
        reach: classify_reach(semantic, node_id, &self.work),
        role: DemandRole::Other,
      };
      self.record_path_call(semantic, member, use_site);
    }
  }

  fn record_path_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    member: &StaticMemberExpression<'_>,
    site: MemberUse,
  ) {
    if site.optional {
      return;
    }
    let Some((ident, keys)) = peel_static_chain(&member.object, &self.work) else {
      return;
    };
    if keys.is_empty() {
      return;
    }
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let root = self.root_of(symbol_id);
    let path = keys.into_iter().map(str::to_string).collect();
    self.path_calls.entry(root).or_default().push(PathCall {
      keys: path,
      method: member.property.name.as_str().to_string(),
      site,
    });
  }

  fn record_flow(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    self.record_event(semantic, line_index, sfc_source, script_offset, node_id, span);
    self.record_control_event(node_id, line_index, sfc_source, script_offset, span);
  }

  fn record_control_event(
    &mut self,
    node_id: NodeId,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    span: Span,
  ) {
    let Some(block) = self.owner(node_id).block else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.control_events_by_block.entry(block).or_default().push(offset);
  }

  fn record_pause_event(
    &mut self,
    node_id: NodeId,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    span: Span,
  ) {
    let Some(block) = self.owner(node_id).block else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.pause_events_by_block.entry(block).or_default().push(offset);
  }

  fn record_identifier_call(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Some(ident) = call.callee.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let block = owner.block.unwrap_or(node_id);
    let has_spread = call.arguments.iter().any(Argument::is_spread);
    let argc = u8::try_from(call.arguments.len()).unwrap_or(u8::MAX);
    let use_site = CallUse {
      offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
      span: call.span,
      callable: owner.callable,
      region,
      block,
      optional,
      reach: classify_reach(semantic, node_id, &self.work),
      argc,
      has_spread,
    };
    let root = self.root_of(symbol_id);
    if self.symbol_is_vueuse_producer(root) {
      self.producer_call_offsets.push(use_site.offset);
    }
    self.identifier_calls.entry(root).or_default().push(use_site);
    self.record_wrapped_result_demand(
      semantic,
      line_index,
      sfc_source,
      script_offset,
      node_id,
      root,
      use_site,
    );
  }

  #[expect(clippy::too_many_arguments, reason = "span mapping matches other record helpers")]
  fn record_wrapped_result_demand(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    root: SymbolId,
    inner: CallUse,
  ) {
    let parent = super::proof::skip_ts_parent(semantic, node_id, &self.work);
    let AstKind::StaticMemberExpression(member) = semantic.nodes().kind(parent) else {
      return;
    };
    let grand = super::proof::skip_ts_parent(semantic, parent, &self.work);
    let AstKind::CallExpression(outer) = semantic.nodes().kind(grand) else {
      return;
    };
    let optional = chain_optional(semantic, grand, &self.work);
    let owner = self.owner(grand);
    let region = region_of(owner, grand);
    let block = owner.block.unwrap_or(grand);
    let site = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, outer.span).offset,
      span: outer.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, grand, &self.work),
      role: DemandRole::Other,
    };
    self.result_demands.entry(root).or_default().push(ResultDemand {
      member: member.property.name.as_str().to_string(),
      site,
      inner,
      block,
    });
  }

  fn symbol_is_vueuse_producer(&self, root: SymbolId) -> bool {
    let Some(init) = self.init_span.get(&root).copied() else {
      return false;
    };
    let info = self.calls.get(&span_key(init)).copied().or_else(|| {
      let super::shape::ShapeHint::Call(span) = self.hints.get(&span_key(init)).copied()? else {
        return None;
      };
      self.calls.get(&span_key(span)).copied()
    });
    info.is_some_and(|call| call.vueuse.is_some() && !call.has_spread)
  }

  fn record_result_demand(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return;
    };
    let optional = chain_optional(semantic, node_id, &self.work);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let block = owner.block.unwrap_or(node_id);
    let site = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
      span: call.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, node_id, &self.work),
      role: DemandRole::Other,
    };
    let object = peel_ts(&member.object);
    let Expression::StaticMemberExpression(inner) = object else {
      return;
    };
    if inner.property.name.as_str() != "value" {
      return;
    }
    let Some(ident) = inner.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let value_read = MemberUse {
      offset: mapped(line_index, sfc_source, script_offset, inner.span).offset,
      span: inner.span,
      callable: owner.callable,
      region,
      optional,
      call_optional: false,
      reach: classify_reach(semantic, node_id, &self.work),
      role: DemandRole::Read,
    };
    self.value_demands.entry(self.root_of(symbol_id)).or_default().push(ValueDemand {
      member: member.property.name.as_str().to_string(),
      site,
      value_read,
      block,
    });
  }

  fn note_ctor_shadows(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    for symbol_id in semantic.scoping().symbol_ids() {
      self.work.add_references(1);
      match semantic.scoping().symbol_name(symbol_id) {
        "String" => {
          self.shadowed_ctors.insert("String");
        }
        "Number" => {
          self.shadowed_ctors.insert("Number");
        }
        "Boolean" => {
          self.shadowed_ctors.insert("Boolean");
        }
        "BigInt" => {
          self.shadowed_ctors.insert("BigInt");
        }
        "Object" => {
          self.shadowed_ctors.insert("Object");
        }
        "Symbol" => {
          self.shadowed_ctors.insert("Symbol");
        }
        "Date" => {
          self.shadowed_ctors.insert("Date");
        }
        "JSON" => {
          self.shadowed_ctors.insert("JSON");
        }
        _ => {}
      }
    }
  }

  #[expect(clippy::too_many_arguments, reason = "span mapping matches other record helpers")]
  fn record_native_symbol(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    symbol_id: SymbolId,
    init: &Expression<'_>,
  ) {
    if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
      return;
    }
    let Expression::CallExpression(call) = init.get_inner_expression() else {
      return;
    };
    if call.arguments.iter().any(Argument::is_spread) || call.arguments.len() > 1 {
      return;
    }
    let Some(identifier) = call.callee.get_inner_expression().get_identifier_reference() else {
      return;
    };
    if identifier.name.as_str() != "Symbol" || reference_symbol(semantic, identifier).is_some() {
      return;
    }
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    self.native_symbols.insert(
      symbol_id,
      NativeSymbol {
        offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
        span: call.span,
        callable: owner.callable,
        region,
      },
    );
  }

  fn record_injection(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Some(info) = self.calls.get(&span_key(call.span)).copied() else {
      return;
    };
    let Some(key_expr) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Some(ident) = key_expr.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = reference_symbol(semantic, ident) else {
      return;
    };
    let root = self.root_of(symbol_id);
    let owner = self.owner(node_id);
    let region = region_of(owner, node_id);
    let site = InjectionSite {
      offset: mapped(line_index, sfc_source, script_offset, call.span).offset,
      span: call.span,
      callable: owner.callable,
      region,
      block: owner.block.unwrap_or(node_id),
      reach: classify_reach(semantic, node_id, &self.work),
      optional: chain_optional(semantic, node_id, &self.work),
      argc: info.arg_count,
      has_spread: info.has_spread,
      payload: info.second_arg,
      factory: call.arguments.get(2).and_then(Argument::as_expression).and_then(boolean_literal),
      node_id,
    };
    match info.api {
      Some("provide") => self.provides_by_key.entry(root).or_default().push(site),
      Some("inject") => self.injects_by_key.entry(root).or_default().push(site),
      _ => {}
    }
  }

  fn note_native_prototype_assignment(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    left: &AssignmentTarget<'_>,
  ) {
    let (object, last_key) = match left {
      AssignmentTarget::StaticMemberExpression(member) => {
        (&member.object, Some(member.property.name.as_str()))
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        (&member.object, expression_static_key(&member.expression))
      }
      _ => return,
    };
    if prototype_receiver_is_native_ctor(semantic, object, last_key) {
      self.prototype_mutated = true;
    }
  }

  fn record_destructure(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    pattern: &BindingPattern<'_>,
    init: &Expression<'_>,
  ) {
    let Some(ident) = init.get_inner_expression().get_identifier_reference() else {
      if let BindingPattern::ObjectPattern(object) = pattern
        && let Expression::CallExpression(call) = init.get_inner_expression()
      {
        self.record_object_destructure_from_call(semantic, object, call.span);
      }
      return;
    };
    let Some(object_symbol) = reference_symbol(semantic, ident) else {
      return;
    };
    let root = self.root_of(object_symbol);
    let BindingPattern::ObjectPattern(object) = pattern else {
      return;
    };
    if object.rest.is_some() {
      self.uncertain.insert(root);
      return;
    }
    for property in &object.properties {
      let Some(key) = property.key.static_name() else {
        self.uncertain.insert(root);
        return;
      };
      if let BindingPattern::BindingIdentifier(binding) = &property.value
        && let Some(local) = binding.symbol_id.get()
      {
        self.destructure_by_object.entry(root).or_default().push((local, key.to_string()));
      }
    }
  }

  fn record_object_destructure_from_call(
    &mut self,
    _semantic: &oxc_semantic::Semantic<'_>,
    object: &oxc_ast::ast::ObjectPattern<'_>,
    call_span: Span,
  ) {
    if object.rest.is_some() {
      return;
    }
    let mut bindings = Vec::new();
    for property in &object.properties {
      let Some(key) = property.key.static_name() else {
        return;
      };
      if let BindingPattern::BindingIdentifier(binding) = &property.value
        && let Some(local) = binding.symbol_id.get()
      {
        self.work.add_key_copies(1);
        let name = key.to_string();
        self.destructure_by_object.entry(local).or_default().push((local, name.clone()));
        bindings.push((local, name));
      }
    }
    if !bindings.is_empty() {
      self.call_destructure.insert(span_key(call_span), bindings);
    }
  }

  fn index_vueuse_value_writes(&mut self) {
    let mut value_writes_by_callable: HashMap<(SymbolId, Option<NodeId>), Vec<ValueWrite>> =
      HashMap::new();
    for (root, writes) in &self.value_write_events {
      for write in writes {
        value_writes_by_callable.entry((*root, write.callable)).or_default().push(*write);
      }
    }
    self.value_writes_by_callable = value_writes_by_callable;
  }

  fn build_until_await_lookups(&mut self) {
    for site in &self.until.awaits {
      self.until.await_by_argument.insert(span_key(site.argument), *site);
      self.until.awaits_by_region.entry((site.callable, site.region)).or_default().push(*site);
      if let Some(symbol_id) = site.bound {
        let root = self.root_of(symbol_id);
        self.until.await_by_bound.entry(root).or_default().push(*site);
      }
    }
  }

  fn summarize_until_closed_sources(&mut self) {
    let roots: Vec<_> = self.init_span.keys().copied().collect();
    for root in roots {
      self.work.add_queries(1);
      let foreign_escape = self.until.escapes.get(&root).is_some_and(|sites| {
        sites.iter().any(|site| {
          self.work.add_queries(1);
          !site.until_borrow
        })
      });
      let Some(init_span) = self.init_span.get(&root).copied() else {
        continue;
      };
      let closed = !foreign_escape
        && !self.mixed_value_owners.contains(&root)
        && !self.reassigned.contains(&root)
        && !self.until.uncertain_value_writes.contains(&root)
        && !self.unknown_member_touch.contains(&root)
        && !self.capability_touch.contains(&root);
      self.until.closed_sources.insert(root, UntilClosedSource { init_span, closed });
    }
  }

  fn summarize_closed_keys(&mut self) {
    for (key, entries) in &self.objects {
      let mut keys = HashSet::new();
      let mut closed = true;
      for entry in entries {
        self.work.add_object_entries(1);
        match entry {
          ObjectEntry::Data { name, .. } => {
            if is_custom_prototype_key(name) {
              closed = false;
              break;
            }
            self.work.add_key_copies(1);
            keys.insert(name.clone());
          }
          ObjectEntry::Spread
          | ObjectEntry::Computed
          | ObjectEntry::Accessor { .. }
          | ObjectEntry::Method { .. } => {
            closed = false;
            break;
          }
        }
      }
      if closed {
        self.closed_keys.insert(*key, keys);
      }
    }
  }

  fn record_barrier(
    &mut self,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let Some(region) = self.owner(node_id).region else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.barriers_by_region.entry(region).or_default().push(offset);
  }

  fn record_barrier_end(
    &mut self,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let Some(region) = self.owner(node_id).region else {
      return;
    };
    let mapped_span = mapped(line_index, sfc_source, script_offset, span);
    let offset = mapped_span.offset.saturating_add(mapped_span.length);
    self.barriers_by_region.entry(region).or_default().push(offset);
  }

  fn record_class(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    class: &oxc_ast::ast::Class<'_>,
  ) {
    let Some(symbol_id) = class_binding_symbol(semantic, node_id, class) else {
      return;
    };
    let record = analyze_class(class, &self.work);
    self.classes.insert(symbol_id, record);
  }

  fn record_class_new(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &oxc_ast::ast::NewExpression<'_>,
  ) {
    let callee = expression
      .callee
      .get_inner_expression()
      .get_identifier_reference()
      .and_then(|ident| reference_symbol(semantic, ident));
    self.class_news.insert(span_key(expression.span), class_new_info(expression, callee));
  }

  fn record_stmt_site(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
  ) {
    let parent_id = semantic.nodes().parent_id(node_id);
    let AstKind::ExpressionStatement(statement) = semantic.nodes().kind(parent_id) else {
      return;
    };
    let block_id = semantic.nodes().parent_id(parent_id);
    if !block_is_straight_line(semantic, block_id) {
      return;
    }
    let span = mapped(line_index, sfc_source, script_offset, statement.span);
    let owner = self.owner(node_id);
    self.stmt_site.insert(
      node_id,
      StmtSite { block: block_id, callable: owner.callable, expr_offset: span.offset },
    );
  }

  fn record_event(
    &mut self,
    _semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let Some(block) = self.owner(node_id).block else {
      return;
    };
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.events_by_block.entry(block).or_default().push(offset);
  }

  fn has_callable_termination_between(
    &self,
    callable: Option<NodeId>,
    start: usize,
    end: usize,
  ) -> bool {
    let Some(offsets) = self.terminations_by_callable.get(&callable) else {
      self.work.add_queries(1);
      return false;
    };
    let index = self.work.partition_point(offsets, |offset| *offset <= start);
    self.work.add_queries(1);
    offsets.get(index).is_some_and(|offset| *offset < end)
  }

  fn call_reach_is_straight(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut node_id: NodeId,
  ) -> bool {
    for _ in 0..MAX_ROLE_ANCESTORS {
      self.work.add_queries(1);
      let parent = semantic.nodes().parent_id(node_id);
      let current_span = semantic.nodes().kind(node_id).span();
      match semantic.nodes().kind(parent) {
        AstKind::Program(_) | AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
          return true;
        }
        AstKind::LogicalExpression(logical) => {
          if callee_span_matches(&logical.left, current_span) {
            node_id = parent;
            continue;
          }
          return false;
        }
        AstKind::ConditionalExpression(conditional) => {
          if callee_span_matches(&conditional.test, current_span) {
            node_id = parent;
            continue;
          }
          return false;
        }
        AstKind::IfStatement(statement) => {
          if callee_span_matches(&statement.test, current_span) {
            node_id = parent;
            continue;
          }
          return false;
        }
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::TSInstantiationExpression(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::VariableDeclarator(_)
        | AstKind::VariableDeclaration(_)
        | AstKind::BlockStatement(_)
        | AstKind::FunctionBody(_)
        | AstKind::StaticMemberExpression(_)
        | AstKind::ComputedMemberExpression(_)
        | AstKind::CallExpression(_)
        | AstKind::NewExpression(_)
        | AstKind::UnaryExpression(_)
        | AstKind::UpdateExpression(_)
        | AstKind::AssignmentExpression(_)
        | AstKind::SequenceExpression(_)
        | AstKind::ArrayExpression(_)
        | AstKind::ObjectExpression(_)
        | AstKind::SpreadElement(_)
        | AstKind::ReturnStatement(_)
        | AstKind::ThrowStatement(_) => {
          node_id = parent;
        }
        _ => return false,
      }
    }
    false
  }

  fn precompute_aliases(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let mut pending_prototype_aliases = Vec::new();
    for node in semantic.nodes() {
      self.work.add_nodes(1);
      let AstKind::VariableDeclarator(declarator) = node.kind() else {
        continue;
      };
      let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
        continue;
      };
      let Some(local) = binding.symbol_id.get() else {
        continue;
      };
      if !semantic.scoping().symbol_flags(local).contains(SymbolFlags::ConstVariable) {
        continue;
      }
      let Some(init) = &declarator.init else {
        continue;
      };
      let inner = init.get_inner_expression();
      if let Expression::StaticMemberExpression(member) = inner {
        self.note_prototype_alias(semantic, local, member, &mut pending_prototype_aliases);
        continue;
      }
      let Some(identifier) = inner.get_identifier_reference() else {
        continue;
      };
      match reference_symbol(semantic, identifier) {
        Some(target) => {
          self.work.add_queries(1);
          self.alias_root.insert(local, target);
        }
        None if identifier.name.as_str() == "globalThis" => {
          self.global_this_aliases.insert(local);
        }
        None => {
          if let Some(ctor) = intern_native_ctor(identifier.name.as_str()) {
            self.record_native_ctor_alias(local, ctor);
          }
        }
      }
    }
    self.canonicalize_aliases();
    self.resolve_prototype_aliases(pending_prototype_aliases);
  }

  fn note_prototype_alias(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    local: SymbolId,
    member: &StaticMemberExpression<'_>,
    pending: &mut Vec<(SymbolId, SymbolId)>,
  ) {
    self.work.add_queries(1);
    if member.property.name.as_str() != "prototype" {
      return;
    }
    let Some(identifier) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    match reference_symbol(semantic, identifier) {
      Some(object) => {
        self.work.add_queries(1);
        pending.push((local, object));
      }
      None => {
        if let Some(ctor) = intern_native_ctor(identifier.name.as_str()) {
          self.record_native_ctor_alias(local, ctor);
        }
      }
    }
  }

  fn resolve_prototype_aliases(&mut self, pending: Vec<(SymbolId, SymbolId)>) {
    for (local, object) in pending {
      self.work.add_queries(1);
      if let Some(ctor) = self.native_ctor_kind(self.root_of(object)) {
        self.record_native_ctor_alias(local, ctor);
      }
    }
  }

  fn record_native_ctor_alias(&mut self, local: SymbolId, ctor: &'static str) {
    self.work.add_queries(1);
    self.native_ctor_aliases.insert(local, ctor);
  }

  fn native_ctor_kind(&self, root: SymbolId) -> Option<&'static str> {
    self.work.add_queries(1);
    self.native_ctor_aliases.get(&root).copied()
  }

  fn native_ctor_of_identifier(
    &self,
    identifier: &IdentifierReference<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
  ) -> Option<&'static str> {
    self.work.add_queries(1);
    reference_symbol(semantic, identifier).map_or_else(
      || intern_native_ctor(identifier.name.as_str()),
      |symbol_id| self.native_ctor_kind(self.root_of(symbol_id)),
    )
  }

  fn canonicalize_aliases(&mut self) {
    let locals: Vec<SymbolId> = self.alias_root.keys().copied().collect();
    for local in locals {
      self.work.add_queries(1);
      let mut current = local;
      let mut depth = 0_u8;
      loop {
        self.work.add_queries(1);
        let Some(&next) = self.alias_root.get(&current) else {
          break;
        };
        if next == current {
          break;
        }
        depth = depth.saturating_add(1);
        if depth > MAX_ALIAS_DEPTH {
          self.capability_poisoned.insert(local);
          current = local;
          break;
        }
        current = next;
      }
      self.alias_root.insert(local, current);
    }
  }

  fn poison_expr(&mut self, semantic: &oxc_semantic::Semantic<'_>, expression: &Expression<'_>) {
    self.poison_expr_bounded(semantic, expression, MAX_ESCAPE_DEPTH);
  }

  fn poison_expr_bounded(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    remaining: u8,
  ) {
    self.work.add_queries(1);
    if remaining == 0 {
      self.poison_unresolved_escape(semantic, expression);
      return;
    }
    let next = remaining.saturating_sub(1);
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.capability_poisoned.insert(self.root_of(symbol_id));
        }
        self.taint_native_ctor_identifier(identifier, semantic);
        self.taint_if_global_object(semantic, identifier);
      }
      Expression::LogicalExpression(logical) => {
        self.poison_expr_bounded(semantic, &logical.left, next);
        self.poison_expr_bounded(semantic, &logical.right, next);
      }
      Expression::ConditionalExpression(conditional) => {
        self.poison_expr_bounded(semantic, &conditional.consequent, next);
        self.poison_expr_bounded(semantic, &conditional.alternate, next);
      }
      Expression::SequenceExpression(sequence) => {
        if let Some(last) = sequence.expressions.last() {
          self.poison_expr_bounded(semantic, last, next);
        }
      }
      Expression::ArrayExpression(array) => {
        for element in &array.elements {
          if let Some(element) = element.as_expression() {
            self.poison_expr_bounded(semantic, element, next);
          }
        }
      }
      Expression::ObjectExpression(object) => {
        for property in &object.properties {
          if let ObjectPropertyKind::ObjectProperty(property) = property {
            self.poison_expr_bounded(semantic, &property.value, next);
          }
        }
      }
      inner => self.taint_native_prototype(semantic, inner),
    }
  }

  fn poison_unresolved_escape(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    let inner = expression.get_inner_expression();
    if let Some(identifier) = inner.get_identifier_reference() {
      if let Some(symbol_id) = reference_symbol(semantic, identifier) {
        self.capability_poisoned.insert(self.root_of(symbol_id));
      }
      self.taint_native_ctor_identifier(identifier, semantic);
      self.taint_if_global_object(semantic, identifier);
      return;
    }
    self.unresolved_escape_spans.push(expression.span());
  }

  fn merged_unresolved_escape_ranges(&self) -> Vec<(u32, u32)> {
    self.work.add_queries(1);
    if self.unresolved_escape_spans.is_empty() {
      return Vec::new();
    }
    let mut ranges = Vec::with_capacity(self.unresolved_escape_spans.len());
    for span in &self.unresolved_escape_spans {
      self.work.add_queries(1);
      ranges.push((span.start, span.end));
    }
    self.work.sort_unstable(&mut ranges);
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for range in ranges {
      self.work.add_queries(1);
      match merged.last_mut() {
        Some(last) if range.0 <= last.1 => last.1 = last.1.max(range.1),
        _ => merged.push(range),
      }
    }
    merged
  }

  fn unresolved_escape_covers(&self, ranges: &[(u32, u32)], span: Span) -> bool {
    let index = self.work.partition_point(ranges, |range| range.0 <= span.start);
    self.work.add_queries(1);
    index
      .checked_sub(1)
      .and_then(|index| ranges.get(index))
      .is_some_and(|range| span.end <= range.1)
  }

  fn taint_native_ctor_identifier(
    &mut self,
    identifier: &IdentifierReference<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
  ) {
    if let Some(ctor) = self.native_ctor_of_identifier(identifier, semantic) {
      self.tainted_ctors.insert(ctor);
    }
  }

  fn taint_if_global_object(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    identifier: &IdentifierReference<'_>,
  ) {
    if self.is_global_this_ref(semantic, identifier) {
      self.taint_all_native_ctors();
    }
  }

  fn taint_all_native_ctors(&mut self) {
    self.work.add_queries(1);
    for name in ["Map", "Set", "Array"] {
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      }
    }
  }

  fn index_unresolved_global_ref(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    identifier: &IdentifierReference<'_>,
  ) {
    let Some(name) = intern_unresolved_global_name(identifier.name.as_str()) else {
      return;
    };
    if reference_symbol(semantic, identifier).is_some() {
      return;
    }
    self.work.add_references(1);
    self.unresolved_global_refs.push((identifier.span(), name));
  }

  fn apply_unresolved_global_escape_coverage(&mut self, leftover: &[(u32, u32)]) {
    let refs = std::mem::take(&mut self.unresolved_global_refs);
    for (span, name) in refs {
      self.work.add_references(1);
      if !self.unresolved_escape_covers(leftover, span) {
        continue;
      }
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      } else {
        self.taint_all_native_ctors();
      }
    }
  }

  fn taint_native_prototype(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    inner: &Expression<'_>,
  ) {
    let Expression::StaticMemberExpression(member) = inner else {
      return;
    };
    if member.property.name.as_str() != "prototype" {
      return;
    }
    let Some(identifier) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    if let Some(ctor) = self.native_ctor_of_identifier(identifier, semantic) {
      self.tainted_ctors.insert(ctor);
    }
  }

  fn taint_assignment_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTarget<'_>,
  ) {
    match target {
      AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      AssignmentTarget::StaticMemberExpression(member) => {
        self.taint_native_prototype(semantic, member.object.get_inner_expression());
        if member.property.name.as_str() == "prototype"
          && let Some(identifier) = member.object.get_inner_expression().get_identifier_reference()
        {
          self.taint_native_ctor_identifier(identifier, semantic);
        }
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      AssignmentTarget::ComputedMemberExpression(member) => {
        self.taint_native_prototype(semantic, member.object.get_inner_expression());
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      AssignmentTarget::ObjectAssignmentTarget(object) => {
        for property in &object.properties {
          match property {
            AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
              self.taint_maybe_default(semantic, &property.binding);
            }
            AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(_) => {}
          }
        }
        if let Some(rest) = &object.rest {
          self.taint_assignment_target(semantic, &rest.target);
        }
      }
      AssignmentTarget::ArrayAssignmentTarget(array) => {
        for element in array.elements.iter().flatten() {
          self.taint_maybe_default(semantic, element);
        }
        if let Some(rest) = &array.rest {
          self.taint_assignment_target(semantic, &rest.target);
        }
      }
      AssignmentTarget::TSAsExpression(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::TSSatisfiesExpression(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::TSNonNullExpression(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::TSTypeAssertion(inner) => {
        self.taint_expression_target(semantic, &inner.expression);
      }
      AssignmentTarget::PrivateFieldExpression(_) => {}
    }
  }

  fn taint_maybe_default(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &AssignmentTargetMaybeDefault<'_>,
  ) {
    match target {
      AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
        self.taint_assignment_target(semantic, &with_default.binding);
      }
      other => {
        if let Some(assignment_target) = other.as_assignment_target() {
          self.taint_assignment_target(semantic, assignment_target);
        }
      }
    }
  }

  fn taint_expression_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      Expression::StaticMemberExpression(member) => {
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      Expression::ComputedMemberExpression(member) => {
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      _ => {}
    }
  }

  fn taint_global_ctor_member(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
    static_key: Option<&str>,
    computed_key: Option<&Expression<'_>>,
  ) {
    self.work.add_queries(1);
    let Some(identifier) = object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    if !self.is_global_this_ref(semantic, identifier) {
      return;
    }
    if let Some(name) = static_key {
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      }
      return;
    }
    let Some(key) = computed_key else {
      return;
    };
    if let Some(name) = static_string_key(key) {
      if let Some(ctor) = intern_native_ctor(name) {
        self.tainted_ctors.insert(ctor);
      }
    } else {
      self.taint_all_native_ctors();
    }
  }

  fn is_global_this_ref(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    identifier: &IdentifierReference<'_>,
  ) -> bool {
    self.work.add_queries(1);
    reference_symbol(semantic, identifier).map_or_else(
      || identifier.name.as_str() == "globalThis",
      |symbol_id| self.global_this_aliases.contains(&self.root_of(symbol_id)),
    )
  }

  fn note_receiver_use(&mut self, semantic: &oxc_semantic::Semantic<'_>, callee: &Expression<'_>) {
    match callee.get_inner_expression() {
      Expression::StaticMemberExpression(member) => {
        if is_known_receiver_method(member.property.name.as_str()) {
          return;
        }
        self.poison_member_object(semantic, &member.object);
      }
      Expression::ComputedMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
      }
      _ => {}
    }
  }

  fn record_method_extraction(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    declarator: &VariableDeclarator<'_>,
  ) {
    let owner = self.owner(node_id);
    let callable = owner.callable;
    let block = owner.block.unwrap_or(node_id);
    match &declarator.id {
      BindingPattern::BindingIdentifier(binding) => {
        let Some(symbol_id) = binding.symbol_id.get() else {
          return;
        };
        if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
          return;
        }
        let Some(init) = &declarator.init else {
          return;
        };
        let Expression::StaticMemberExpression(member) = init.get_inner_expression() else {
          return;
        };
        let Some(method) = intern_extractable_method(member.property.name.as_str()) else {
          return;
        };
        let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(collection) = reference_symbol(semantic, object) else {
          return;
        };
        let extract_offset = mapped(line_index, sfc_source, script_offset, member.span).offset;
        self.extracted_methods.insert(
          symbol_id,
          ExtractedMethod {
            collection,
            method,
            extract_span: member.span,
            extract_offset,
            callable,
            block,
          },
        );
      }
      BindingPattern::ObjectPattern(pattern) => {
        if pattern.rest.is_some() {
          return;
        }
        let Some(init) = &declarator.init else {
          return;
        };
        let Some(object) = init.get_inner_expression().get_identifier_reference() else {
          return;
        };
        let Some(collection) = reference_symbol(semantic, object) else {
          return;
        };
        let mut pending = Vec::new();
        for property in &pattern.properties {
          let Some(name) = property.key.static_name() else {
            return;
          };
          let Some(method) = intern_extractable_method(name.as_ref()) else {
            return;
          };
          let BindingPattern::BindingIdentifier(binding) = &property.value else {
            return;
          };
          let Some(symbol_id) = binding.symbol_id.get() else {
            return;
          };
          if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
            return;
          }
          let extract_offset =
            mapped(line_index, sfc_source, script_offset, property.span()).offset;
          pending.push((
            symbol_id,
            ExtractedMethod {
              collection,
              method,
              extract_span: property.span(),
              extract_offset,
              callable,
              block,
            },
          ));
        }
        for (symbol_id, extracted) in pending {
          self.extracted_methods.insert(symbol_id, extracted);
        }
      }
      _ => {}
    }
  }

  fn poison_simple_target(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    target: &SimpleAssignmentTarget<'_>,
  ) {
    match target {
      SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      SimpleAssignmentTarget::StaticMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      SimpleAssignmentTarget::ComputedMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      SimpleAssignmentTarget::TSAsExpression(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::TSNonNullExpression(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::TSTypeAssertion(inner) => {
        self.poison_member_expression(semantic, &inner.expression);
      }
      SimpleAssignmentTarget::PrivateFieldExpression(_) => {}
    }
  }

  fn poison_member_expression(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
  ) {
    match expression.get_inner_expression() {
      Expression::Identifier(identifier) => {
        if let Some(symbol_id) = reference_symbol(semantic, identifier) {
          self.reassigned.insert(self.root_of(symbol_id));
        }
        self.taint_native_ctor_identifier(identifier, semantic);
      }
      Expression::StaticMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        self.taint_global_ctor_member(
          semantic,
          &member.object,
          Some(member.property.name.as_str()),
          None,
        );
      }
      Expression::ComputedMemberExpression(member) => {
        self.poison_member_object(semantic, &member.object);
        if let Some(object) = member.object.get_inner_expression().get_identifier_reference()
          && let Some(symbol_id) = reference_symbol(semantic, object)
        {
          self.unknown_member_touch.insert(self.root_of(symbol_id));
        }
        self.taint_global_ctor_member(semantic, &member.object, None, Some(&member.expression));
      }
      _ => {}
    }
  }

  fn poison_member_object(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    object: &Expression<'_>,
  ) {
    if let Some(identifier) = object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = reference_symbol(semantic, identifier)
    {
      self.capability_poisoned.insert(self.root_of(symbol_id));
    }
  }

  fn record_termination(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    line_index: &vue_vet_core::LineIndex,
    sfc_source: &str,
    script_offset: usize,
    node_id: NodeId,
    span: Span,
  ) {
    let owner = self.owner(node_id);
    let Some(block) = owner.block else {
      return;
    };
    if !block_is_straight_line(semantic, block) {
      return;
    }
    let offset = mapped(line_index, sfc_source, script_offset, span).offset;
    self.terminations_by_callable.entry(owner.callable).or_default().push(offset);
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
  let parent = skip_ts_parent(semantic, node_id);
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
  let parent = skip_ts_parent(semantic, node_id);
  let AstKind::CallExpression(call) = semantic.nodes().kind(parent) else {
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
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => {
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
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_)
      | AstKind::TSInstantiationExpression(_) => {
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
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => node_id = parent,
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
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => {
        options.flush = FlushKind::Unknown;
        options.immediate = OptionFlag::Unknown;
        options.once = OptionFlag::Unknown;
      }
      ObjectPropertyKind::ObjectProperty(property) => {
        let Some(name) = property.key.static_name() else {
          options.flush = FlushKind::Unknown;
          options.immediate = OptionFlag::Unknown;
          options.once = OptionFlag::Unknown;
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
        }
      }
    }
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
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => {
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

fn peel_ts<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
  let mut current = expression.get_inner_expression();
  for _ in 0..64 {
    current = match current {
      Expression::ParenthesizedExpression(inner) => inner.expression.get_inner_expression(),
      Expression::TSAsExpression(inner) => inner.expression.get_inner_expression(),
      Expression::TSSatisfiesExpression(inner) => inner.expression.get_inner_expression(),
      Expression::TSNonNullExpression(inner) => inner.expression.get_inner_expression(),
      Expression::TSTypeAssertion(inner) => inner.expression.get_inner_expression(),
      other => return other,
    };
  }
  current
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
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => current = parent_id,
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
use map_index::{
  array_is_map_entry, array_is_map_iterable, assignment_poisons_map_intrinsic,
  capability_mutating_callee, expression_poisons_map_intrinsic, known_map_constructor_key_role,
  literal_nullish, literal_truthy, object_has_skip_marker, simple_target_poisons_map_intrinsic,
  vue_wrapper_skips_capability,
};
pub(super) use map_index::{proxy_flavor, skip_ts_parent};
