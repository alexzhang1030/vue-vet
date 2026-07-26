//! Interactive browser for per-module reactivity tracer facts.
//!
//! Pure browse-state helpers are unit-tested without a TTY. The ratatui loop
//! requires an interactive terminal and is wired from the CLI flag.

use std::{cmp::Reverse, io::IsTerminal};

use ratatui::{
  DefaultTerminal, Frame,
  crossterm::event::{self, Event, KeyCode, KeyEventKind},
  layout::{Constraint, Direction, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use vue_vet_reporters::ReactivityModuleStats;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowseModule {
  pub id: String,
  pub weight: usize,
  pub bindings: Vec<String>,
  pub scopes: Vec<String>,
  pub edges: Vec<String>,
  pub template_reads: Vec<String>,
}

impl BrowseModule {
  fn from_stats(module: &ReactivityModuleStats) -> Self {
    let weight = module
      .bindings
      .saturating_add(module.scopes)
      .saturating_add(module.edges)
      .saturating_add(module.template_reads);
    Self {
      id: module.id.clone(),
      weight,
      bindings: module.binding_labels.clone(),
      scopes: module.scope_labels.clone(),
      edges: module.edge_labels.clone(),
      template_reads: module.template_labels.clone(),
    }
  }
}

#[derive(Debug)]
struct BrowseApp {
  modules: Vec<BrowseModule>,
  visible: Vec<usize>,
  list_state: ListState,
  show_empty: bool,
  error: Option<String>,
  should_quit: bool,
}

impl BrowseApp {
  fn new(modules: Vec<BrowseModule>, error: Option<String>) -> Self {
    let mut app = Self {
      modules,
      visible: Vec::new(),
      list_state: ListState::default(),
      show_empty: false,
      error,
      should_quit: false,
    };
    app.rebuild_visible();
    app
  }

  fn rebuild_visible(&mut self) {
    let previous_id = self.selected_module().map(|module| module.id.clone());
    self.visible = self
      .modules
      .iter()
      .enumerate()
      .filter(|(_, module)| self.show_empty || module.weight > 0)
      .map(|(index, _)| index)
      .collect();
    let selected = previous_id
      .and_then(|id| {
        self
          .visible
          .iter()
          .position(|index| self.modules.get(*index).is_some_and(|module| module.id == id))
      })
      .or_else(|| (!self.visible.is_empty()).then_some(0));
    self.list_state.select(selected);
  }

  fn selected_module(&self) -> Option<&BrowseModule> {
    let visible_index = self.list_state.selected()?;
    let module_index = *self.visible.get(visible_index)?;
    self.modules.get(module_index)
  }

  fn move_selection(&mut self, forward: bool) {
    let len = self.visible.len();
    if len == 0 {
      self.list_state.select(None);
      return;
    }
    let current = self.list_state.selected().unwrap_or(0);
    let next = if forward {
      current.saturating_add(1) % len
    } else if current == 0 {
      len.saturating_sub(1)
    } else {
      current.saturating_sub(1)
    };
    self.list_state.select(Some(next));
  }

  fn toggle_empty(&mut self) {
    self.show_empty = !self.show_empty;
    self.rebuild_visible();
  }

  fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), String> {
    while !self.should_quit {
      terminal
        .draw(|frame| self.draw(frame))
        .map_err(|error| format!("reactivity TUI draw failed: {error}"))?;
      self.handle_events()?;
    }
    Ok(())
  }

  fn handle_events(&mut self) -> Result<(), String> {
    let Event::Key(key) =
      event::read().map_err(|error| format!("reactivity TUI input failed: {error}"))?
    else {
      return Ok(());
    };
    if key.kind != KeyEventKind::Press {
      return Ok(());
    }
    match key.code {
      KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
      KeyCode::Down | KeyCode::Char('j') => self.move_selection(true),
      KeyCode::Up | KeyCode::Char('k') => self.move_selection(false),
      KeyCode::Char('e') => self.toggle_empty(),
      KeyCode::Home => self.list_state.select((!self.visible.is_empty()).then_some(0)),
      KeyCode::End => {
        if let Some(last) = self.visible.len().checked_sub(1) {
          self.list_state.select(Some(last));
        }
      }
      _ => {}
    }
    Ok(())
  }

  fn draw(&mut self, frame: &mut Frame<'_>) {
    let [header, body, footer] = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(2)])
      .areas(frame.area());
    self.draw_header(frame, header);
    self.draw_body(frame, body);
    Self::draw_footer(frame, footer);
  }

  fn draw_header(&self, frame: &mut Frame<'_>, area: Rect) {
    let bindings = self.modules.iter().map(|module| module.bindings.len()).sum::<usize>();
    let scopes = self.modules.iter().map(|module| module.scopes.len()).sum::<usize>();
    let edges = self.modules.iter().map(|module| module.edges.len()).sum::<usize>();
    let template_reads =
      self.modules.iter().map(|module| module.template_reads.len()).sum::<usize>();
    let title = self.error.as_ref().map_or_else(
      || {
        format!(
          "Reactivity TUI — {} module(s) · {}b · {}s · {}e · {}t · showing {}{}",
          self.modules.len(),
          bindings,
          scopes,
          edges,
          template_reads,
          self.visible.len(),
          if self.show_empty { " (including empty)" } else { " (non-empty)" }
        )
      },
      |error| format!("Reactivity TUI — unavailable: {error}"),
    );
    frame.render_widget(
      Paragraph::new(title).block(Block::default().borders(Borders::ALL).title("vue-vet")),
      area,
    );
  }

  fn draw_body(&mut self, frame: &mut Frame<'_>, area: Rect) {
    let [list_area, detail_area] = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
      .areas(area);
    let items = self
      .visible
      .iter()
      .filter_map(|index| self.modules.get(*index))
      .map(|module| {
        ListItem::new(format!(
          "{:>4}  {}  ({}b {}s {}e {}t)",
          module.weight,
          module.id,
          module.bindings.len(),
          module.scopes.len(),
          module.edges.len(),
          module.template_reads.len()
        ))
      })
      .collect::<Vec<_>>();
    let list = List::new(items)
      .block(Block::default().borders(Borders::ALL).title("modules (busiest first)"))
      .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
      .highlight_symbol("> ");
    frame.render_stateful_widget(list, list_area, &mut self.list_state);

    let detail = self.selected_module().map_or_else(
      || Paragraph::new("No modules to show. Press e to include empty modules."),
      |module| {
        let mut lines = vec![Line::from(Span::styled(
          module.id.clone(),
          Style::default().add_modifier(Modifier::BOLD),
        ))];
        push_section(&mut lines, "bindings", &module.bindings);
        push_section(&mut lines, "scopes", &module.scopes);
        push_section(&mut lines, "edges", &module.edges);
        push_section(&mut lines, "template", &module.template_reads);
        Paragraph::new(lines).wrap(Wrap { trim: false })
      },
    );
    frame.render_widget(
      detail.block(Block::default().borders(Borders::ALL).title("detail")),
      detail_area,
    );
  }

  fn draw_footer(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
      Paragraph::new("↑/↓ or j/k move · e toggle empty · Home/End · q/Esc quit"),
      area,
    );
  }
}

fn push_section(lines: &mut Vec<Line<'static>>, label: &str, values: &[String]) {
  lines.push(Line::from(format!("{label}:")));
  if values.is_empty() {
    lines.push(Line::from("  (none)"));
    return;
  }
  for value in values {
    lines.push(Line::from(format!("  {value}")));
  }
}

/// Rank modules busiest-first for the TUI list.
#[must_use]
pub fn ranked_modules(stats: &[ReactivityModuleStats]) -> Vec<BrowseModule> {
  let mut modules = stats.iter().map(BrowseModule::from_stats).collect::<Vec<_>>();
  modules.sort_by(|left, right| {
    (Reverse(left.weight), left.id.as_str()).cmp(&(Reverse(right.weight), right.id.as_str()))
  });
  modules
}

/// Open the reactivity browser. Requires an interactive stdin/stdout TTY.
pub fn run_reactivity_tui(
  stats: &[ReactivityModuleStats],
  error: Option<String>,
) -> Result<(), String> {
  if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
    return Err(
      "--reactivity-tui requires an interactive terminal; use --print-reactivity for text detail"
        .into(),
    );
  }
  let modules = ranked_modules(stats);
  let mut app = BrowseApp::new(modules, error);
  let mut terminal =
    ratatui::try_init().map_err(|error| format!("failed to initialize reactivity TUI: {error}"))?;
  let run_result = app.run(&mut terminal);
  if let Err(error) = ratatui::try_restore() {
    return Err(format!("failed to restore terminal after reactivity TUI: {error}"));
  }
  run_result
}

#[cfg(test)]
mod tests {
  use super::*;

  fn stats(
    id: &str,
    bindings: usize,
    scopes: usize,
    edges: usize,
    reads: usize,
  ) -> ReactivityModuleStats {
    ReactivityModuleStats {
      id: id.into(),
      bindings,
      scopes,
      edges,
      template_reads: reads,
      binding_labels: (0..bindings).map(|index| format!("b{index}")).collect(),
      scope_labels: (0..scopes).map(|index| format!("s{index}")).collect(),
      edge_labels: (0..edges).map(|index| format!("e{index}")).collect(),
      template_labels: (0..reads).map(|index| format!("t{index}")).collect(),
    }
  }

  #[test]
  fn ranks_busiest_modules_first_and_keeps_stable_ties() {
    let ranked = ranked_modules(&[
      stats("b.vue", 1, 0, 0, 0),
      stats("a.vue", 2, 2, 0, 0),
      stats("c.vue", 2, 2, 0, 0),
      stats("empty.ts", 0, 0, 0, 0),
    ]);
    let ids = ranked.iter().map(|module| module.id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids, ["a.vue", "c.vue", "b.vue", "empty.ts"]);
  }

  #[test]
  fn hides_empty_modules_until_toggled() {
    let ranked = ranked_modules(&[stats("hot.vue", 2, 0, 0, 0), stats("empty.ts", 0, 0, 0, 0)]);
    let mut app = BrowseApp::new(ranked, None);
    assert_eq!(app.visible.len(), 1);
    assert_eq!(app.selected_module().map(|module| module.id.as_str()), Some("hot.vue"));
    app.toggle_empty();
    assert_eq!(app.visible.len(), 2);
  }

  #[test]
  fn selection_wraps_at_list_edges() {
    let ranked = ranked_modules(&[
      stats("a.vue", 3, 0, 0, 0),
      stats("b.vue", 2, 0, 0, 0),
      stats("c.vue", 1, 0, 0, 0),
    ]);
    let mut app = BrowseApp::new(ranked, None);
    assert_eq!(app.list_state.selected(), Some(0));
    app.move_selection(false);
    assert_eq!(app.list_state.selected(), Some(2));
    app.move_selection(true);
    assert_eq!(app.list_state.selected(), Some(0));
  }
}
