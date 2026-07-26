//! Interactive browser for per-module reactivity tracer facts.
//!
//! Pure browse-state helpers are unit-tested without a TTY. The ratatui loop
//! requires an interactive terminal and is wired from the CLI flag.

use std::{cmp::Reverse, collections::BTreeMap, io::IsTerminal};

use ratatui::{
  DefaultTerminal, Frame,
  crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind},
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use vue_vet_reporters::{
  ReactivityModuleStats, humanize_binding, humanize_edge, humanize_scope, humanize_source,
  humanize_template_read, humanize_template_surface,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
  Modules,
  Panel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PanelMode {
  Detail,
  Graph,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseHit {
  Modules { visible_index: usize },
  Panel,
  Help,
  Outside,
}

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
  focus: Focus,
  panel_mode: PanelMode,
  panel_scroll: u16,
  show_help: bool,
  error: Option<String>,
  should_quit: bool,
  modules_area: Rect,
  panel_area: Rect,
}

impl BrowseApp {
  fn new(modules: Vec<BrowseModule>, error: Option<String>) -> Self {
    let mut app = Self {
      modules,
      visible: Vec::new(),
      list_state: ListState::default(),
      show_empty: false,
      focus: Focus::Modules,
      panel_mode: PanelMode::Detail,
      panel_scroll: 0,
      show_help: false,
      error,
      should_quit: false,
      modules_area: Rect::default(),
      panel_area: Rect::default(),
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
    self.panel_scroll = 0;
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
    self.panel_scroll = 0;
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
    match event::read().map_err(|error| format!("reactivity TUI input failed: {error}"))? {
      Event::Key(key) => {
        if key.kind != KeyEventKind::Press {
          return Ok(());
        }
        self.handle_key(key.code);
      }
      Event::Mouse(mouse) => match mouse.kind {
        MouseEventKind::ScrollDown => self.on_scroll(1),
        MouseEventKind::ScrollUp => self.on_scroll(-1),
        MouseEventKind::Down(MouseButton::Left) => self.on_click(mouse.column, mouse.row),
        _ => {}
      },
      _ => {}
    }
    Ok(())
  }

  fn on_click(&mut self, column: u16, row: u16) {
    let list_offset = self.list_state.offset();
    match hit_test(
      column,
      row,
      self.modules_area,
      self.panel_area,
      self.show_help,
      list_offset,
      self.visible.len(),
    ) {
      MouseHit::Help => self.show_help = false,
      MouseHit::Modules { visible_index } => {
        self.focus = Focus::Modules;
        self.list_state.select(Some(visible_index));
        self.panel_scroll = 0;
      }
      MouseHit::Panel => self.focus = Focus::Panel,
      MouseHit::Outside => {}
    }
  }

  fn on_scroll(&mut self, delta: i32) {
    match self.focus {
      Focus::Modules => {
        if delta > 0 {
          self.move_selection(true);
        } else {
          self.move_selection(false);
        }
      }
      Focus::Panel => {
        // Viewport height is refreshed on draw; approximate a page with 3 lines.
        self.panel_scroll = if delta > 0 {
          self.panel_scroll.saturating_add(3)
        } else {
          self.panel_scroll.saturating_sub(3)
        };
      }
    }
  }

  fn handle_key(&mut self, code: KeyCode) {
    if self.show_help {
      match code {
        KeyCode::Char('?' | 'q') | KeyCode::Esc => self.show_help = false,
        _ => {}
      }
      return;
    }
    match code {
      KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
      KeyCode::Char('?') => self.show_help = true,
      KeyCode::Tab | KeyCode::BackTab => {
        self.focus = match self.focus {
          Focus::Modules => Focus::Panel,
          Focus::Panel => Focus::Modules,
        };
      }
      KeyCode::Char('g') => {
        self.panel_mode = match self.panel_mode {
          PanelMode::Detail => PanelMode::Graph,
          PanelMode::Graph => PanelMode::Detail,
        };
        self.panel_scroll = 0;
        self.focus = Focus::Panel;
      }
      KeyCode::Char('e') => self.toggle_empty(),
      KeyCode::Home if self.focus == Focus::Modules => {
        self.list_state.select((!self.visible.is_empty()).then_some(0));
        self.panel_scroll = 0;
      }
      KeyCode::End if self.focus == Focus::Modules => {
        if let Some(last) = self.visible.len().checked_sub(1) {
          self.list_state.select(Some(last));
          self.panel_scroll = 0;
        }
      }
      KeyCode::Down | KeyCode::Char('j') => match self.focus {
        Focus::Modules => self.move_selection(true),
        Focus::Panel => self.panel_scroll = self.panel_scroll.saturating_add(1),
      },
      KeyCode::Up | KeyCode::Char('k') => match self.focus {
        Focus::Modules => self.move_selection(false),
        Focus::Panel => self.panel_scroll = self.panel_scroll.saturating_sub(1),
      },
      KeyCode::PageDown | KeyCode::Char('d') if self.focus == Focus::Panel => {
        self.panel_scroll = self.panel_scroll.saturating_add(10);
      }
      KeyCode::PageUp | KeyCode::Char('u') if self.focus == Focus::Panel => {
        self.panel_scroll = self.panel_scroll.saturating_sub(10);
      }
      KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::Panel,
      KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Modules,
      _ => {}
    }
  }

  fn draw(&mut self, frame: &mut Frame<'_>) {
    let [header, body, footer] = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(2)])
      .areas(frame.area());
    self.draw_header(frame, header);
    let panel_viewport = self.draw_body(frame, body);
    self.clamp_panel_scroll(panel_viewport);
    Self::draw_footer(frame, footer, self.focus, self.panel_mode);
    if self.show_help {
      draw_help(frame);
    }
  }

  fn clamp_panel_scroll(&mut self, viewport_rows: u16) {
    let content_rows = self.panel_lines().len();
    let max_scroll = content_rows.saturating_sub(usize::from(viewport_rows.max(1)));
    let max_scroll = u16::try_from(max_scroll).unwrap_or(u16::MAX);
    self.panel_scroll = self.panel_scroll.min(max_scroll);
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
          "Reactivity — {} modules · {} bindings · {} scopes · {} edges · {} template reads · showing {}",
          self.modules.len(),
          bindings,
          scopes,
          edges,
          template_reads,
          self.visible.len(),
        )
      },
      |error| format!("Reactivity TUI — unavailable: {error}"),
    );
    frame.render_widget(
      Paragraph::new(title).block(Block::default().borders(Borders::ALL).title("vue-vet")),
      area,
    );
  }

  fn draw_body(&mut self, frame: &mut Frame<'_>, area: Rect) -> u16 {
    let [list_area, panel_area] = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
      .areas(area);
    self.modules_area = list_area;
    self.panel_area = panel_area;
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
    let list_title =
      if self.focus == Focus::Modules { "modules (focused · click)" } else { "modules (click)" };
    let list = List::new(items)
      .block(Block::default().borders(Borders::ALL).title(list_title))
      .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
      .highlight_symbol("> ");
    frame.render_stateful_widget(list, list_area, &mut self.list_state);

    let lines = self.panel_lines();
    let panel_title = match (self.panel_mode, self.focus) {
      (PanelMode::Detail, Focus::Panel) => "detail (focused · j/k / wheel)",
      (PanelMode::Detail, Focus::Modules) => "detail (click/Tab · g graph)",
      (PanelMode::Graph, Focus::Panel) => "graph (focused · j/k / wheel)",
      (PanelMode::Graph, Focus::Modules) => "graph (click/Tab · g detail)",
    };
    let border =
      if self.focus == Focus::Panel { Style::default().fg(Color::Cyan) } else { Style::default() };
    let viewport = panel_area.height.saturating_sub(2);
    frame.render_widget(
      Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((self.panel_scroll, 0))
        .block(Block::default().borders(Borders::ALL).border_style(border).title(panel_title)),
      panel_area,
    );
    viewport
  }

  fn panel_lines(&self) -> Vec<Line<'static>> {
    let Some(module) = self.selected_module() else {
      return vec![Line::from("No modules to show. Press e to include empty modules.")];
    };
    match self.panel_mode {
      PanelMode::Detail => detail_lines(module),
      PanelMode::Graph => graph_lines(module),
    }
  }

  fn draw_footer(frame: &mut Frame<'_>, area: Rect, focus: Focus, mode: PanelMode) {
    let mode = match mode {
      PanelMode::Detail => "detail",
      PanelMode::Graph => "graph",
    };
    let focus = match focus {
      Focus::Modules => "modules",
      Focus::Panel => "panel",
    };
    frame.render_widget(
      Paragraph::new(format!(
        "click/Tab focus ({focus}) · j/k move/scroll · g {mode} · e empty · ? help · q quit"
      )),
      area,
    );
  }
}

/// Resolve a mouse click against the last drawn pane rects.
#[must_use]
fn hit_test(
  column: u16,
  row: u16,
  modules_area: Rect,
  panel_area: Rect,
  show_help: bool,
  list_offset: usize,
  visible_len: usize,
) -> MouseHit {
  if show_help {
    return MouseHit::Help;
  }
  if rect_contains(modules_area, column, row) {
    if visible_len == 0 {
      return MouseHit::Modules { visible_index: 0 };
    }
    let last = visible_len.saturating_sub(1);
    let inner_y = row.saturating_sub(modules_area.y).saturating_sub(1);
    let inner_height = modules_area.height.saturating_sub(2);
    if inner_height == 0
      || row <= modules_area.y
      || row >= modules_area.y.saturating_add(modules_area.height).saturating_sub(1)
    {
      return MouseHit::Modules { visible_index: list_offset.min(last) };
    }
    let index = list_offset.saturating_add(usize::from(inner_y));
    return MouseHit::Modules { visible_index: index.min(last) };
  }
  if rect_contains(panel_area, column, row) {
    return MouseHit::Panel;
  }
  MouseHit::Outside
}

const fn rect_contains(area: Rect, column: u16, row: u16) -> bool {
  column >= area.x
    && row >= area.y
    && column < area.x.saturating_add(area.width)
    && row < area.y.saturating_add(area.height)
}

fn detail_lines(module: &BrowseModule) -> Vec<Line<'static>> {
  let mut lines = vec![
    Line::from(Span::styled(module.id.clone(), Style::default().add_modifier(Modifier::BOLD))),
    Line::from(""),
    Line::from(Span::styled(
      "Readable labels — @N is a source byte offset (not a Vue id).",
      Style::default().fg(Color::DarkGray),
    )),
    Line::from(""),
  ];
  push_section(&mut lines, "bindings", &module.bindings, humanize_binding);
  push_section(&mut lines, "scopes", &module.scopes, humanize_scope);
  push_section(&mut lines, "edges", &module.edges, humanize_edge);
  push_section(&mut lines, "template reads", &module.template_reads, humanize_template_read);
  lines
}

fn graph_lines(module: &BrowseModule) -> Vec<Line<'static>> {
  let mut lines = vec![
    Line::from(Span::styled(module.id.clone(), Style::default().add_modifier(Modifier::BOLD))),
    Line::from(""),
    Line::from(Span::styled(
      "Who tracks which binding (edge → target)",
      Style::default().fg(Color::DarkGray),
    )),
    Line::from(""),
  ];

  let mut by_target: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
  for edge in &module.edges {
    if let Some((from, to)) = split_edge(edge) {
      let reader = humanize_source(from);
      *by_target.entry(to.to_owned()).or_default().entry(reader).or_default() += 1;
    }
  }
  for read in &module.template_reads {
    if let Some((binding, surface)) = split_template_read(read) {
      let reader = humanize_template_surface(surface);
      *by_target.entry(binding.to_owned()).or_default().entry(reader).or_default() += 1;
    }
  }

  if by_target.is_empty() {
    lines.push(Line::from("  (no edges or template reads)"));
    if !module.bindings.is_empty() {
      lines.push(Line::from(""));
      lines.push(Line::from("bindings without inbound edges yet:"));
      for binding in &module.bindings {
        lines.push(Line::from(format!("  · {}", humanize_binding(binding))));
      }
    }
    return lines;
  }

  for (target, readers) in by_target {
    lines.push(Line::from(Span::styled(
      format!("● {target}"),
      Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )));
    for (reader, count) in readers {
      let suffix = if count > 1 { format!(" ×{count}") } else { String::new() };
      lines.push(Line::from(format!("    ← {reader}{suffix}")));
    }
    lines.push(Line::from(""));
  }
  lines
}

fn push_section(
  lines: &mut Vec<Line<'static>>,
  label: &str,
  values: &[String],
  humanize: fn(&str) -> String,
) {
  lines.push(Line::from(Span::styled(
    label.to_owned(),
    Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan),
  )));
  if values.is_empty() {
    lines.push(Line::from("  (none)"));
    lines.push(Line::from(""));
    return;
  }
  for value in values {
    lines.push(Line::from(format!("  {}", humanize(value))));
  }
  lines.push(Line::from(""));
}

fn split_edge(edge: &str) -> Option<(&str, &str)> {
  edge.split_once(" -> ")
}

fn split_template_read(label: &str) -> Option<(&str, &str)> {
  label.split_once('@')
}

fn draw_help(frame: &mut Frame<'_>) {
  let area = centered_rect(70, 70, frame.area());
  let help = Paragraph::new(vec![
    Line::from(Span::styled("Help", Style::default().add_modifier(Modifier::BOLD))),
    Line::from(""),
    Line::from("Navigation"),
    Line::from("  click         select module / focus panel"),
    Line::from("  Tab / h l     focus modules ↔ panel"),
    Line::from("  j k / ↑ ↓     move list or scroll panel"),
    Line::from("  wheel         same as j/k for focused pane"),
    Line::from("  PgUp PgDn     scroll panel faster"),
    Line::from("  g             toggle detail ↔ graph"),
    Line::from("  e             show/hide empty modules"),
    Line::from("  ? / click     close this help"),
    Line::from(""),
    Line::from("What the labels mean"),
    Line::from("  binding → reactive local (ref / computed / …)"),
    Line::from("  scope   → tracking region (watchEffect, watch, …)"),
    Line::from("  edge    → scope/template read that depends on a binding"),
    Line::from("  @12345  → byte offset in the .vue/.ts source (not a Vue id)"),
    Line::from(""),
    Line::from("Graph view groups inbound reads by binding target."),
    Line::from("Example:  ● error   ← v-if   ← {{ }}"),
  ])
  .block(Block::default().borders(Borders::ALL).title("reactivity TUI"));
  frame.render_widget(Clear, area);
  frame.render_widget(help, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
  let vertical = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Percentage((100_u16.saturating_sub(percent_y)) / 2),
      Constraint::Percentage(percent_y),
      Constraint::Percentage((100_u16.saturating_sub(percent_y)) / 2),
    ])
    .split(area);
  let Some(middle) = vertical.get(1).copied() else {
    return area;
  };
  let horizontal = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
      Constraint::Percentage((100_u16.saturating_sub(percent_x)) / 2),
      Constraint::Percentage(percent_x),
      Constraint::Percentage((100_u16.saturating_sub(percent_x)) / 2),
    ])
    .split(middle);
  horizontal.get(1).copied().unwrap_or(area)
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
  // Best-effort mouse wheel support; ignore failures on hosts without mouse.
  if let Err(_error) = crossterm_enable_mouse() {}
  let run_result = app.run(&mut terminal);
  if let Err(_error) = crossterm_disable_mouse() {}
  if let Err(error) = ratatui::try_restore() {
    return Err(format!("failed to restore terminal after reactivity TUI: {error}"));
  }
  run_result
}

fn crossterm_enable_mouse() -> std::io::Result<()> {
  use ratatui::crossterm::execute;
  execute!(std::io::stdout(), event::EnableMouseCapture)
}

fn crossterm_disable_mouse() -> std::io::Result<()> {
  use ratatui::crossterm::execute;
  execute!(std::io::stdout(), event::DisableMouseCapture)
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
    let mut stats = ReactivityModuleStats::empty(id);
    stats.bindings = bindings;
    stats.scopes = scopes;
    stats.edges = edges;
    stats.template_reads = reads;
    stats.binding_labels = (0..bindings).map(|index| format!("b{index}:ref")).collect();
    stats.scope_labels = (0..scopes).map(|index| format!("watch_effect(s{index})")).collect();
    stats.edge_labels =
      (0..edges).map(|index| format!("template:if@{index} -> target{index}")).collect();
    stats.template_labels = (0..reads).map(|index| format!("name{index}@interpolation")).collect();
    stats
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

  #[test]
  fn hit_test_selects_module_row_and_panel() {
    let modules = Rect { x: 0, y: 3, width: 20, height: 10 };
    let panel = Rect { x: 20, y: 3, width: 40, height: 10 };
    assert_eq!(hit_test(2, 5, modules, panel, false, 0, 5), MouseHit::Modules { visible_index: 1 });
    assert_eq!(hit_test(2, 5, modules, panel, false, 2, 5), MouseHit::Modules { visible_index: 3 });
    assert_eq!(hit_test(25, 6, modules, panel, false, 0, 5), MouseHit::Panel);
    assert_eq!(hit_test(25, 6, modules, panel, true, 0, 5), MouseHit::Help);
  }

  #[test]
  fn click_focuses_panel_and_selects_module() {
    let ranked = ranked_modules(&[
      stats("a.vue", 3, 0, 0, 0),
      stats("b.vue", 2, 0, 0, 0),
      stats("c.vue", 1, 0, 0, 0),
    ]);
    let mut app = BrowseApp::new(ranked, None);
    app.modules_area = Rect { x: 0, y: 3, width: 20, height: 10 };
    app.panel_area = Rect { x: 20, y: 3, width: 40, height: 10 };
    app.on_click(25, 6);
    assert_eq!(app.focus, Focus::Panel);
    app.on_click(2, 5);
    assert_eq!(app.focus, Focus::Modules);
    assert_eq!(app.selected_module().map(|module| module.id.as_str()), Some("b.vue"));
  }

  #[test]
  fn graph_groups_inbound_readers_by_binding() {
    let module = BrowseModule {
      id: "App.vue".into(),
      weight: 3,
      bindings: vec!["error:ref".into()],
      scopes: Vec::new(),
      edges: vec![
        "template:if@1 -> error".into(),
        "template:interpolation@2 -> error".into(),
        "watch_sources:watch@3 -> backend".into(),
      ],
      template_reads: Vec::new(),
    };
    let rendered = graph_lines(&module)
      .into_iter()
      .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<Vec<_>>().join(""))
      .collect::<Vec<_>>()
      .join("\n");
    assert!(rendered.contains("● error"));
    assert!(rendered.contains("← v-if"));
    assert!(rendered.contains("← {{ }}"));
    assert!(rendered.contains("● backend"));
    assert!(rendered.contains("← watch()"));
  }

  #[test]
  fn panel_focus_scrolls_instead_of_changing_module() {
    let ranked = ranked_modules(&[stats("a.vue", 3, 0, 0, 0), stats("b.vue", 2, 0, 0, 0)]);
    let mut app = BrowseApp::new(ranked, None);
    app.focus = Focus::Panel;
    app.panel_scroll = 0;
    app.handle_key(KeyCode::Char('j'));
    assert_eq!(app.list_state.selected(), Some(0));
    assert_eq!(app.panel_scroll, 1);
  }
}
