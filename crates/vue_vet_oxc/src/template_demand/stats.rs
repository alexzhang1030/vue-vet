//! Adapter-only work counters for template-ref demand collection.
//!
//! Production `WorkCounter` is zero-sized and does not record. Test builds
//! keep saturating `Cell` counters so inner-work growth tests stay real.

#[cfg(test)]
use std::cell::Cell;

/// Completed collector work. Not part of the stable Vue Vet fact contract.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TemplateDemandStats {
  pub nodes: u64,
  pub owners: u64,
  pub references: u64,
  pub template_nodes: u64,
  pub expression_ast: u64,
  pub links: u64,
  pub sorts: u64,
  pub comparisons: u64,
  pub queries: u64,
}

impl TemplateDemandStats {
  #[cfg(test)]
  #[must_use]
  pub const fn work(self) -> u64 {
    self
      .nodes
      .saturating_add(self.owners)
      .saturating_add(self.references)
      .saturating_add(self.template_nodes)
      .saturating_add(self.expression_ast)
      .saturating_add(self.links)
      .saturating_add(self.sorts)
      .saturating_add(self.comparisons)
      .saturating_add(self.queries)
  }

  #[cfg(test)]
  #[must_use]
  pub const fn is_import_preflight_only(self) -> bool {
    self.owners == 0 && self.template_nodes == 0 && self.links == 0 && self.queries == 0
  }
}

#[derive(Default)]
pub(super) struct WorkCounter {
  #[cfg(test)]
  nodes: Cell<u64>,
  #[cfg(test)]
  owners: Cell<u64>,
  #[cfg(test)]
  references: Cell<u64>,
  #[cfg(test)]
  template_nodes: Cell<u64>,
  #[cfg(test)]
  expression_ast: Cell<u64>,
  #[cfg(test)]
  links: Cell<u64>,
  #[cfg(test)]
  sorts: Cell<u64>,
  #[cfg(test)]
  comparisons: Cell<u64>,
  #[cfg(test)]
  queries: Cell<u64>,
}

macro_rules! counter_pair {
  ($add:ident, $field:ident) => {
    impl WorkCounter {
      #[cfg(test)]
      pub(super) fn $add(&self, n: u64) {
        self.$field.set(self.$field.get().saturating_add(n));
      }

      #[cfg(not(test))]
      #[expect(
        clippy::missing_const_for_fn,
        reason = "zero-sized production counter keeps the test method shape"
      )]
      pub(super) fn $add(&self, n: u64) {
        let _ = (self, n);
      }
    }
  };
}

counter_pair!(add_nodes, nodes);
counter_pair!(add_owners, owners);
counter_pair!(add_references, references);
counter_pair!(add_template_nodes, template_nodes);
counter_pair!(add_expression_ast, expression_ast);
counter_pair!(add_links, links);
counter_pair!(add_sorts, sorts);
counter_pair!(add_comparisons, comparisons);
counter_pair!(add_queries, queries);

impl WorkCounter {
  #[cfg(test)]
  pub(super) const fn snapshot(&self) -> TemplateDemandStats {
    TemplateDemandStats {
      nodes: self.nodes.get(),
      owners: self.owners.get(),
      references: self.references.get(),
      template_nodes: self.template_nodes.get(),
      expression_ast: self.expression_ast.get(),
      links: self.links.get(),
      sorts: self.sorts.get(),
      comparisons: self.comparisons.get(),
      queries: self.queries.get(),
    }
  }

  #[cfg(not(test))]
  #[expect(clippy::unused_self, reason = "production snapshot is always zero")]
  pub(super) const fn snapshot(&self) -> TemplateDemandStats {
    TemplateDemandStats {
      nodes: 0,
      owners: 0,
      references: 0,
      template_nodes: 0,
      expression_ast: 0,
      links: 0,
      sorts: 0,
      comparisons: 0,
      queries: 0,
    }
  }
}
