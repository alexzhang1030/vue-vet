//! Actual Vue Proxy proof for native `structuredClone` (issue #224).
//!
//! `Shape::DeepProxy` / `ShallowProxy` / `ReadonlyProxy` are API-kind labels
//! only. This module requires a fresh object/array literal passed to a
//! symbol-resolved Vue 3 proxy constructor, or a const alias of that result.
//! Ordinary later mutations of that binding keep Proxy identity. Raw-target
//! bindings (`reactive(existing)`) stay unproved without write/mark history.
//! Named `#imports` and `vue-demi` constructors stay unproved here.

use oxc_semantic::SymbolFlags;
use oxc_span::Span;

use super::index::{CallInfo, ObjectEntry};
use super::shape::span_key;
use super::{Collector, MAX_DEPTH};
use vue_vet_core::SourceContractSiteFact;

const PROXY_APIS: &[&str] = &["reactive", "readonly", "shallowReactive", "shallowReadonly"];

const BLOCKED_KEYS: &[&str] = &["__v_skip", "__v_isReadonly", "__v_isRef", "__v_raw", "__proto__"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProxyProof {
  Yes,
  No,
  Exhausted,
}

impl Collector<'_> {
  pub(super) fn collect_structured_clone(&mut self, info: CallInfo) {
    if self.indexes.clone_intrinsic_poisoned {
      return;
    }
    if info.has_spread || info.arg_count != 1 {
      return;
    }
    let Some(argument) = info.first_arg else {
      return;
    };
    if self.is_actual_proxy(argument, MAX_DEPTH) != ProxyProof::Yes {
      return;
    }
    self
      .facts
      .uncloneable_proxy_data
      .push(SourceContractSiteFact { span: self.span(argument), api: "structuredClone".into() });
  }

  fn is_actual_proxy(&mut self, span: Span, remaining: u8) -> ProxyProof {
    self.indexes.note_query();
    if remaining == 0 {
      return ProxyProof::Exhausted;
    }
    if let Some(info) = self.indexes.calls.get(&span_key(span)).copied() {
      return self.call_allocates_proxy(info, remaining);
    }
    if let Some(hint) = self.indexes.hints.get(&span_key(span)).copied() {
      match hint {
        super::shape::ShapeHint::Call(call_span) => {
          if let Some(info) = self.indexes.calls.get(&span_key(call_span)).copied() {
            return self.call_allocates_proxy(info, remaining);
          }
        }
        super::shape::ShapeHint::Identifier(Some(symbol_id), false) => {
          return self.symbol_holds_proxy(symbol_id, remaining);
        }
        _ => {}
      }
    }
    ProxyProof::No
  }

  fn call_allocates_proxy(&mut self, info: CallInfo, remaining: u8) -> ProxyProof {
    if !info.actual_proxy_origin {
      return ProxyProof::No;
    }
    let Some(api) = info.api else {
      return ProxyProof::No;
    };
    if !PROXY_APIS.contains(&api) || info.has_spread {
      return ProxyProof::No;
    }
    let Some(target) = info.first_arg else {
      return ProxyProof::No;
    };
    if self.is_fresh_supported_root(target) {
      return ProxyProof::Yes;
    }
    self.is_actual_proxy(target, remaining.saturating_sub(1))
  }

  fn symbol_holds_proxy(&mut self, symbol_id: oxc_semantic::SymbolId, remaining: u8) -> ProxyProof {
    let root = self.indexes.root_of(symbol_id);
    if let Some(cached) = self.proxy_proof.get(&root) {
      return if *cached { ProxyProof::Yes } else { ProxyProof::No };
    }
    if remaining == 0 {
      return ProxyProof::Exhausted;
    }
    if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
      self.proxy_proof.insert(root, false);
      return ProxyProof::No;
    }
    if self.indexes.reassigned.contains(&root) {
      self.proxy_proof.insert(root, false);
      return ProxyProof::No;
    }
    let Some(init_span) = self.indexes.init_span.get(&root).copied() else {
      self.proxy_proof.insert(root, false);
      return ProxyProof::No;
    };
    match self.is_actual_proxy(init_span, remaining.saturating_sub(1)) {
      ProxyProof::Yes => {
        self.proxy_proof.insert(root, true);
        ProxyProof::Yes
      }
      ProxyProof::No => {
        self.proxy_proof.insert(root, false);
        ProxyProof::No
      }
      ProxyProof::Exhausted => ProxyProof::Exhausted,
    }
  }

  fn is_fresh_supported_root(&self, span: Span) -> bool {
    self.indexes.note_query();
    let Some(literal) = self
      .indexes
      .literal_span
      .get(&span_key(span))
      .copied()
      .or_else(|| self.indexes.objects.contains_key(&span_key(span)).then_some(span))
    else {
      return false;
    };
    if let Some(entries) = self.indexes.objects.get(&span_key(literal)) {
      return eligible_object_entries(entries, |n| self.indexes.note_object_entries(n));
    }
    if self.indexes.array_spread.contains(&span_key(literal)) {
      return false;
    }
    self
      .indexes
      .hints
      .get(&span_key(literal))
      .is_some_and(|hint| matches!(hint, super::shape::ShapeHint::PlainRecord))
      && !self.indexes.objects.contains_key(&span_key(literal))
  }
}

fn eligible_object_entries(entries: &[ObjectEntry], mut note: impl FnMut(u64)) -> bool {
  if entries.is_empty() {
    return true;
  }
  entries.iter().all(|entry| {
    note(1);
    match entry {
      ObjectEntry::Data { name, .. } => !BLOCKED_KEYS.contains(&name.as_str()),
      ObjectEntry::Spread
      | ObjectEntry::Computed
      | ObjectEntry::Accessor { .. }
      | ObjectEntry::Method { .. } => false,
    }
  })
}
