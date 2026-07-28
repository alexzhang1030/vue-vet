//! Line starts and UTF-16 ↔ byte conversions for editor protocols.

/// Precomputed line starts for one source buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineIndex {
  line_starts: Vec<usize>,
}

impl LineIndex {
  /// Build an index over `source` (UTF-8).
  #[must_use]
  pub fn new(source: &str) -> Self {
    let mut line_starts = vec![0];
    for (offset, byte) in source.as_bytes().iter().enumerate() {
      if *byte == b'\n' {
        line_starts.push(offset.saturating_add(1));
      }
    }
    Self { line_starts }
  }

  /// 1-based line and UTF-8 byte column for a byte offset (Vue Vet span convention).
  #[must_use]
  pub fn byte_to_line_column(&self, offset: usize) -> (usize, usize) {
    let line_idx = self.line_index_for_offset(offset);
    let start = self.line_starts.get(line_idx).copied().unwrap_or(0);
    (line_idx.saturating_add(1), offset.saturating_sub(start).saturating_add(1))
  }

  /// 0-based LSP position using UTF-16 code units for the character offset.
  #[must_use]
  pub fn byte_to_utf16(&self, source: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(source.len());
    let line_idx = self.line_index_for_offset(offset);
    let start = self.line_starts.get(line_idx).copied().unwrap_or(0);
    let line_bytes = source.get(start..offset).unwrap_or("");
    let character = utf16_len(line_bytes);
    (u32_from_usize(line_idx), u32_from_usize(character))
  }

  /// Convert a 0-based UTF-16 LSP position into a UTF-8 byte offset.
  #[must_use]
  pub fn utf16_to_byte(&self, source: &str, line: u32, character: u32) -> Option<usize> {
    let line_idx = usize::try_from(line).ok()?;
    let start = *self.line_starts.get(line_idx)?;
    let end = self.line_starts.get(line_idx.saturating_add(1)).copied().unwrap_or(source.len());
    let line_text = source.get(start..end)?;
    let mut units = 0_usize;
    let target = usize::try_from(character).ok()?;
    for (byte_offset, ch) in line_text.char_indices() {
      if units == target {
        return Some(start.saturating_add(byte_offset));
      }
      let width = ch.len_utf16();
      if units.saturating_add(width) > target {
        return Some(start.saturating_add(byte_offset));
      }
      units = units.saturating_add(width);
    }
    (units == target).then_some(end)
  }

  fn line_index_for_offset(&self, offset: usize) -> usize {
    match self.line_starts.binary_search(&offset) {
      Ok(index) => index,
      Err(index) => index.saturating_sub(1),
    }
  }
}

fn utf16_len(text: &str) -> usize {
  text.chars().map(char::len_utf16).sum()
}

fn u32_from_usize(value: usize) -> u32 {
  u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[expect(clippy::panic, reason = "line-index fixture assertions must fail the unit test")]
  fn utf16_positions_count_multibyte_prefixes() {
    let source = "<template>\n  <div>中文😀</div>\n  <main v-html=\"html\" />\n</template>\n";
    let index = LineIndex::new(source);
    let Some(offset) = source.find("v-html") else {
      panic!("fixture must contain v-html");
    };
    let (line, character) = index.byte_to_utf16(source, offset);
    assert_eq!(line, 2);
    // "  <main " is 8 ASCII UTF-16 units.
    assert_eq!(character, 8);
    assert_eq!(index.utf16_to_byte(source, line, character), Some(offset));
  }

  #[test]
  #[expect(clippy::panic, reason = "line-index fixture assertions must fail the unit test")]
  fn emoji_uses_two_utf16_units() {
    let source = "中文😀x";
    let index = LineIndex::new(source);
    let Some(offset) = source.find('x') else {
      panic!("fixture must contain x");
    };
    let (line, character) = index.byte_to_utf16(source, offset);
    assert_eq!(line, 0);
    // 中文 = 2 units, 😀 = 2 units
    assert_eq!(character, 4);
    assert_eq!(index.utf16_to_byte(source, 0, 4), Some(offset));
  }
}
