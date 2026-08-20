//! Interactive browser for per-module reactivity tracer facts.
//!
//! Pure browse-state helpers are unit-tested without a TTY. The ratatui loop
//! requires an interactive terminal and is wired from the CLI flag.

use std::{
  cmp::Reverse,
  collections::{BTreeMap, BTreeSet},
  io::IsTerminal,
};

use ratatui::{
  DefaultTerminal, Frame,
  crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind},
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use vue_vet_reporters::{
  ComponentNavDigest, ComponentNavLink, ComponentNavModule, ReactivityModuleStats,
  ReactivityScopeDetail, humanize_binding, humanize_edge, humanize_scope, humanize_source,
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
  /// Navigable list of bindings to inspect.
  Pick,
  /// Inbound readers + outbound dependencies for one binding.
  Inspect,
  /// Structural component `uses` / `used_by` (project graph; not prop dataflow).
  Components,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseHit {
  Modules { visible_index: usize },
  Panel { content_row: Option<usize> },
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
  /// Same `scope_details` as `--print-reactivity` (includes explain-scope `summary`).
  pub scope_details: Vec<ReactivityScopeDetail>,
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
      scope_details: module.scope_details.clone(),
    }
  }
}

#[derive(Debug)]
struct BrowseApp {
  modules: Vec<BrowseModule>,
  /// Path → component `uses` / `used_by` (structural project-graph edges).
  component_nav: BTreeMap<String, ComponentNavModule>,
  visible: Vec<usize>,
  list_state: ListState,
  show_empty: bool,
  focus: Focus,
  panel_mode: PanelMode,
  panel_scroll: u16,
  /// Cursor into [`Self::pick_bindings`] while picking / inspecting navigation.
  binding_cursor: usize,
  /// Bare binding name under inspect (`error`, not `error:ref`).
  selected_binding: Option<String>,
  /// Cursor into `uses` / `used_by` rows in Components mode.
  component_cursor: usize,
  show_help: bool,
  error: Option<String>,
  should_quit: bool,
  modules_area: Rect,
  panel_area: Rect,
}

impl BrowseApp {
  fn new(
    modules: Vec<BrowseModule>,
    error: Option<String>,
    component_nav: ComponentNavDigest,
  ) -> Self {
    let component_nav =
      component_nav.modules.into_iter().map(|module| (module.id.clone(), module)).collect();
    let mut app = Self {
      modules,
      component_nav,
      visible: Vec::new(),
      list_state: ListState::default(),
      show_empty: false,
      focus: Focus::Modules,
      panel_mode: PanelMode::Detail,
      panel_scroll: 0,
      binding_cursor: 0,
      selected_binding: None,
      component_cursor: 0,
      show_help: false,
      error,
      should_quit: false,
      modules_area: Rect::default(),
      panel_area: Rect::default(),
    };
    app.rebuild_visible();
    app
  }

  fn pick_bindings(&self) -> Vec<(String, String)> {
    let Some(module) = self.selected_module() else {
      return Vec::new();
    };
    expand_binding_picks(module)
  }

  fn clear_inspect(&mut self) {
    self.selected_binding = None;
    if self.panel_mode == PanelMode::Inspect {
      self.panel_mode = PanelMode::Pick;
    }
    self.panel_scroll = 0;
  }

  fn select_binding(&mut self, name: impl Into<String>) {
    self.selected_binding = Some(name.into());
    self.panel_mode = PanelMode::Inspect;
    self.panel_scroll = 0;
    self.focus = Focus::Panel;
  }

  fn open_binding_picker(&mut self) {
    let picks = self.pick_bindings();
    if picks.is_empty() {
      return;
    }
    if let Some(selected) = &self.selected_binding {
      self.binding_cursor = picks.iter().position(|(name, _)| name == selected).unwrap_or(0);
    } else {
      self.binding_cursor = self.binding_cursor.min(picks.len().saturating_sub(1));
    }
    self.selected_binding = None;
    self.panel_mode = PanelMode::Pick;
    self.panel_scroll = 0;
    self.focus = Focus::Panel;
  }

  fn confirm_picked_binding(&mut self) {
    let picks = self.pick_bindings();
    let Some((name, _)) = picks.get(self.binding_cursor) else {
      return;
    };
    self.select_binding(name.clone());
  }

  fn move_binding_cursor(&mut self, forward: bool) {
    let len = self.pick_bindings().len();
    if len == 0 {
      self.binding_cursor = 0;
      return;
    }
    self.binding_cursor = if forward {
      self.binding_cursor.saturating_add(1) % len
    } else if self.binding_cursor == 0 {
      len.saturating_sub(1)
    } else {
      self.binding_cursor.saturating_sub(1)
    };
    // Keep the highlighted pick roughly in view.
    let cursor = u16::try_from(self.binding_cursor).unwrap_or(u16::MAX);
    if cursor < self.panel_scroll {
      self.panel_scroll = cursor;
    } else if cursor >= self.panel_scroll.saturating_add(8) {
      self.panel_scroll = cursor.saturating_sub(7);
    }
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
    self.binding_cursor = 0;
    self.selected_binding = None;
    if matches!(self.panel_mode, PanelMode::Inspect | PanelMode::Pick) {
      self.panel_mode = PanelMode::Graph;
    }
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
    self.binding_cursor = 0;
    self.selected_binding = None;
    if matches!(self.panel_mode, PanelMode::Inspect | PanelMode::Pick) {
      self.panel_mode = PanelMode::Graph;
    }
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
        MouseEventKind::Down(MouseButton::Left) => self.on_click(mouse.column, mouse.row, false),
        MouseEventKind::Down(MouseButton::Right) => self.on_click(mouse.column, mouse.row, true),
        _ => {}
      },
      _ => {}
    }
    Ok(())
  }

  fn on_click(&mut self, column: u16, row: u16, right: bool) {
    let list_offset = self.list_state.offset();
    match hit_test(
      column,
      row,
      self.modules_area,
      self.panel_area,
      self.show_help,
      list_offset,
      self.visible.len(),
      self.panel_scroll,
    ) {
      MouseHit::Help => self.show_help = false,
      MouseHit::Modules { visible_index } => {
        self.focus = Focus::Modules;
        self.list_state.select(Some(visible_index));
        self.panel_scroll = 0;
        self.binding_cursor = 0;
        self.selected_binding = None;
        if matches!(self.panel_mode, PanelMode::Inspect | PanelMode::Pick) {
          self.panel_mode = PanelMode::Graph;
        }
      }
      MouseHit::Panel { content_row } => {
        self.focus = Focus::Panel;
        if right {
          self.on_panel_right_click(content_row);
        } else {
          self.on_panel_left_click(content_row);
        }
      }
      MouseHit::Outside => {}
    }
  }

  fn on_panel_left_click(&mut self, content_row: Option<usize>) {
    let Some(row) = content_row else {
      return;
    };
    match self.panel_mode {
      PanelMode::Pick => {
        if let Some(index) = self.pick_index_at_content_row(row) {
          self.binding_cursor = index;
        }
      }
      PanelMode::Graph => {
        if let Some(name) = self.graph_binding_at_content_row(row) {
          let picks = self.pick_bindings();
          if let Some(index) = picks.iter().position(|(binding, _)| binding == &name) {
            self.binding_cursor = index;
          }
        }
      }
      PanelMode::Components => {
        if let Some(index) = self.component_index_at_content_row(row) {
          self.component_cursor = index;
        }
      }
      PanelMode::Detail | PanelMode::Inspect => {}
    }
  }

  fn on_panel_right_click(&mut self, content_row: Option<usize>) {
    if self.panel_mode == PanelMode::Inspect {
      self.clear_inspect();
      return;
    }
    let Some(row) = content_row else {
      self.open_binding_picker();
      return;
    };
    if let Some(name) = self.binding_name_at_content_row(row) {
      self.select_binding(name);
      return;
    }
    self.open_binding_picker();
  }

  fn binding_name_at_content_row(&self, row: usize) -> Option<String> {
    match self.panel_mode {
      PanelMode::Pick => self
        .pick_index_at_content_row(row)
        .and_then(|index| self.pick_bindings().get(index).map(|(name, _)| name.clone())),
      PanelMode::Graph => self.graph_binding_at_content_row(row),
      PanelMode::Detail | PanelMode::Inspect | PanelMode::Components => None,
    }
  }

  fn component_rows(&self) -> Vec<(String, ComponentNavLink)> {
    let Some(module) = self.selected_module() else {
      return Vec::new();
    };
    let Some(nav) = self.component_nav.get(&module.id) else {
      return Vec::new();
    };
    let mut rows = Vec::new();
    for link in &nav.uses {
      rows.push(("uses".into(), link.clone()));
    }
    for link in &nav.used_by {
      rows.push(("used_by".into(), link.clone()));
    }
    rows
  }

  fn component_index_at_content_row(&self, row: usize) -> Option<usize> {
    // components_lines: title, blank, disclaimer, blank, then labeled rows with
    // section headers — map via rendered "  > " / "    " peer lines only.
    let module = self.selected_module()?;
    let lines = components_lines(module, self.component_nav.get(&module.id), self.component_cursor);
    let text = line_text(lines.get(row)?);
    let trimmed = text.trim_start();
    let marker = trimmed.strip_prefix("> ").or_else(|| trimmed.strip_prefix("  "))?;
    let peer = marker.split_whitespace().next()?;
    self.component_rows().iter().position(|(_, link)| link.peer == peer)
  }

  fn open_components_panel(&mut self) {
    self.selected_binding = None;
    self.panel_mode = PanelMode::Components;
    self.component_cursor = 0;
    self.panel_scroll = 0;
    self.focus = Focus::Panel;
  }

  fn confirm_component_jump(&mut self) {
    let rows = self.component_rows();
    let Some((_, link)) = rows.get(self.component_cursor) else {
      return;
    };
    let peer = link.peer.clone();
    if let Some(index) = self.modules.iter().position(|module| module.id == peer) {
      if let Some(visible_index) = self.visible.iter().position(|item| *item == index) {
        self.list_state.select(Some(visible_index));
      } else {
        // Peer is empty-weight; reveal it.
        self.show_empty = true;
        self.rebuild_visible();
        if let Some(visible_index) = self.visible.iter().position(|item| *item == index) {
          self.list_state.select(Some(visible_index));
        }
      }
      self.component_cursor = 0;
      self.panel_scroll = 0;
    }
  }

  fn move_component_cursor(&mut self, forward: bool) {
    let len = self.component_rows().len();
    if len == 0 {
      self.component_cursor = 0;
      return;
    }
    self.component_cursor = if forward {
      self.component_cursor.saturating_add(1) % len
    } else if self.component_cursor == 0 {
      len.saturating_sub(1)
    } else {
      self.component_cursor.saturating_sub(1)
    };
  }

  fn pick_index_at_content_row(&self, row: usize) -> Option<usize> {
    // pick_lines: title, blank, hint, blank, then one row per binding.
    let first = 4;
    row.checked_sub(first).filter(|index| *index < self.pick_bindings().len())
  }

  fn graph_binding_at_content_row(&self, row: usize) -> Option<String> {
    let module = self.selected_module()?;
    let lines = graph_lines(module);
    let line = lines.get(row)?;
    let text = line_text(line);
    text.strip_prefix("● ").map(str::to_owned)
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
      KeyCode::Char('q') => self.should_quit = true,
      KeyCode::Esc => {
        if self.selected_binding.is_some() {
          self.clear_inspect();
        } else if self.panel_mode == PanelMode::Pick {
          self.panel_mode = PanelMode::Graph;
          self.panel_scroll = 0;
        } else if self.panel_mode == PanelMode::Components {
          self.panel_mode = PanelMode::Detail;
          self.panel_scroll = 0;
        } else {
          self.should_quit = true;
        }
      }
      KeyCode::Char('?') => self.show_help = true,
      KeyCode::Tab | KeyCode::BackTab => {
        self.focus = match self.focus {
          Focus::Modules => Focus::Panel,
          Focus::Panel => Focus::Modules,
        };
      }
      KeyCode::Char('g') => {
        self.selected_binding = None;
        self.panel_mode = match self.panel_mode {
          PanelMode::Detail => PanelMode::Graph,
          PanelMode::Graph | PanelMode::Pick | PanelMode::Inspect | PanelMode::Components => {
            PanelMode::Detail
          }
        };
        self.panel_scroll = 0;
        self.focus = Focus::Panel;
      }
      KeyCode::Char('b') => self.open_binding_picker(),
      KeyCode::Char('c') => self.open_components_panel(),
      KeyCode::Char('x') if self.selected_binding.is_some() => self.clear_inspect(),
      KeyCode::Enter | KeyCode::Char(' ') if self.focus == Focus::Panel => match self.panel_mode {
        PanelMode::Pick => self.confirm_picked_binding(),
        PanelMode::Graph => {
          // Enter on graph: inspect binding under cursor if we have picks.
          let picks = self.pick_bindings();
          if let Some((name, _)) = picks.get(self.binding_cursor) {
            self.select_binding(name.clone());
          } else {
            self.open_binding_picker();
          }
        }
        PanelMode::Detail => self.open_binding_picker(),
        PanelMode::Inspect => {}
        PanelMode::Components => self.confirm_component_jump(),
      },
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
        Focus::Panel if self.panel_mode == PanelMode::Pick => self.move_binding_cursor(true),
        Focus::Panel if self.panel_mode == PanelMode::Components => {
          self.move_component_cursor(true);
        }
        Focus::Panel => self.panel_scroll = self.panel_scroll.saturating_add(1),
      },
      KeyCode::Up | KeyCode::Char('k') => match self.focus {
        Focus::Modules => self.move_selection(false),
        Focus::Panel if self.panel_mode == PanelMode::Pick => self.move_binding_cursor(false),
        Focus::Panel if self.panel_mode == PanelMode::Components => {
          self.move_component_cursor(false);
        }
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
    Self::draw_footer(frame, footer, self.focus, self.panel_mode, self.selected_binding.as_deref());
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
    let panel_title = match (self.panel_mode, self.focus, self.selected_binding.as_deref()) {
      (PanelMode::Detail, Focus::Panel, _) => "detail (focused · b pick · c components)".into(),
      (PanelMode::Detail, Focus::Modules, _) => "detail (click/Tab · b pick · c · g)".into(),
      (PanelMode::Graph, Focus::Panel, _) => "graph (focused · b/Enter inspect)".into(),
      (PanelMode::Graph, Focus::Modules, _) => "graph (click/Tab · right-click inspect)".into(),
      (PanelMode::Pick, _, _) => "pick binding (j/k · Enter · right-click)".into(),
      (PanelMode::Inspect, _, Some(name)) => {
        return_panel_title_inspect(name, self.focus == Focus::Panel)
      }
      (PanelMode::Inspect, _, None) => "inspect".into(),
      (PanelMode::Components, _, _) => {
        "components (structural · Enter jumps · not prop dataflow)".into()
      }
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
      PanelMode::Pick => pick_lines(module, self.binding_cursor),
      PanelMode::Inspect => {
        let name = self.selected_binding.as_deref().unwrap_or("?");
        inspect_lines(module, name)
      }
      PanelMode::Components => {
        components_lines(module, self.component_nav.get(&module.id), self.component_cursor)
      }
    }
  }

  fn draw_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    focus: Focus,
    mode: PanelMode,
    selected: Option<&str>,
  ) {
    let mode = match (mode, selected) {
      (PanelMode::Detail, _) => "detail",
      (PanelMode::Graph, _) => "graph",
      (PanelMode::Pick, _) => "pick",
      (PanelMode::Inspect, Some(name)) => name,
      (PanelMode::Inspect, None) => "inspect",
      (PanelMode::Components, _) => "components",
    };
    let focus = match focus {
      Focus::Modules => "modules",
      Focus::Panel => "panel",
    };
    frame.render_widget(
      Paragraph::new(format!(
        "focus:{focus} · {mode} · b pick · c components · Enter/right-click · Esc/x · g · ? · q"
      )),
      area,
    );
  }
}

fn return_panel_title_inspect(name: &str, focused: bool) -> String {
  if focused {
    format!("inspect ● {name} (Esc/x clear · right-click clear)")
  } else {
    format!("inspect ● {name}")
  }
}

/// Resolve a mouse click against the last drawn pane rects.
#[must_use]
#[expect(clippy::too_many_arguments, reason = "hit-test takes explicit layout geometry")]
fn hit_test(
  column: u16,
  row: u16,
  modules_area: Rect,
  panel_area: Rect,
  show_help: bool,
  list_offset: usize,
  visible_len: usize,
  panel_scroll: u16,
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
    let inner_y = row.saturating_sub(panel_area.y).saturating_sub(1);
    let inner_height = panel_area.height.saturating_sub(2);
    let content_row = if inner_height == 0
      || row <= panel_area.y
      || row >= panel_area.y.saturating_add(panel_area.height).saturating_sub(1)
    {
      None
    } else {
      Some(usize::from(panel_scroll.saturating_add(inner_y)))
    };
    return MouseHit::Panel { content_row };
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

fn pick_lines(module: &BrowseModule, cursor: usize) -> Vec<Line<'static>> {
  let mut lines = vec![
    Line::from(Span::styled(module.id.clone(), Style::default().add_modifier(Modifier::BOLD))),
    Line::from(""),
    Line::from(Span::styled(
      "Select a binding — Enter/right-click inspects readers + dependencies.",
      Style::default().fg(Color::DarkGray),
    )),
    Line::from(""),
  ];
  let picks = expand_binding_picks(module);
  if picks.is_empty() {
    lines.push(Line::from("  (no bindings)"));
    return lines;
  }
  for (index, (name, kind)) in picks.iter().enumerate() {
    let kind = kind.replace('_', " ");
    let marker = if index == cursor { ">" } else { " " };
    let style = if index == cursor {
      Style::default().add_modifier(Modifier::REVERSED)
    } else {
      Style::default()
    };
    // Member picks are indented so they read as bag fields, not a second binding.
    let row = if name.contains('.') {
      format!("{marker}   {name}  ({kind})")
    } else {
      format!("{marker} {name}  ({kind})")
    };
    lines.push(Line::from(Span::styled(row, style)));
  }
  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled(
    "Tip: binding → all readers · indented bag.member → that field · computed → deps",
    Style::default().fg(Color::DarkGray),
  )));
  lines
}

fn inspect_lines(module: &BrowseModule, target: &str) -> Vec<Line<'static>> {
  let (binding, property) = split_inspect_target(target);
  let kind = module
    .bindings
    .iter()
    .find(|label| bare_binding_name(label) == binding)
    .map_or_else(|| "?".into(), |label| binding_kind_label(label).replace('_', " "));
  let kind = match property {
    Some(property) => format!("{kind} · .{property}"),
    None => kind,
  };
  let mut lines = vec![
    Line::from(Span::styled(
      format!("● {target}  ({kind})"),
      Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )),
    Line::from(""),
  ];
  let summaries = scope_summaries_for(module, binding, property);
  if !summaries.is_empty() {
    lines.push(Line::from(Span::styled(
      "would Vue re-run? — same verdict as --explain-scope",
      Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));
    for summary in summaries {
      lines.push(Line::from(format!("  {summary}")));
    }
    lines.push(Line::from(""));
  }
  lines.push(Line::from(Span::styled(
    "readers (inbound) — who tracks / reads this",
    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
  )));
  let inbound = inbound_readers(module, binding, property);
  if inbound.is_empty() {
    lines.push(Line::from("  (none)"));
  } else {
    for (reader, count) in inbound {
      let suffix = if count > 1 { format!(" ×{count}") } else { String::new() };
      lines.push(Line::from(format!("  ← {reader}{suffix}")));
    }
  }
  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled(
    "dependencies (outbound) — what this binding's scope reads",
    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
  )));
  let outbound = outbound_dependencies(module, binding, property);
  if outbound.is_empty() {
    lines.push(Line::from("  (none — typical for plain ref / reactive)"));
  } else {
    for (dep, count) in outbound {
      let suffix = if count > 1 { format!(" ×{count}") } else { String::new() };
      lines.push(Line::from(format!("  → {dep}{suffix}")));
    }
  }
  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled(
    "Esc / x / right-click clears selection · b returns to picker",
    Style::default().fg(Color::DarkGray),
  )));
  lines
}

/// Explain-scope summaries for the binding that owns a tracking scope.
/// Member picks (`props.count`) stay inbound-only — they are not a scope.
fn scope_summaries_for<'module>(
  module: &'module BrowseModule,
  binding: &str,
  property: Option<&str>,
) -> Vec<&'module str> {
  if property.is_some() {
    return Vec::new();
  }
  let mut summaries = module
    .scope_details
    .iter()
    .filter(|scope| scope.binding.as_deref() == Some(binding))
    .filter_map(|scope| scope.summary.as_deref())
    .filter(|summary| !summary.is_empty())
    .collect::<Vec<_>>();
  summaries.sort_unstable();
  summaries.dedup();
  summaries
}

/// Expand reactive / shallowReactive bags into bag + bag.property pick rows.
///
/// Member rows keep target `bag.prop` for inspect, but use a distinct kind label
/// (`reactive · .prop`) so the picker does not look like a duplicate binding.
fn expand_binding_picks(module: &BrowseModule) -> Vec<(String, String)> {
  let mut picks = Vec::new();
  for label in &module.bindings {
    let name = bare_binding_name(label).to_owned();
    let kind = binding_kind_label(label).to_owned();
    picks.push((name.clone(), kind.clone()));
    if !is_reactive_bag_kind(&kind) {
      continue;
    }
    for property in properties_for_bag(module, &name) {
      picks.push((format!("{name}.{property}"), format!("{kind} · .{property}")));
    }
  }
  picks
}

fn is_reactive_bag_kind(kind: &str) -> bool {
  matches!(kind, "reactive" | "shallow_reactive")
}

fn properties_for_bag(module: &BrowseModule, bag: &str) -> BTreeSet<String> {
  let mut properties = BTreeSet::new();
  let prefix = format!("{bag}.");
  for edge in &module.edges {
    if let Some((_, to)) = split_edge(edge)
      && let Some(rest) = to.strip_prefix(&prefix)
    {
      let property = rest.split('.').next().unwrap_or(rest);
      if !property.is_empty() {
        properties.insert(property.to_owned());
      }
    }
  }
  properties
}

fn split_inspect_target(target: &str) -> (&str, Option<&str>) {
  match target.split_once('.') {
    Some((binding, property)) if !property.is_empty() => (binding, Some(property)),
    _ => (target, None),
  }
}

fn edge_to_matches(to_path: &str, binding: &str, property: Option<&str>) -> bool {
  property.map_or_else(
    || to_path == binding || to_path.starts_with(&format!("{binding}.")),
    |property| to_path == format!("{binding}.{property}"),
  )
}

fn inbound_readers(
  module: &BrowseModule,
  binding: &str,
  property: Option<&str>,
) -> BTreeMap<String, usize> {
  let mut readers = BTreeMap::new();
  for edge in &module.edges {
    if let Some((from, to)) = split_edge(edge)
      && edge_to_matches(to, binding, property)
    {
      *readers.entry(humanize_source(from)).or_default() += 1;
    }
  }
  // Template joins name the bare binding only; include them for bag-level inspect.
  if property.is_none() {
    for read in &module.template_reads {
      if let Some((name, surface)) = split_template_read(read)
        && name == binding
      {
        *readers.entry(humanize_template_surface(surface)).or_default() += 1;
      }
    }
  }
  readers
}

fn outbound_dependencies(
  module: &BrowseModule,
  binding: &str,
  property: Option<&str>,
) -> BTreeMap<String, usize> {
  let mut deps = BTreeMap::new();
  // Member picks (`props.count`) are inbound-only; outbound uses the bare binding.
  if property.is_some() {
    return deps;
  }
  for edge in &module.edges {
    if let Some((from, to)) = split_edge(edge)
      && edge_from_is_binding(from, binding)
    {
      *deps.entry(to.to_owned()).or_default() += 1;
    }
  }
  deps
}

fn edge_from_is_binding(from: &str, binding: &str) -> bool {
  if from == binding {
    return true;
  }
  // Span-qualified synthetic labels rarely use bare binding as from; computed
  // scopes emit the bare binding name when known (see scope_edge_from).
  let head = from.split_once('@').map_or(from, |(head, _)| head);
  head == binding || head.ends_with(&format!(":{binding}"))
}

fn bare_binding_name(label: &str) -> &str {
  label.split_once(':').map_or(label, |(name, _)| name)
}

fn binding_kind_label(label: &str) -> &str {
  label.split_once(':').map_or("?", |(_, kind)| kind)
}

fn line_text(line: &Line<'_>) -> String {
  line.spans.iter().map(|span| span.content.as_ref()).collect::<Vec<_>>().join("")
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

fn components_lines(
  module: &BrowseModule,
  nav: Option<&ComponentNavModule>,
  cursor: usize,
) -> Vec<Line<'static>> {
  let mut lines = vec![
    Line::from(Span::styled(module.id.clone(), Style::default().add_modifier(Modifier::BOLD))),
    Line::from(""),
    Line::from(Span::styled(
      "Component reference graph (uses / used by) — not props dataflow.",
      Style::default().fg(Color::DarkGray),
    )),
    Line::from(""),
  ];
  let Some(nav) = nav else {
    lines.push(Line::from("  (no ComponentUsage / AutoComponent edges for this file)"));
    return lines;
  };

  let mut row_index = 0usize;
  let mut push_section =
    |title: &str, links: &[ComponentNavLink], lines: &mut Vec<Line<'static>>| {
      lines.push(Line::from(Span::styled(
        title.to_owned(),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
      )));
      if links.is_empty() {
        lines.push(Line::from("  (none)"));
      } else {
        for link in links {
          let marker = if row_index == cursor { ">" } else { " " };
          let style = if row_index == cursor {
            Style::default().add_modifier(Modifier::REVERSED)
          } else {
            Style::default()
          };
          lines.push(Line::from(Span::styled(
            format!(
              "{marker} {peer}  ({kind} · <{specifier}>)",
              peer = link.peer,
              kind = link.kind,
              specifier = link.specifier
            ),
            style,
          )));
          row_index += 1;
        }
      }
      lines.push(Line::from(""));
    };

  push_section("uses (this file templates)", &nav.uses, &mut lines);
  push_section("used by (who templates this)", &nav.used_by, &mut lines);
  lines.push(Line::from(Span::styled(
    "Enter jumps to peer module when it is in the loaded list",
    Style::default().fg(Color::DarkGray),
  )));
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
    Line::from("  b             pick a binding to inspect"),
    Line::from("  c             component uses / used by (structural)"),
    Line::from("  Enter/Space   inspect highlighted binding / jump peer"),
    Line::from("  right-click   inspect binding under cursor / clear"),
    Line::from("  Esc / x       clear inspect (Esc again quits)"),
    Line::from("  e             show/hide empty modules"),
    Line::from("  ? / click     close this help"),
    Line::from(""),
    Line::from("Inspect a binding"),
    Line::from("  would Vue re-run? same ScopeExplain summary as --explain-scope"),
    Line::from("  readers (←)       who tracks / reads this ref"),
    Line::from("  dependencies (→)  what a computed/effect binding reads"),
    Line::from("  props.count       member pick filters inbound readers"),
    Line::from(""),
    Line::from("Components panel"),
    Line::from("  Project-graph ComponentUsage / AutoComponent only."),
    Line::from("  Not parent :prop → child props dataflow."),
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
  component_nav: &ComponentNavDigest,
) -> Result<(), String> {
  if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
    return Err(
      "--reactivity-tui requires an interactive terminal; use --print-reactivity for text detail"
        .into(),
    );
  }
  let modules = ranked_modules(stats);
  let mut app = BrowseApp::new(modules, error, component_nav.clone());
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
  fn ranked_modules_keep_explain_scope_summaries() {
    let mut stats = stats("App.vue", 1, 1, 0, 0);
    let mut detail = vue_vet_reporters::scope_detail(
      "computed",
      "computed",
      Some("label".into()),
      vue_vet_reporters::ReactivitySpanRef::new(10, 8),
    );
    detail.summary = Some("`label` has no known reactive dependency".into());
    stats.scope_details = vec![detail];
    let ranked = ranked_modules(&[stats]);
    assert_eq!(
      ranked
        .first()
        .and_then(|module| module.scope_details.first())
        .and_then(|scope| { scope.summary.as_deref() }),
      Some("`label` has no known reactive dependency")
    );
  }

  #[test]
  fn hides_empty_modules_until_toggled() {
    let ranked = ranked_modules(&[stats("hot.vue", 2, 0, 0, 0), stats("empty.ts", 0, 0, 0, 0)]);
    let mut app = BrowseApp::new(ranked, None, ComponentNavDigest::default());
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
    let mut app = BrowseApp::new(ranked, None, ComponentNavDigest::default());
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
    assert_eq!(
      hit_test(2, 5, modules, panel, false, 0, 5, 0),
      MouseHit::Modules { visible_index: 1 }
    );
    assert_eq!(
      hit_test(2, 5, modules, panel, false, 2, 5, 0),
      MouseHit::Modules { visible_index: 3 }
    );
    assert_eq!(
      hit_test(25, 6, modules, panel, false, 0, 5, 0),
      MouseHit::Panel { content_row: Some(2) }
    );
    assert_eq!(hit_test(25, 6, modules, panel, true, 0, 5, 0), MouseHit::Help);
  }

  #[test]
  fn click_focuses_panel_and_selects_module() {
    let ranked = ranked_modules(&[
      stats("a.vue", 3, 0, 0, 0),
      stats("b.vue", 2, 0, 0, 0),
      stats("c.vue", 1, 0, 0, 0),
    ]);
    let mut app = BrowseApp::new(ranked, None, ComponentNavDigest::default());
    app.modules_area = Rect { x: 0, y: 3, width: 20, height: 10 };
    app.panel_area = Rect { x: 20, y: 3, width: 40, height: 10 };
    app.on_click(25, 6, false);
    assert_eq!(app.focus, Focus::Panel);
    app.on_click(2, 5, false);
    assert_eq!(app.focus, Focus::Modules);
    assert_eq!(app.selected_module().map(|module| module.id.as_str()), Some("b.vue"));
  }

  #[test]
  fn inspect_shows_inbound_readers_and_outbound_deps() {
    let module = BrowseModule {
      id: "App.vue".into(),
      weight: 4,
      bindings: vec!["count:ref".into(), "double:computed".into()],
      scopes: Vec::new(),
      edges: vec![
        "double -> count".into(),
        "template:if@1 -> count".into(),
        "watch_sources:watch@2 -> double".into(),
      ],
      template_reads: vec!["count@interpolation".into()],
      scope_details: Vec::new(),
    };
    let count = line_text_join(&inspect_lines(&module, "count"));
    assert!(count.contains("readers (inbound)"));
    assert!(count.contains("← v-if"));
    assert!(count.contains("← {{ }}"));
    assert!(count.contains("← computed(double)") || count.contains("← double"));
    assert!(count.contains("(none — typical for plain ref") || count.contains("dependencies"));

    let double = line_text_join(&inspect_lines(&module, "double"));
    assert!(double.contains("→ count"));
    assert!(double.contains("← watch()"));
    assert!(!double.contains("would Vue re-run?"));
  }

  #[test]
  fn inspect_shows_explain_scope_summary_for_owning_binding() {
    let mut module = BrowseModule {
      id: "App.vue".into(),
      weight: 2,
      bindings: vec!["label:computed".into(), "count:ref".into()],
      scopes: vec!["computed(label)".into()],
      edges: vec!["label -> count".into()],
      template_reads: Vec::new(),
      scope_details: vec![vue_vet_reporters::scope_detail(
        "computed",
        "computed",
        Some("label".into()),
        vue_vet_reporters::ReactivitySpanRef::new(70, 24),
      )],
    };
    module.scope_details[0].summary = Some(
      "`label` has no known reactive dependency — Vue will not re-run it when state changes".into(),
    );

    let label = line_text_join(&inspect_lines(&module, "label"));
    assert!(label.contains("would Vue re-run?"));
    assert!(label.contains("no known reactive dependency"));
    assert!(label.contains("→ count"));

    let count = line_text_join(&inspect_lines(&module, "count"));
    assert!(!count.contains("would Vue re-run?"), "plain refs are not tracking scopes: {count}");
  }

  #[test]
  fn components_panel_lists_uses_and_used_by() {
    let ranked = ranked_modules(&[
      stats("pages/index.vue", 1, 0, 0, 0),
      stats("components/Demo.vue", 1, 0, 0, 0),
    ]);
    let nav = ComponentNavDigest {
      modules: vec![
        ComponentNavModule {
          id: "pages/index.vue".into(),
          uses: vec![ComponentNavLink {
            peer: "components/Demo.vue".into(),
            kind: "auto_component".into(),
            specifier: "Demo".into(),
            span: vue_vet_reporters::ReactivitySpanRef::new(10, 4),
          }],
          used_by: Vec::new(),
        },
        ComponentNavModule {
          id: "components/Demo.vue".into(),
          uses: Vec::new(),
          used_by: vec![ComponentNavLink {
            peer: "pages/index.vue".into(),
            kind: "auto_component".into(),
            specifier: "Demo".into(),
            span: vue_vet_reporters::ReactivitySpanRef::new(10, 4),
          }],
        },
      ],
    };
    let mut app = BrowseApp::new(ranked, None, nav);
    // Rank puts Demo first by id; select the page that *uses* Demo.
    let page_index = app.visible.iter().position(|index| {
      app.modules.get(*index).is_some_and(|module| module.id == "pages/index.vue")
    });
    assert_eq!(page_index, Some(1));
    app.list_state.select(page_index);
    app.focus = Focus::Panel;
    app.handle_key(KeyCode::Char('c'));
    assert_eq!(app.panel_mode, PanelMode::Components);
    let rendered = line_text_join(&app.panel_lines());
    assert!(rendered.contains("Component reference graph"));
    assert!(rendered.contains("components/Demo.vue"));
    assert!(rendered.contains("not props dataflow"));
    app.handle_key(KeyCode::Enter);
    assert_eq!(app.selected_module().map(|module| module.id.as_str()), Some("components/Demo.vue"));
  }

  #[test]
  fn reactive_bag_picks_expand_properties_and_filter_inbound() {
    let module = BrowseModule {
      id: "Child.vue".into(),
      weight: 3,
      bindings: vec!["props:reactive".into(), "label:computed".into()],
      scopes: Vec::new(),
      edges: vec![
        "label -> props.count".into(),
        "watch_sources:watch@1 -> props.mode".into(),
        "template:if@2 -> props".into(),
      ],
      template_reads: vec!["props@if".into()],
      scope_details: Vec::new(),
    };
    let picks = expand_binding_picks(&module);
    assert!(picks.iter().any(|(name, kind)| name == "props" && kind == "reactive"));
    assert!(
      picks.iter().any(|(name, kind)| name == "props.count" && kind == "reactive · .count"),
      "member picks must not reuse bare bag kind: {picks:?}"
    );
    assert!(picks.iter().any(|(name, kind)| name == "props.mode" && kind == "reactive · .mode"));

    let picker = line_text_join(&pick_lines(&module, 1));
    assert!(picker.contains("props  (reactive)"));
    assert!(
      picker.contains("  props.count  (reactive · .count)"),
      "member rows should indent and show member kind: {picker}"
    );

    let bag = line_text_join(&inspect_lines(&module, "props"));
    assert!(bag.contains("← computed(label)") || bag.contains("← label"));
    assert!(bag.contains("← watch()"));
    assert!(bag.contains("← v-if"));

    let count = line_text_join(&inspect_lines(&module, "props.count"));
    assert!(count.contains("← computed(label)") || count.contains("← label"));
    assert!(!count.contains("← watch()"));
    assert!(!count.contains("← v-if"));

    let label = line_text_join(&inspect_lines(&module, "label"));
    assert!(label.contains("→ props.count"));
  }

  #[test]
  fn pick_enter_and_esc_toggle_inspect() {
    let ranked = ranked_modules(&[stats("a.vue", 2, 0, 1, 0)]);
    let mut app = BrowseApp::new(ranked, None, ComponentNavDigest::default());
    app.focus = Focus::Panel;
    app.handle_key(KeyCode::Char('b'));
    assert_eq!(app.panel_mode, PanelMode::Pick);
    app.handle_key(KeyCode::Enter);
    assert_eq!(app.panel_mode, PanelMode::Inspect);
    assert_eq!(app.selected_binding.as_deref(), Some("b0"));
    app.handle_key(KeyCode::Esc);
    assert!(app.selected_binding.is_none());
    assert_eq!(app.panel_mode, PanelMode::Pick);
    app.handle_key(KeyCode::Esc);
    assert_eq!(app.panel_mode, PanelMode::Graph);
  }

  fn line_text_join(lines: &[Line<'static>]) -> String {
    lines.iter().map(line_text).collect::<Vec<_>>().join("\n")
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
      scope_details: Vec::new(),
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
    let mut app = BrowseApp::new(ranked, None, ComponentNavDigest::default());
    app.focus = Focus::Panel;
    app.panel_scroll = 0;
    app.handle_key(KeyCode::Char('j'));
    assert_eq!(app.list_state.selected(), Some(0));
    assert_eq!(app.panel_scroll, 1);
  }
}
