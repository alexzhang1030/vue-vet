//! Script-block access and setup-scoped call walks.

use vue_vet_core::{ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind};

/// True when `block` is `<script setup>` (compiler-macro / instance-binding surface).
#[must_use]
pub fn is_setup_block(block: &ScriptBlockFacts) -> bool {
  block.kind == ScriptKind::Setup
}

/// First script block of `kind`, if any. Matches the historical
/// `blocks.iter().find(|block| block.kind == kind)` lookup.
#[must_use]
pub fn script_block(script: &ScriptFacts, kind: ScriptKind) -> Option<&ScriptBlockFacts> {
  script.blocks.iter().find(|block| block.kind == kind)
}

/// `<script setup>` blocks in source order.
#[must_use]
pub fn setup_blocks(script: &ScriptFacts) -> impl Iterator<Item = &ScriptBlockFacts> {
  script.blocks.iter().filter(|block| is_setup_block(block))
}

/// Calls in `block` whose callee name equals `callee`.
#[must_use]
pub fn block_calls<'a>(
  block: &'a ScriptBlockFacts,
  callee: &'a str,
) -> impl Iterator<Item = &'a ScriptCallFact> {
  block.calls.iter().filter(move |call| call.callee == callee)
}

/// Whether any script block records a call named `callee`.
#[must_use]
pub fn script_has_call(script: &ScriptFacts, callee: &str) -> bool {
  script.blocks.iter().any(|block| block.calls.iter().any(|call| call.callee == callee))
}

/// SFC-absolute end offset of the first top-level `await` in `block`.
#[must_use]
pub fn first_top_level_await_end(block: &ScriptBlockFacts) -> Option<usize> {
  block.top_level_await_ends.first().copied()
}

/// Setup-block calls of `callee` whose start offset is at or after the first
/// top-level `await` end in that block.
///
/// Same predicate as the matrix after-await registrar pack and the
/// `define*` after-await extras: empty `top_level_await_ends` yields nothing;
/// a call at the await end itself is included (`offset >= first`).
#[must_use]
pub fn setup_calls_after_first_top_level_await<'a>(
  script: &'a ScriptFacts,
  callee: &'a str,
) -> impl Iterator<Item = &'a ScriptCallFact> {
  setup_blocks(script).flat_map(move |block| {
    let first = first_top_level_await_end(block);
    block_calls(block, callee).filter(move |call| first.is_some_and(|end| call.span.offset >= end))
  })
}

/// Second and later setup-block calls of `callee` (per block).
///
/// Used by the `define*` once-only rules; the first matching call in a setup
/// block is the allowed declaration.
#[must_use]
pub fn extra_setup_calls<'a>(
  script: &'a ScriptFacts,
  callee: &'a str,
) -> impl Iterator<Item = &'a ScriptCallFact> {
  setup_blocks(script).flat_map(move |block| block_calls(block, callee).skip(1))
}
