use std::{
  error::Error,
  fmt,
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EditApplicability {
  Safe,
  Unsafe,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ByteRange {
  pub offset: usize,
  pub length: usize,
}

impl ByteRange {
  #[must_use]
  pub const fn end(self) -> Option<usize> {
    self.offset.checked_add(self.length)
  }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TextEdit {
  pub file: PathBuf,
  pub range: ByteRange,
  pub replacement: String,
  pub applicability: EditApplicability,
  pub rule_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EditPlan {
  edits: Vec<TextEdit>,
}

impl EditPlan {
  /// Validates byte ranges, rejects conflicting edits, and returns a deterministic plan.
  ///
  /// # Errors
  ///
  /// Returns [`EditPlanError::RangeOverflow`] when an edit's range cannot be
  /// represented, or [`EditPlanError::Conflict`] when two edits target
  /// overlapping or order-dependent ranges in the same file.
  pub fn new(mut edits: Vec<TextEdit>) -> Result<Self, EditPlanError> {
    for edit in &edits {
      if edit.range.end().is_none() {
        return Err(EditPlanError::RangeOverflow { edit: Box::new(edit.clone()) });
      }
    }

    edits.sort_by(|left, right| {
      (
        normalized_path(&left.file),
        left.range,
        &left.rule_id,
        left.applicability,
        &left.replacement,
      )
        .cmp(&(
          normalized_path(&right.file),
          right.range,
          &right.rule_id,
          right.applicability,
          &right.replacement,
        ))
    });

    for pair in edits.windows(2) {
      let [first, second] = pair else {
        continue;
      };
      if normalized_path(&first.file) == normalized_path(&second.file)
        && ranges_conflict(first.range, second.range)
      {
        return Err(EditPlanError::Conflict {
          first: Box::new(first.clone()),
          second: Box::new(second.clone()),
        });
      }
    }

    Ok(Self { edits })
  }

  #[must_use]
  pub fn edits(&self) -> &[TextEdit] {
    &self.edits
  }

  #[must_use]
  pub fn into_edits(self) -> Vec<TextEdit> {
    self.edits
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditPlanError {
  RangeOverflow { edit: Box<TextEdit> },
  Conflict { first: Box<TextEdit>, second: Box<TextEdit> },
}

impl fmt::Display for EditPlanError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::RangeOverflow { edit } => write!(
        formatter,
        "edit range overflows for {} at byte {} with length {}",
        edit.file.display(),
        edit.range.offset,
        edit.range.length
      ),
      Self::Conflict { first, second } => write!(
        formatter,
        "conflicting edits for {} from `{}` and `{}`",
        first.file.display(),
        first.rule_id,
        second.rule_id
      ),
    }
  }
}

impl Error for EditPlanError {}

fn normalized_path(path: &Path) -> String {
  path.to_string_lossy().replace('\\', "/")
}

const fn ranges_conflict(left: ByteRange, right: ByteRange) -> bool {
  let left_end = left.offset.saturating_add(left.length);
  let right_end = right.offset.saturating_add(right.length);

  if left.length == 0 {
    return left.offset >= right.offset && left.offset <= right_end;
  }
  if right.length == 0 {
    return right.offset >= left.offset && right.offset <= left_end;
  }
  left.offset < right_end && right.offset < left_end
}

#[cfg(test)]
mod tests {
  use super::*;

  fn edit(file: &str, offset: usize, length: usize, replacement: &str, rule_id: &str) -> TextEdit {
    TextEdit {
      file: file.into(),
      range: ByteRange { offset, length },
      replacement: replacement.into(),
      applicability: EditApplicability::Safe,
      rule_id: rule_id.into(),
    }
  }

  #[test]
  fn orders_valid_edits_deterministically() {
    let plan = EditPlan::new(vec![
      edit("src/z.vue", 8, 2, "z", "vue-vet/test/z"),
      edit("src/a.vue", 4, 2, "a", "vue-vet/test/a"),
      edit("src/a.vue", 8, 2, "b", "vue-vet/test/b"),
    ]);
    let rules = plan
      .as_ref()
      .ok()
      .into_iter()
      .flat_map(EditPlan::edits)
      .map(|edit| edit.rule_id.as_str())
      .collect::<Vec<_>>();
    assert_eq!(rules, ["vue-vet/test/a", "vue-vet/test/b", "vue-vet/test/z"]);
  }

  #[test]
  fn rejects_overlapping_replacements() {
    let first = edit("src/App.vue", 4, 6, "first", "vue-vet/test/first");
    let second = edit("src/App.vue", 8, 4, "second", "vue-vet/test/second");
    let plan = EditPlan::new(vec![second.clone(), first.clone()]);

    assert_eq!(
      plan,
      Err(EditPlanError::Conflict { first: Box::new(first), second: Box::new(second) })
    );
  }

  #[test]
  fn rejects_order_dependent_insertions_at_boundaries() {
    let replacement = edit("src/App.vue", 4, 4, "value", "vue-vet/test/replace");
    let insertion = edit("src/App.vue", 8, 0, ";", "vue-vet/test/insert");
    let plan = EditPlan::new(vec![insertion.clone(), replacement.clone()]);

    assert_eq!(
      plan,
      Err(EditPlanError::Conflict { first: Box::new(replacement), second: Box::new(insertion) })
    );
  }

  #[test]
  fn rejects_multiple_insertions_at_the_same_offset() {
    let first = edit("src/App.vue", 8, 0, "first", "vue-vet/test/first");
    let second = edit("src/App.vue", 8, 0, "second", "vue-vet/test/second");
    let plan = EditPlan::new(vec![second.clone(), first.clone()]);

    assert_eq!(
      plan,
      Err(EditPlanError::Conflict { first: Box::new(first), second: Box::new(second) })
    );
  }

  #[test]
  fn accepts_adjacent_replacements() {
    let first = edit("src/App.vue", 4, 4, "first", "vue-vet/test/first");
    let second = edit("src/App.vue", 8, 4, "second", "vue-vet/test/second");

    let plan = EditPlan::new(vec![first, second]);
    assert_eq!(plan.as_ref().ok().map(|plan| plan.edits().len()), Some(2));
  }

  #[test]
  fn accepts_equal_ranges_in_different_files() {
    let first = edit("src/App.vue", 4, 4, "first", "vue-vet/test/first");
    let second = edit("src/Other.vue", 4, 4, "second", "vue-vet/test/second");
    let plan = EditPlan::new(vec![first, second]);

    assert_eq!(plan.as_ref().ok().map(|plan| plan.edits().len()), Some(2));
  }

  #[test]
  fn treats_path_separator_variants_as_one_logical_file() {
    let first = edit("src/App.vue", 4, 4, "first", "vue-vet/test/first");
    let second = edit("src\\App.vue", 4, 4, "second", "vue-vet/test/second");
    let plan = EditPlan::new(vec![second.clone(), first.clone()]);

    assert_eq!(
      plan,
      Err(EditPlanError::Conflict { first: Box::new(first), second: Box::new(second) })
    );
  }

  #[test]
  fn rejects_ranges_that_overflow_usize() {
    let invalid = edit("src/App.vue", usize::MAX, 1, "value", "vue-vet/test/overflow");
    let plan = EditPlan::new(vec![invalid.clone()]);

    assert_eq!(plan, Err(EditPlanError::RangeOverflow { edit: Box::new(invalid) }));
  }

  #[test]
  fn serializes_machine_readable_applicability_and_ranges() {
    let source = TextEdit {
      applicability: EditApplicability::Unsafe,
      ..edit("src/App.vue", 4, 2, "value", "vue-vet/test/edit")
    };
    let serialized = serde_json::to_value(source);
    let value = serialized.as_ref().ok();

    assert_eq!(
      value.and_then(|value| value.get("applicability")).and_then(serde_json::Value::as_str),
      Some("unsafe")
    );
    assert_eq!(
      value
        .and_then(|value| value.get("range"))
        .and_then(|range| range.get("offset"))
        .and_then(serde_json::Value::as_u64),
      Some(4)
    );
    assert_eq!(
      value
        .and_then(|value| value.get("range"))
        .and_then(|range| range.get("length"))
        .and_then(serde_json::Value::as_u64),
      Some(2)
    );
  }
}
