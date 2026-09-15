//! Neutral Vue API source-contract facts (issue #224).
//!
//! Produced by `vue_vet_oxc`; consumed by `vue_vet_rules`. No parser AST types.
//!
//! Normalization facts (`toref_ignored_key`, `effect_scope_callback`) compose
//! with source5, lost-notification, watch-API, and watch-callback fields.

use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

/// Proven Vue API contract sites collected from Oxc semantics.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceContractFacts {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub trigger_ref_non_ref: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub torefs_non_proxy: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub primitive_reactive_target: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_unwrapped_source: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_replaced_object_source: Vec<WatchReplacedObjectSourceFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_ignored_option: Vec<WatchIgnoredOptionFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_signature_mismatch: Vec<WatchSignatureMismatchFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_callback_contracts: Vec<WatchCallbackContractFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub toref_ignored_key: Vec<ToRefIgnoredKeyFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub effect_scope_callback: Vec<SourceContractSiteFact>,
  /// Proven Vue Proxy used as the data argument of native `structuredClone`.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub uncloneable_proxy_data: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub invalid_custom_ref_interface: Vec<InvalidCustomRefInterfaceFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub inactive_scope_result: Vec<InactiveScopeResultFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub missing_torefs_key: Vec<MissingToRefsKeyFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub extracted_reactive_collection_method: Vec<ExtractedReactiveCollectionMethodFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub raw_proxy_map_key: Vec<RawProxyMapKeyFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub keyed_map_dependency: Vec<KeyedMapDependencyFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub custom_ref_lost_notification: Vec<CustomRefLostNotificationFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub memoize_stale_result_demand: Vec<CachedResultDemandFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub controlled_computed_stale_result_demand: Vec<CachedResultDemandFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub stable_computed_identity: Vec<StableComputedIdentityFact>,
  /// Practice opportunities (one-way syncRef / conditional watch sources).
  #[serde(default, skip_serializing_if = "DerivationPracticeFacts::is_empty")]
  pub derivation_practice: DerivationPracticeFacts,
  /// Practice opportunities (queued flush / attached child scope / lazy async).
  #[serde(default, skip_serializing_if = "SchedulingPracticeFacts::is_empty")]
  pub scheduling_practice: SchedulingPracticeFacts,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub reactive_private_field_access: Vec<ReactivePrivateFieldAccessFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub until_timeout_unmatched_demand: Vec<UntilTimeoutUnmatchedDemandFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub inject_same_instance_provide: Vec<InjectSameInstanceDemandFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub ignorable_async_ignore_window: Vec<IgnorableAsyncIgnoreWindowFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub shared_composable_first_instance_args: Vec<SharedComposableFirstInstanceArgsFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub cancelled_filter_promise_demand: Vec<CancelledFilterPromiseDemandFact>,
}

impl SourceContractFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.trigger_ref_non_ref.is_empty()
      && self.torefs_non_proxy.is_empty()
      && self.primitive_reactive_target.is_empty()
      && self.watch_unwrapped_source.is_empty()
      && self.watch_replaced_object_source.is_empty()
      && self.watch_ignored_option.is_empty()
      && self.watch_signature_mismatch.is_empty()
      && self.watch_callback_contracts.is_empty()
      && self.toref_ignored_key.is_empty()
      && self.effect_scope_callback.is_empty()
      && self.uncloneable_proxy_data.is_empty()
      && self.invalid_custom_ref_interface.is_empty()
      && self.inactive_scope_result.is_empty()
      && self.missing_torefs_key.is_empty()
      && self.extracted_reactive_collection_method.is_empty()
      && self.raw_proxy_map_key.is_empty()
      && self.keyed_map_dependency.is_empty()
      && self.custom_ref_lost_notification.is_empty()
      && self.memoize_stale_result_demand.is_empty()
      && self.controlled_computed_stale_result_demand.is_empty()
      && self.stable_computed_identity.is_empty()
      && self.derivation_practice.is_empty()
      && self.scheduling_practice.is_empty()
      && self.reactive_private_field_access.is_empty()
      && self.until_timeout_unmatched_demand.is_empty()
      && self.inject_same_instance_provide.is_empty()
      && self.ignorable_async_ignore_window.is_empty()
      && self.shared_composable_first_instance_args.is_empty()
      && self.cancelled_filter_promise_demand.is_empty()
  }
}

/// Closed derivation-practice opportunities collected beside source contracts.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DerivationPracticeFacts {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub sync_ref_one_way: Vec<SyncRefOneWayFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub conditional_watch_source: Vec<ConditionalWatchSourceFact>,
}

impl DerivationPracticeFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.sync_ref_one_way.is_empty() && self.conditional_watch_source.is_empty()
  }
}

/// Proven default two-way `syncRef` whose right side is a closed sink.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SyncRefOneWayFact {
  pub call_span: SourceSpan,
  pub source_span: SourceSpan,
  pub sink_span: SourceSpan,
  pub demand_span: SourceSpan,
  pub sink_name: String,
  /// Root plus const aliases of the sink allocation, sorted.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub sink_names: Vec<String>,
}

/// Proven array-source watch that keeps an idle computed producer live.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConditionalWatchSourceFact {
  pub source_array_span: SourceSpan,
  pub guard_span: SourceSpan,
  pub producer_span: SourceSpan,
  pub idle_write_span: SourceSpan,
  pub producer_name: String,
  /// Root plus const aliases of the producer allocation, sorted.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub producer_names: Vec<String>,
}

/// Closed scheduling-practice opportunities collected beside source contracts.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SchedulingPracticeFacts {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub queued_watch_flush: Vec<QueuedWatchFlushFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub attached_effect_scope: Vec<AttachedEffectScopeFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub lazy_computed_async: Vec<LazyComputedAsyncFact>,
}

impl SchedulingPracticeFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.queued_watch_flush.is_empty()
      && self.attached_effect_scope.is_empty()
      && self.lazy_computed_async.is_empty()
  }
}

/// Proven `flush: 'sync'` watch whose sink is only observed after `nextTick`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QueuedWatchFlushFact {
  pub flush_span: SourceSpan,
  pub write_span: SourceSpan,
  pub demand_span: SourceSpan,
  pub sink_name: String,
}

/// Proven detached child scope that joins parent pause with valid stop ownership.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(clippy::struct_field_names, reason = "each field is a distinct causal span")]
pub struct AttachedEffectScopeFact {
  pub detached_span: SourceSpan,
  pub parent_span: SourceSpan,
  pub cleanup_span: SourceSpan,
  pub pause_span: SourceSpan,
  pub write_span: SourceSpan,
}

/// Proven eager `computedAsync` whose startup work is superseded before first demand.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(clippy::struct_field_names, reason = "each field is a distinct causal span")]
pub struct LazyComputedAsyncFact {
  pub call_span: SourceSpan,
  pub source_span: SourceSpan,
  pub demand_span: SourceSpan,
}

/// One call/argument site with a proven contract failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceContractSiteFact {
  pub span: SourceSpan,
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub api: String,
}

/// Proven `toRef(source, key)` overload that ignores a static key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToRefIgnoredKeyFact {
  pub span: SourceSpan,
  pub reason: ToRefIgnoredKeyReason,
}

/// Runtime `toRef` branch that discards the key argument.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToRefIgnoredKeyReason {
  Ref,
  Function,
  Primitive,
}

/// `watch(objectMember)` plus a later same-identity property replacement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchReplacedObjectSourceFact {
  pub source_span: SourceSpan,
  pub replacement_span: SourceSpan,
  pub object: String,
  pub property: String,
}

/// Why a watch-family options key is ignored by Vue 3.5.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchIgnoredOptionReason {
  Equals,
  Immediate,
  Deep,
  Once,
}

/// One ignored `watch` / `watch*Effect` options property.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchIgnoredOptionFact {
  pub span: SourceSpan,
  pub api: String,
  pub reason: WatchIgnoredOptionReason,
}

/// Why a watch-family argument is the wrong slot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchSignatureMismatchReason {
  WatchNonFunctionCallback,
  EffectFunctionAsOptions,
  EffectRefWithCallback,
}

/// One watch-family argument in the wrong slot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchSignatureMismatchFact {
  pub span: SourceSpan,
  pub api: String,
  pub reason: WatchSignatureMismatchReason,
}

/// Proven watch-callback contract failure (once/immediate discard or root identity).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchCallbackContractFact {
  pub watch_span: SourceSpan,
  pub source_span: SourceSpan,
  pub guard_span: SourceSpan,
  pub reason: WatchCallbackContractReason,
}

/// Why a watch callback is proven useless for later value-dependent work.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum WatchCallbackContractReason {
  OnceImmediateUndefinedGuard,
  ReactiveRootIdentityGuard,
}

/// Demanded `customRef` factory capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomRefCapability {
  Get,
  Set,
}

impl CustomRefCapability {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Get => "get",
      Self::Set => "set",
    }
  }
}

/// `customRef` factory missing/noncallable `get` or `set` plus a demanded `.value`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvalidCustomRefInterfaceFact {
  pub interface_span: SourceSpan,
  pub demand_span: SourceSpan,
  pub factory_span: SourceSpan,
  pub missing: CustomRefCapability,
}

/// `effectScope().stop()` then `scope.run` whose result is used as an object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(clippy::struct_field_names, reason = "each field is a distinct causal span")]
pub struct InactiveScopeResultFact {
  pub consumer_span: SourceSpan,
  pub stop_span: SourceSpan,
  pub run_span: SourceSpan,
}

/// `toRefs` of a closed reactive object, then a missing key is dereferenced.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MissingToRefsKeyFact {
  pub demand_span: SourceSpan,
  pub torefs_span: SourceSpan,
  pub key: String,
}

/// Bare call of a method extracted from a proven reactive Map/Set/array.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExtractedReactiveCollectionMethodFact {
  pub call_span: SourceSpan,
  pub extraction_span: SourceSpan,
  pub constructor_span: SourceSpan,
  pub object: String,
  pub method: String,
  pub collection: String,
  pub api: String,
}

/// Native `Map` stores one of a raw/proxy pair and later `get`s the other.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RawProxyMapKeyFact {
  pub demand_span: SourceSpan,
  pub get_span: SourceSpan,
  pub stored_key_span: SourceSpan,
  pub wrapper_span: SourceSpan,
  /// `"raw"` or `"proxy"` for the stored key identity.
  pub stored_kind: String,
  /// `"raw"` or `"proxy"` for the lookup identity.
  pub lookup_kind: String,
}

/// Reactive `Map.forEach` selecting one literal string key inside computed / `watchEffect`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyedMapDependencyFact {
  pub for_each_span: SourceSpan,
  pub map_span: SourceSpan,
  pub result_span: SourceSpan,
  pub key: String,
  /// `"computed"` or `"watchEffect"`.
  pub api: String,
}

/// Missing `track` in get versus missing `trigger` in set. One rule ID.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CustomRefLostNotificationReason {
  GetTracking,
  SetNotification,
}

/// Closed-local customRef whose primitive slot cannot notify a proven consumer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CustomRefLostNotificationFact {
  pub span: SourceSpan,
  pub source_span: SourceSpan,
  pub consumer_span: SourceSpan,
  pub write_span: SourceSpan,
  pub reason: CustomRefLostNotificationReason,
}

/// Proven primitive kind retained by a cache entry or later source write.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveValueKind {
  Number,
  String,
  Boolean,
  Bigint,
  Nullish,
}

impl PrimitiveValueKind {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Number => "number",
      Self::String => "string",
      Self::Boolean => "boolean",
      Self::Bigint => "bigint",
      Self::Nullish => "nullish",
    }
  }
}

/// Cached memoize / controlled-computed result fails a later native demand.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CachedResultDemandFact {
  pub demand_span: SourceSpan,
  pub fill_span: SourceSpan,
  pub write_span: SourceSpan,
  pub producer_span: SourceSpan,
  pub cached_kind: PrimitiveValueKind,
  pub current_kind: PrimitiveValueKind,
  pub member: String,
  pub api: String,
}

/// Proven computed identity churn with an established identity consumer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StableComputedIdentityFact {
  pub computed_span: SourceSpan,
  pub source_span: SourceSpan,
  pub replacement_span: SourceSpan,
  pub consumer_span: SourceSpan,
  pub reason: StableComputedIdentityReason,
}

/// Why a computed result is a measured identity-stability opportunity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum StableComputedIdentityReason {
  FreshPrimitiveProjection,
}

/// Native private-field access through a Vue reactive proxy receiver.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivePrivateFieldAccessFact {
  pub demand_span: SourceSpan,
  pub proxy_span: SourceSpan,
  pub member_span: SourceSpan,
  pub private_span: SourceSpan,
  pub api: String,
  pub member: String,
  pub field: String,
  pub getter: bool,
}

/// `until(ref).toBe(expected, { timeout })` fulfills the unmatched source value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UntilTimeoutUnmatchedDemandFact {
  pub demand_span: SourceSpan,
  pub comparison_span: SourceSpan,
  pub options_span: SourceSpan,
  pub source_span: SourceSpan,
  pub capability: String,
}

/// Why a same-instance provide/inject pair fails a later capability demand.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectSameInstanceDemandReason {
  LocalProvideMiss,
}

impl InjectSameInstanceDemandReason {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::LocalProvideMiss => "local_provide_miss",
    }
  }
}

/// Same-setup `provide` does not supply `inject`; the fallback fails a native demand.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InjectSameInstanceDemandFact {
  pub demand_span: SourceSpan,
  pub key_span: SourceSpan,
  pub provide_span: SourceSpan,
  pub inject_span: SourceSpan,
  pub fallback_kind: PrimitiveValueKind,
  pub provided_kind: PrimitiveValueKind,
  pub member: String,
  #[serde(default)]
  pub default_absent: bool,
  pub reason: InjectSameInstanceDemandReason,
}

/// Post-await source write after a proven synchronous `ignoreUpdates` window.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IgnorableAsyncIgnoreWindowFact {
  pub write_span: SourceSpan,
  pub ignore_span: SourceSpan,
  pub await_span: SourceSpan,
  pub api: String,
}

/// Later shared-state demand that the first initializer cannot satisfy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SharedComposableFirstInstanceArgsFact {
  pub demand_span: SourceSpan,
  pub first_call_span: SourceSpan,
  pub later_arg_span: SourceSpan,
  pub api: String,
  pub capability: String,
}

/// Default-debounce cancellation fulfills the earlier promise with `undefined`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CancelledFilterPromiseDemandFact {
  pub demand_span: SourceSpan,
  pub producer_span: SourceSpan,
  pub first_call_span: SourceSpan,
  pub superseding_call_span: SourceSpan,
  pub await_span: SourceSpan,
  pub member: String,
  pub api: String,
}
