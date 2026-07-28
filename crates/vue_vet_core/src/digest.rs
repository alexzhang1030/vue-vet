//! Stable content digests for IR cache keys and incremental invalidation.

use std::fmt::Write;

use serde::Serialize;
use sha2::{Digest, Sha256};

/// Hex-encoded SHA-256 digest of raw bytes.
#[must_use]
pub fn content_digest(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  hex_digest(&hasher.finalize())
}

/// Hex-encoded SHA-256 digest of a JSON-stable serde serialization.
///
/// Falls back to a tagged error digest when serialization fails so callers can
/// still compare keys without panicking on the analysis path.
#[must_use]
pub fn serde_digest<T: Serialize>(value: &T) -> String {
  match serde_json::to_vec(value) {
    Ok(bytes) => content_digest(&bytes),
    Err(error) => content_digest(format!("serde-digest-error:{error}").as_bytes()),
  }
}

fn hex_digest(bytes: &[u8]) -> String {
  let mut output = String::with_capacity(bytes.len().saturating_mul(2));
  for byte in bytes {
    if write!(&mut output, "{byte:02x}").is_err() {
      break;
    }
  }
  output
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, SourceSpan};

  #[test]
  fn content_digest_is_stable_for_identical_bytes() {
    assert_eq!(content_digest(b"hello"), content_digest(b"hello"));
    assert_ne!(content_digest(b"hello"), content_digest(b"world"));
  }

  #[test]
  fn serde_digest_changes_when_reactivity_graph_changes() {
    let empty = ReactivityGraph::default();
    let mut with_binding = ReactivityGraph::default();
    with_binding.bindings.push(ReactiveBindingFact {
      name: "count".into(),
      kind: ReactiveBindingKind::Ref,
      initialized_with_null: false,
      span: SourceSpan { offset: 0, length: 5, line: 1, column: 1 },
    });
    assert_eq!(serde_digest(&empty), serde_digest(&ReactivityGraph::default()));
    assert_ne!(serde_digest(&empty), serde_digest(&with_binding));
  }
}
