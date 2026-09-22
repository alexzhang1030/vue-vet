//! Span-keyed sites recorded by the source-contract index.
//!
//! `Indexes` in the parent module owns the maps. These types stay here so
//! `mod.rs` is the scan and the query surface.

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::Expression;
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::Span;

use super::{DemandRole, Reach, Scalar, Timed, WorkCounter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) struct StmtSite {
  pub block: NodeId,
  pub callable: Option<NodeId>,
  pub expr_offset: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum WriteLiteral {
  Number(u64),
  Bool(bool),
  Undefined,
  Other,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct ValueWrite {
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
pub(in crate::source_contracts) struct MemberWrite {
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
pub(in crate::source_contracts) struct DirectMemberWrite<'a> {
  pub(in crate::source_contracts) offset: usize,
  pub(in crate::source_contracts) callable: Option<NodeId>,
  pub(in crate::source_contracts) block: NodeId,
  pub(in crate::source_contracts) right: &'a Expression<'a>,
  pub(in crate::source_contracts) simple: bool,
  pub(in crate::source_contracts) fresh: bool,
  pub(in crate::source_contracts) span: Span,
  pub(in crate::source_contracts) node_id: NodeId,
}

#[derive(Clone, Debug)]
pub(in crate::source_contracts) struct MemberInfo {
  pub object: Span,
  pub property: String,
  pub span: Span,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct CallInfo {
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
pub(in crate::source_contracts) struct PathCall {
  pub keys: Vec<String>,
  pub method: String,
  pub site: MemberUse,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct NestedWrite {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub rhs: Span,
  pub simple_assign: bool,
}

#[derive(Clone, Debug)]
pub(in crate::source_contracts) struct PathRead {
  pub keys: Vec<String>,
  pub site: MemberUse,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct PathWrite {
  pub offset: usize,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub simple_assign: bool,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct SnapshotCall {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub reach: Reach,
  pub optional: bool,
}

impl SnapshotCall {
  pub(in crate::source_contracts) const fn from_call_use(call: CallUse) -> Self {
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
pub(in crate::source_contracts) struct InjectionSite {
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
pub(in crate::source_contracts) struct NativeSymbol {
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub region: NodeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(in crate::source_contracts) enum ProxyFlavor {
  Deep,
  Shallow,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct MapKeyRef {
  pub span: Span,
  pub symbol: Option<SymbolId>,
  pub proxy_of: Option<SymbolId>,
  pub actual_proxy: bool,
  pub proxy_flavor: Option<ProxyFlavor>,
  pub is_literal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum ExecKind {
  Always,
  Never,
  Maybe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum ChainPlace {
  Inside,
  Outside,
  Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum WrapperOrigin {
  None,
  Known(SymbolId),
  Unknown,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct NewInfo {
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
pub(in crate::source_contracts) struct MemberCallSite {
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
pub(in crate::source_contracts) struct MapOp {
  pub method: Option<&'static str>,
  pub site: MemberCallSite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum CtorKeyState {
  Unknown,
  Known(usize),
}

pub(in crate::source_contracts) struct PendingMapKeyArg {
  pub(in crate::source_contracts) receiver: SymbolId,
  pub(in crate::source_contracts) method: &'static str,
  pub(in crate::source_contracts) arg: SymbolId,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct ExtractedMethod {
  pub collection: SymbolId,
  pub method: &'static str,
  pub extract_span: Span,
  pub extract_offset: usize,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct MemberUse {
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
pub(in crate::source_contracts) struct NamedUse {
  pub key: String,
  pub site: MemberUse,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct CallUse {
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
pub(in crate::source_contracts) struct ResultDemand {
  pub member: String,
  pub site: MemberUse,
  pub inner: CallUse,
  pub block: NodeId,
}

#[derive(Clone, Debug)]
pub(in crate::source_contracts) struct ValueDemand {
  pub member: String,
  pub site: MemberUse,
  pub value_read: MemberUse,
  pub block: NodeId,
}

#[derive(Clone, Debug)]
pub(in crate::source_contracts) enum ObjectEntry {
  Spread,
  Computed,
  Accessor { name: Option<String> },
  Data { name: String, key: Span, value: Span },
  Method { name: String, key: Span, value: Span },
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) enum ObjectProp {
  Unknown,
  Value(Span),
}

#[derive(Clone, Copy)]
pub(in crate::source_contracts) struct Owner {
  pub callable: Option<NodeId>,
  pub block: Option<NodeId>,
  pub region: Option<NodeId>,
}

#[derive(Clone, Debug)]
pub(in crate::source_contracts) struct FunctionInfo {
  #[expect(dead_code, reason = "node identity is stored for callback owner joins")]
  pub node_id: NodeId,
  pub params: Vec<Option<SymbolId>>,
  pub has_rest: bool,
  #[expect(dead_code, reason = "arrow expression flag is stored with the function index")]
  pub expression_arrow: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum FlushKind {
  Pre,
  Post,
  Sync,
  Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum OptionFlag {
  Default,
  On,
  Off,
  Unknown,
}

/// Which `ref` initializer walk `Indexes::ref_init_scalar` runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) enum RefInitLookup {
  /// Closed source. A missing argument is nullish.
  Closed,
  /// Init span through a hint-call. A missing argument is absent.
  Direct,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::source_contracts) struct WatchConsumerOptions {
  pub flush: FlushKind,
  pub immediate: OptionFlag,
  pub once: OptionFlag,
}

impl WatchConsumerOptions {
  pub(in crate::source_contracts) fn default_for(api: Option<&str>) -> Self {
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
  pub(in crate::source_contracts) fn immediately_active(self, api: Option<&str>) -> Option<bool> {
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
pub(in crate::source_contracts) struct ArgUse {
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
pub(in crate::source_contracts) struct IdentCall {
  #[expect(dead_code, reason = "call span is stored for stop/pause site identity")]
  pub span: Span,
  pub offset: usize,
  #[expect(dead_code, reason = "callable owner is stored for stop/pause ordering")]
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct ValueRead {
  pub node_id: NodeId,
  pub offset: usize,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

/// `.value` reads split by contract predicate. One static-member visit fills
/// the lanes; each predicate stays the one that lane had before they shared a visit.
#[derive(Default)]
pub(in crate::source_contracts) struct ValueReadLanes {
  pub(in crate::source_contracts) derivation: HashMap<SymbolId, Vec<ValueRead>>,
  pub(in crate::source_contracts) scheduling: HashMap<SymbolId, Vec<ValueRead>>,
  pub(in crate::source_contracts) custom_ref: HashMap<SymbolId, Vec<ValueRead>>,
  pub(in crate::source_contracts) scheduling_calls: HashMap<SymbolId, Vec<MemberCall>>,
}

impl ValueReadLanes {
  pub(in crate::source_contracts) fn record_custom_ref(
    &mut self,
    symbol_id: SymbolId,
    read: ValueRead,
  ) {
    self.custom_ref.entry(symbol_id).or_default().push(read);
  }

  pub(in crate::source_contracts) fn record_derivation(&mut self, root: SymbolId, read: ValueRead) {
    self.derivation.entry(root).or_default().push(read);
  }

  pub(in crate::source_contracts) fn record_scheduling(&mut self, root: SymbolId, read: ValueRead) {
    self.scheduling.entry(root).or_default().push(read);
  }

  pub(in crate::source_contracts) fn record_scheduling_call(
    &mut self,
    root: SymbolId,
    call: MemberCall,
  ) {
    self.scheduling_calls.entry(root).or_default().push(call);
  }

  pub(in crate::source_contracts) fn sort(&mut self, work: &WorkCounter) {
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

  pub(in crate::source_contracts) fn remap_roots(
    &mut self,
    mut root_of: impl FnMut(SymbolId) -> SymbolId,
    mut note: impl FnMut(),
  ) {
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
pub(in crate::source_contracts) struct MemberCall {
  pub offset: usize,
  pub method: &'static str,
  pub span: Span,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct AwaitSite {
  pub offset: usize,
  pub callee_api: Option<&'static str>,
  pub callable: Option<NodeId>,
  pub block: NodeId,
}

/// Await-position site: operand span, bound symbol, region, and reach.
#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct AwaitPositionSite {
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
pub(in crate::source_contracts) struct AwaitEscapeSite {
  pub offset: usize,
  pub until_borrow: bool,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct AwaitClosedSource {
  pub init_span: Span,
  pub closed: bool,
}

#[derive(Default)]
pub(in crate::source_contracts) struct AwaitIndexes {
  pub(in crate::source_contracts) await_method_calls: HashMap<u64, Vec<NamedUse>>,
  pub(in crate::source_contracts) result_method_calls: HashMap<SymbolId, Vec<NamedUse>>,
  pub(in crate::source_contracts) results_by_await: HashMap<u64, SymbolId>,
  pub(in crate::source_contracts) awaits: Vec<AwaitPositionSite>,
  pub(in crate::source_contracts) await_by_argument: HashMap<u64, AwaitPositionSite>,
  pub(in crate::source_contracts) await_by_bound: HashMap<SymbolId, Vec<AwaitPositionSite>>,
  pub(in crate::source_contracts) awaits_by_region:
    HashMap<(Option<NodeId>, NodeId), Vec<AwaitPositionSite>>,
  pub(in crate::source_contracts) escapes: HashMap<SymbolId, Vec<AwaitEscapeSite>>,
  pub(in crate::source_contracts) closed_sources: HashMap<SymbolId, AwaitClosedSource>,
  pub(in crate::source_contracts) scalars: HashMap<u64, Scalar>,
  pub(in crate::source_contracts) uncertain_value_writes: HashSet<SymbolId>,
  pub(in crate::source_contracts) pending_borrow: bool,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct DisposeSite {
  pub offset: usize,
  pub span: Span,
  pub child: Option<SymbolId>,
  pub callable: Option<NodeId>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::source_contracts) struct WatchConsumer {
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
pub(in crate::source_contracts) struct EffectCallback {
  pub offset: usize,
  pub api: &'static str,
}

macro_rules! timed_by_offset {
  ($($site:ty),* $(,)?) => {
    $(impl Timed for $site {
      fn at(&self) -> usize {
        self.offset
      }
    })*
  };
}

timed_by_offset!(
  ValueWrite,
  MemberWrite,
  MemberUse,
  ValueRead,
  CallUse,
  AwaitPositionSite,
  InjectionSite,
  IdentCall,
  ArgUse,
  AwaitSite,
  MemberCall,
);

impl Timed for NamedUse {
  fn at(&self) -> usize {
    self.site.offset
  }
}
