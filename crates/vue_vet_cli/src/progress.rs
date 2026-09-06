//! Bounded CLI scan progress on stderr.
//!
//! Live TTY: one status line, ASCII spinner, elapsed time, optional monotonic
//! file counter. Plain / redirected / `TERM=dumb`: compact per-phase lines.
//! Never writes to stdout.

use std::{
  io::{self, IsTerminal, Write},
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  thread::{self, JoinHandle},
  time::{Duration, Instant},
};

use vue_vet_session::{ProgressEvent, ProgressReporter};

const REFRESH: Duration = Duration::from_millis(100);
const INITIAL_DELAY: Duration = Duration::from_millis(80);
const DEFAULT_WIDTH: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressStyle {
  Silent,
  Plain,
  Live,
}

#[must_use]
pub fn detect_style(enabled: bool) -> ProgressStyle {
  if !enabled {
    return ProgressStyle::Silent;
  }
  if stderr_is_live_tty() { ProgressStyle::Live } else { ProgressStyle::Plain }
}

fn stderr_is_live_tty() -> bool {
  io::stderr().is_terminal() && !term_is_dumb()
}

fn term_is_dumb() -> bool {
  std::env::var_os("TERM")
    .is_some_and(|value| value.is_empty() || value.eq_ignore_ascii_case("dumb"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressModel {
  pub phase: &'static str,
  pub done: usize,
  pub total: usize,
  pub has_counter: bool,
  plain_logged: u32,
}

impl Default for ProgressModel {
  fn default() -> Self {
    Self { phase: "Starting", done: 0, total: 0, has_counter: false, plain_logged: 0 }
  }
}

impl ProgressModel {
  pub fn apply(&mut self, event: &ProgressEvent) -> PlainLog {
    match event {
      ProgressEvent::Discovering => self.begin_scan("Discovering workspace", PhaseBit::DISCOVERING),
      ProgressEvent::CheckingCache => self.enter("Checking cache", PhaseBit::CHECKING_CACHE),
      ProgressEvent::CacheHit => self.enter("Cache hit", PhaseBit::CACHE_HIT),
      ProgressEvent::SavingCache => self.enter("Saving cache", PhaseBit::SAVING_CACHE),
      ProgressEvent::Parsing { .. } => self.enter("Parsing files", PhaseBit::PARSING),
      ProgressEvent::BuildingGraph => {
        self.enter("Building project graph", PhaseBit::BUILDING_GRAPH)
      }
      ProgressEvent::LoadingExternalSeeds { .. } => {
        self.enter("Resolving dependencies", PhaseBit::RESOLVING)
      }
      ProgressEvent::RunningRules { files } => {
        self.phase = "Checking rules";
        self.done = 0;
        self.total = *files;
        self.has_counter = true;
        self.log_once(PhaseBit::RUNNING_RULES)
      }
      ProgressEvent::FileRules { done, total } => {
        self.phase = "Checking rules";
        self.has_counter = true;
        self.total = (*total).max(self.total);
        let next = (*done).min(self.total);
        if next > self.done {
          self.done = next;
        }
        if self.total > 0 && self.done >= self.total {
          self.log_once(PhaseBit::RULES_DONE)
        } else {
          PlainLog::Skip
        }
      }
      ProgressEvent::WritingReport => self.enter("Writing report", PhaseBit::WRITING_REPORT),
    }
  }

  fn begin_scan(&mut self, phase: &'static str, bit: u32) -> PlainLog {
    self.phase = phase;
    self.done = 0;
    self.total = 0;
    self.has_counter = false;
    self.plain_logged = 0;
    self.log_once(bit)
  }

  fn enter(&mut self, phase: &'static str, bit: u32) -> PlainLog {
    self.phase = phase;
    self.has_counter = false;
    self.done = 0;
    self.total = 0;
    self.log_once(bit)
  }

  fn log_once(&mut self, bit: u32) -> PlainLog {
    if self.plain_logged & bit != 0 {
      return PlainLog::Skip;
    }
    self.plain_logged |= bit;
    PlainLog::Line(self.plain_line())
  }

  #[must_use]
  pub fn plain_line(&self) -> String {
    if self.has_counter {
      format!(
        "vue-vet: {} {}/{} eligible files",
        self.phase.to_ascii_lowercase(),
        self.done,
        self.total
      )
    } else {
      format!("vue-vet: {}", self.phase.to_ascii_lowercase())
    }
  }

  #[must_use]
  pub fn counter(&self) -> Option<(usize, usize)> {
    self.has_counter.then_some((self.done, self.total))
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlainLog {
  Skip,
  Line(String),
}

struct PhaseBit;

impl PhaseBit {
  const DISCOVERING: u32 = 1 << 0;
  const CHECKING_CACHE: u32 = 1 << 1;
  const CACHE_HIT: u32 = 1 << 2;
  const SAVING_CACHE: u32 = 1 << 3;
  const PARSING: u32 = 1 << 4;
  const BUILDING_GRAPH: u32 = 1 << 5;
  const RESOLVING: u32 = 1 << 6;
  const RUNNING_RULES: u32 = 1 << 7;
  const RULES_DONE: u32 = 1 << 8;
  const WRITING_REPORT: u32 = 1 << 9;
}

#[must_use]
pub fn format_elapsed(elapsed: Duration) -> String {
  let total = elapsed.as_secs();
  let hours = total / 3600;
  let minutes = (total % 3600) / 60;
  let seconds = total % 60;
  if hours > 0 {
    format!("{hours}:{minutes:02}:{seconds:02}")
  } else {
    format!("{minutes}:{seconds:02}")
  }
}

#[must_use]
pub const fn spinner_frame(ticks: u64) -> char {
  match ticks % 4 {
    0 => '|',
    1 => '/',
    2 => '-',
    _ => '\\',
  }
}

struct PhaseNames<'a> {
  full: &'a str,
  compact: &'a str,
  short: &'a str,
}

fn phase_names(phase: &str) -> PhaseNames<'_> {
  match phase {
    "Discovering workspace" => {
      PhaseNames { full: "Discovering workspace", compact: "Discovering", short: "Discover" }
    }
    "Parsing files" => PhaseNames { full: "Parsing files", compact: "Parsing", short: "Parse" },
    "Building project graph" => {
      PhaseNames { full: "Building project graph", compact: "Building graph", short: "Graph" }
    }
    "Resolving dependencies" => {
      PhaseNames { full: "Resolving dependencies", compact: "Resolving", short: "Resolve" }
    }
    "Checking rules" => PhaseNames { full: "Checking rules", compact: "Rules", short: "Rules" },
    "Checking cache" => PhaseNames { full: "Checking cache", compact: "Cache", short: "Cache" },
    "Cache hit" => PhaseNames { full: "Cache hit", compact: "Cache hit", short: "Hit" },
    "Saving cache" => PhaseNames { full: "Saving cache", compact: "Saving", short: "Save" },
    "Writing report" => PhaseNames { full: "Writing report", compact: "Report", short: "Report" },
    other => PhaseNames { full: other, compact: other, short: other },
  }
}

/// Pack a live status line into `width` columns. Never includes a filename,
/// percentage, or ETA. One column is reserved so the line does not wrap.
#[must_use]
pub fn render_status_line(
  width: usize,
  spinner: char,
  phase: &str,
  counter: Option<(usize, usize)>,
  elapsed: Duration,
) -> String {
  let budget = width.saturating_sub(1);
  if budget == 0 {
    return String::new();
  }
  let elapsed = format_elapsed(elapsed);
  let count = counter.map(|(done, total)| format!("{done}/{total}"));
  let count = count.as_deref();
  let names = phase_names(phase);
  if let Some(line) = pack(budget, true, spinner, names.full, count, &elapsed) {
    return line;
  }
  if names.compact != names.full
    && let Some(line) = pack(budget, true, spinner, names.compact, count, &elapsed)
  {
    return line;
  }
  if let Some(line) = pack(budget, false, spinner, names.compact, count, &elapsed) {
    return line;
  }
  if names.short != names.compact
    && let Some(line) = pack(budget, false, spinner, names.short, count, &elapsed)
  {
    return line;
  }
  if count.is_some() {
    if let Some(line) = pack(budget, false, spinner, names.compact, None, &elapsed) {
      return line;
    }
    if names.short != names.compact
      && let Some(line) = pack(budget, false, spinner, names.short, None, &elapsed)
    {
      return line;
    }
  }
  let spin_elapsed = format!("{spinner} {elapsed}");
  if spin_elapsed.chars().count() <= budget {
    return spin_elapsed;
  }
  spinner.to_string()
}

fn pack(
  budget: usize,
  brand: bool,
  spinner: char,
  phase: &str,
  count: Option<&str>,
  elapsed: &str,
) -> Option<String> {
  let mut parts = Vec::new();
  if brand {
    parts.push("vue-vet");
  }
  let spin = spinner.to_string();
  parts.push(spin.as_str());
  parts.push(phase);
  if let Some(count) = count {
    parts.push(count);
  }
  parts.push(elapsed);
  let line = parts.join(" ");
  (line.chars().count() <= budget).then_some(line)
}

trait StatusIo: Send + Sync {
  fn paint(&self, line: &str) -> bool;
  fn clear(&self);
  fn write_line(&self, line: &str);
}

struct StderrIo;

impl StatusIo for StderrIo {
  fn paint(&self, line: &str) -> bool {
    use ratatui::crossterm::{
      QueueableCommand,
      cursor::MoveToColumn,
      terminal::{Clear, ClearType},
    };
    let mut stderr = io::stderr();
    stderr
      .queue(MoveToColumn(0))
      .and_then(|out| out.queue(Clear(ClearType::UntilNewLine)))
      .and_then(|out| out.write_all(line.as_bytes()))
      .and_then(|()| stderr.flush())
      .is_ok()
  }

  fn clear(&self) {
    use ratatui::crossterm::{
      QueueableCommand,
      cursor::MoveToColumn,
      terminal::{Clear, ClearType},
    };
    let mut stderr = io::stderr();
    drop(
      stderr
        .queue(MoveToColumn(0))
        .and_then(|out| out.queue(Clear(ClearType::UntilNewLine)))
        .and_then(Write::flush),
    );
  }

  fn write_line(&self, line: &str) {
    let mut stderr = io::stderr();
    drop(writeln!(stderr, "{line}"));
  }
}

struct LiveState {
  model: ProgressModel,
  started: Instant,
  ticks: u64,
  painted: bool,
}

struct LiveRuntime {
  shared: Arc<Mutex<LiveState>>,
  stop: Arc<AtomicBool>,
  sink: Arc<dyn StatusIo>,
  width: Option<usize>,
  delay: Duration,
  refresh: Duration,
}

pub struct ProgressController {
  style: ProgressStyle,
  shared: Option<Arc<Mutex<LiveState>>>,
  stop: Arc<AtomicBool>,
  worker: Option<JoinHandle<()>>,
  sink: Option<Arc<dyn StatusIo>>,
  reporter: Option<ProgressReporter>,
}

impl ProgressController {
  #[must_use]
  pub fn start(style: ProgressStyle) -> Self {
    match style {
      ProgressStyle::Silent => Self::silent(),
      ProgressStyle::Plain => Self::plain(Arc::new(StderrIo)),
      ProgressStyle::Live => Self::live(Arc::new(StderrIo), None, INITIAL_DELAY, REFRESH),
    }
  }

  fn silent() -> Self {
    Self {
      style: ProgressStyle::Silent,
      shared: None,
      stop: Arc::new(AtomicBool::new(true)),
      worker: None,
      sink: None,
      reporter: None,
    }
  }

  fn plain(sink: Arc<dyn StatusIo>) -> Self {
    let model = Arc::new(Mutex::new(ProgressModel::default()));
    let writer = Arc::clone(&sink);
    let reporter = ProgressReporter::new(move |event: &ProgressEvent| {
      let Ok(mut model) = model.lock() else {
        return;
      };
      if let PlainLog::Line(line) = model.apply(event) {
        writer.write_line(&line);
      }
    });
    Self {
      style: ProgressStyle::Plain,
      shared: None,
      stop: Arc::new(AtomicBool::new(true)),
      worker: None,
      sink: Some(sink),
      reporter: Some(reporter),
    }
  }

  fn live(
    sink: Arc<dyn StatusIo>,
    width: Option<usize>,
    delay: Duration,
    refresh: Duration,
  ) -> Self {
    let shared = Arc::new(Mutex::new(LiveState {
      model: ProgressModel::default(),
      started: Instant::now(),
      ticks: 0,
      painted: false,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let for_reporter = Arc::clone(&shared);
    let reporter = ProgressReporter::new(move |event: &ProgressEvent| {
      let Ok(mut state) = for_reporter.lock() else {
        return;
      };
      let _ = state.model.apply(event);
    });
    let runtime = LiveRuntime {
      shared: Arc::clone(&shared),
      stop: Arc::clone(&stop),
      sink: Arc::clone(&sink),
      width,
      delay,
      refresh,
    };
    let worker = thread::Builder::new()
      .name("vue-vet-progress".into())
      .spawn(move || live_refresh_loop(&runtime))
      .ok();
    Self {
      style: ProgressStyle::Live,
      shared: Some(shared),
      stop,
      worker,
      sink: Some(sink),
      reporter: Some(reporter),
    }
  }

  #[must_use]
  pub fn reporter(&self) -> Option<ProgressReporter> {
    self.reporter.clone()
  }

  pub fn emit(&self, event: &ProgressEvent) {
    if let Some(reporter) = &self.reporter {
      reporter.emit(event);
    }
  }

  pub fn stop(&mut self) {
    self.stop.store(true, Ordering::SeqCst);
    if let Some(worker) = self.worker.take() {
      worker.thread().unpark();
      drop(worker.join());
    }
    if self.style == ProgressStyle::Live
      && let Some(shared) = &self.shared
      && let Ok(mut state) = shared.lock()
      && state.painted
    {
      if let Some(sink) = &self.sink {
        sink.clear();
      }
      state.painted = false;
    }
    self.reporter = None;
  }
}

impl Drop for ProgressController {
  fn drop(&mut self) {
    self.stop();
  }
}

fn live_refresh_loop(runtime: &LiveRuntime) {
  let origin = Instant::now();
  while !runtime.stop.load(Ordering::Relaxed) {
    park_until(&runtime.stop, runtime.refresh);
    if runtime.stop.load(Ordering::Relaxed) {
      break;
    }
    if origin.elapsed() < runtime.delay {
      continue;
    }
    let Ok(mut state) = runtime.shared.lock() else {
      continue;
    };
    state.ticks = state.ticks.saturating_add(1);
    let width = runtime.width.unwrap_or_else(terminal_width);
    let line = render_status_line(
      width,
      spinner_frame(state.ticks),
      state.model.phase,
      state.model.counter(),
      state.started.elapsed(),
    );
    if runtime.sink.paint(&line) {
      state.painted = true;
    }
  }
}

fn park_until(stop: &AtomicBool, timeout: Duration) {
  let deadline = Instant::now() + timeout;
  while !stop.load(Ordering::Relaxed) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return;
    }
    thread::park_timeout(remaining);
  }
}

fn terminal_width() -> usize {
  resolve_terminal_width(
    ratatui::crossterm::terminal::size().ok().map(|(cols, _)| cols),
    std::env::var("COLUMNS").ok().and_then(|value| value.parse().ok()),
  )
}

#[must_use]
pub fn resolve_terminal_width(reported_cols: Option<u16>, columns_env: Option<usize>) -> usize {
  if let Some(cols) = reported_cols.filter(|cols| *cols > 0) {
    return usize::from(cols);
  }
  columns_env.filter(|width| *width > 0).unwrap_or(DEFAULT_WIDTH)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::{Condvar, MutexGuard};

  #[derive(Clone)]
  struct RecordingIo {
    ops: Arc<Mutex<Vec<IoOp>>>,
    paints: Arc<Mutex<usize>>,
    painted: Arc<Condvar>,
    paint_ok: bool,
  }

  #[derive(Clone, Debug, Eq, PartialEq)]
  enum IoOp {
    Paint(String),
    Clear,
    Line(String),
  }

  impl RecordingIo {
    fn new() -> Self {
      Self::with_paint_ok(true)
    }

    fn failing() -> Self {
      Self::with_paint_ok(false)
    }

    fn with_paint_ok(paint_ok: bool) -> Self {
      Self {
        ops: Arc::new(Mutex::new(Vec::new())),
        paints: Arc::new(Mutex::new(0)),
        painted: Arc::new(Condvar::new()),
        paint_ok,
      }
    }

    fn ops(&self) -> Vec<IoOp> {
      lock(&self.ops).clone()
    }

    fn wait_paints(&self, count: usize) {
      let deadline = Instant::now() + Duration::from_secs(5);
      let mut paints = lock(&self.paints);
      while *paints < count {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for {count} paints (have {})", *paints);
        let (guard, wait) = match self.painted.wait_timeout(paints, remaining) {
          Ok(result) => result,
          Err(error) => error.into_inner(),
        };
        paints = guard;
        assert!(
          !wait.timed_out() || *paints >= count,
          "timed out waiting for {count} paints (have {})",
          *paints
        );
      }
      drop(paints);
    }
  }

  impl StatusIo for RecordingIo {
    fn paint(&self, line: &str) -> bool {
      lock(&self.ops).push(IoOp::Paint(line.to_owned()));
      *lock(&self.paints) += 1;
      self.painted.notify_all();
      self.paint_ok
    }

    fn clear(&self) {
      lock(&self.ops).push(IoOp::Clear);
    }

    fn write_line(&self, line: &str) {
      lock(&self.ops).push(IoOp::Line(line.to_owned()));
    }
  }

  fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  fn assert_status(width: usize, phase: &str, counter: Option<(usize, usize)>, elapsed: Duration) {
    let line = render_status_line(width, '/', phase, counter, elapsed);
    let budget = width.saturating_sub(1);
    assert!(line.chars().count() <= budget, "width {width}: {line}");
    assert!(!line.contains('%'), "{line}");
    assert!(!line.contains("ETA"), "{line}");
  }

  #[test]
  fn status_line_width_80_keeps_brand_phase_and_elapsed() {
    let line = render_status_line(80, '|', "Building project graph", None, Duration::from_secs(12));
    assert!(line.chars().count() <= 79, "{line}");
    assert!(line.contains("vue-vet"), "{line}");
    assert!(line.contains("Building project graph"), "{line}");
    assert!(line.contains("0:12"), "{line}");
  }

  #[test]
  fn status_line_width_36_keeps_stage_and_elapsed() {
    let line = render_status_line(36, '/', "Building project graph", None, Duration::from_secs(12));
    assert!(line.chars().count() <= 35, "{line}");
    assert!(line.contains("0:12"), "{line}");
    assert!(
      line.contains("Building") || line.contains("Graph") || line.contains("graph"),
      "{line}"
    );
  }

  #[test]
  fn status_line_width_20_keeps_short_stage_elapsed_and_counter() {
    let rules = render_status_line(20, '/', "Checking rules", Some((4, 6)), Duration::from_secs(3));
    assert!(rules.chars().count() <= 19, "{rules}");
    assert!(rules.contains("0:03"), "{rules}");
    assert!(rules.contains("Rules") || rules.contains("rules"), "{rules}");
    assert!(rules.contains("4/6"), "{rules}");
    let graph =
      render_status_line(20, '/', "Building project graph", None, Duration::from_secs(12));
    assert!(graph.chars().count() <= 19, "{graph}");
    assert!(graph.contains("0:12"), "{graph}");
    assert!(graph.contains("Graph") || graph.contains("graph"), "{graph}");
  }

  #[test]
  fn status_line_very_small_width_stays_in_budget() {
    for width in [1, 4, 8] {
      assert_status(width, "Building project graph", None, Duration::from_secs(12));
    }
    let tiny = render_status_line(4, '/', "Building project graph", None, Duration::from_secs(12));
    assert!(tiny.chars().count() <= 3, "{tiny}");
  }

  #[test]
  fn status_line_includes_monotonic_counter_when_room() {
    let line = render_status_line(80, '|', "Checking rules", Some((4, 6)), Duration::from_secs(3));
    assert!(line.contains("4/6"), "{line}");
    assert!(line.contains("Checking rules"), "{line}");
    assert!(line.contains("0:03"), "{line}");
  }

  #[test]
  fn status_line_formats_hours_and_remaining_phases() {
    let long =
      render_status_line(80, '|', "Resolving dependencies", None, Duration::from_secs(3661));
    assert!(long.contains("1:01:01"), "{long}");
    assert!(long.contains("Resolving dependencies"), "{long}");
    for phase in [
      "Discovering workspace",
      "Parsing files",
      "Checking cache",
      "Cache hit",
      "Saving cache",
      "Writing report",
    ] {
      let line = render_status_line(36, '/', phase, None, Duration::from_secs(5));
      assert!(line.contains("0:05"), "{phase} -> {line}");
      assert!(!line.is_empty(), "{phase}");
    }
    assert_eq!(spinner_frame(0), '|');
    assert_eq!(spinner_frame(1), '/');
    assert_eq!(spinner_frame(2), '-');
    assert_eq!(spinner_frame(3), '\\');
  }

  #[test]
  fn terminal_width_prefers_real_nonzero_size_over_columns() {
    assert_eq!(resolve_terminal_width(Some(4), Some(80)), 4);
    assert_eq!(resolve_terminal_width(Some(36), Some(80)), 36);
    assert_eq!(resolve_terminal_width(Some(0), Some(20)), 20);
    assert_eq!(resolve_terminal_width(None, Some(20)), 20);
    assert_eq!(resolve_terminal_width(None, None), 80);
  }

  #[test]
  fn out_of_order_file_counts_are_monotonic_and_clamped() {
    let mut model = ProgressModel::default();
    let _ = model.apply(&ProgressEvent::RunningRules { files: 6 });
    let _ = model.apply(&ProgressEvent::FileRules { done: 4, total: 6 });
    let _ = model.apply(&ProgressEvent::FileRules { done: 3, total: 6 });
    assert_eq!(model.counter(), Some((4, 6)));
    let _ = model.apply(&ProgressEvent::FileRules { done: 9, total: 6 });
    assert_eq!(model.counter(), Some((6, 6)));
  }

  #[test]
  fn fresh_analysis_resets_counters() {
    let mut model = ProgressModel::default();
    let _ = model.apply(&ProgressEvent::RunningRules { files: 6 });
    let _ = model.apply(&ProgressEvent::FileRules { done: 6, total: 6 });
    let _ = model.apply(&ProgressEvent::Discovering);
    assert_eq!(model.counter(), None);
    assert_eq!(model.phase, "Discovering workspace");
    let _ = model.apply(&ProgressEvent::RunningRules { files: 2 });
    assert_eq!(model.counter(), Some((0, 2)));
  }

  #[test]
  fn plain_log_is_bounded_per_phase() {
    let mut model = ProgressModel::default();
    assert!(matches!(model.apply(&ProgressEvent::Discovering), PlainLog::Line(_)));
    assert!(matches!(model.apply(&ProgressEvent::BuildingGraph), PlainLog::Line(_)));
    assert_eq!(model.apply(&ProgressEvent::BuildingGraph), PlainLog::Skip);
    let _ = model.apply(&ProgressEvent::CheckingCache);
    let _ = model.apply(&ProgressEvent::CacheHit);
    let _ = model.apply(&ProgressEvent::SavingCache);
    let _ = model.apply(&ProgressEvent::LoadingExternalSeeds { roots: 2 });
    let _ = model.apply(&ProgressEvent::WritingReport);
    let _ = model.apply(&ProgressEvent::RunningRules { files: 50 });
    let mut lines = 0_u32;
    for done in 1..=50 {
      if matches!(model.apply(&ProgressEvent::FileRules { done, total: 50 }), PlainLog::Line(_)) {
        lines += 1;
      }
    }
    assert_eq!(lines, 1, "file completions must not flood the plain log");
  }

  #[test]
  fn silent_controller_has_no_reporter() {
    let controller = ProgressController::start(ProgressStyle::Silent);
    assert!(controller.reporter().is_none());
    assert_eq!(detect_style(false), ProgressStyle::Silent);
  }

  #[test]
  fn live_heartbeat_repeats_without_new_events_then_clears() {
    let sink = RecordingIo::new();
    let mut progress = ProgressController::live(
      Arc::new(sink.clone()),
      Some(80),
      Duration::ZERO,
      Duration::from_millis(15),
    );
    progress.emit(&ProgressEvent::BuildingGraph);
    sink.wait_paints(2);
    progress.stop();
    sink.write_line("vue-vet: writing report");
    let ops = sink.ops();
    let paints: Vec<_> = ops
      .iter()
      .filter_map(|op| match op {
        IoOp::Paint(line) => Some(line.as_str()),
        IoOp::Clear | IoOp::Line(_) => None,
      })
      .collect();
    assert!(paints.len() >= 2, "timer must emit at least two frames: {ops:?}");
    if let (Some(first), Some(second)) = (paints.first(), paints.get(1)) {
      assert!(first.contains("Building"), "{first}");
      assert!(second.contains("Building"), "{second}");
      assert_ne!(first, second, "spinner or elapsed should advance: {paints:?}");
    }
    let clear_at = ops.iter().position(|op| matches!(op, IoOp::Clear));
    let report_at =
      ops.iter().position(|op| matches!(op, IoOp::Line(line) if line == "vue-vet: writing report"));
    assert!(
      matches!((clear_at, report_at), (Some(clear), Some(report)) if clear < report),
      "clear must precede the simulated report: {ops:?}"
    );
    if let Some(clear) = clear_at {
      assert!(
        !ops.iter().skip(clear.saturating_add(1)).any(|op| matches!(op, IoOp::Paint(_))),
        "no paints after clear: {ops:?}"
      );
    }
  }

  #[test]
  fn live_stop_joins_without_a_tick() {
    let sink = RecordingIo::new();
    let mut progress = ProgressController::live(
      Arc::new(sink.clone()),
      Some(80),
      Duration::ZERO,
      Duration::from_secs(5),
    );
    progress.stop();
    assert!(sink.ops().is_empty(), "prompt shutdown must not paint: {:?}", sink.ops());
  }

  #[test]
  fn live_failed_paints_do_not_clear() {
    let sink = RecordingIo::failing();
    let mut progress = ProgressController::live(
      Arc::new(sink.clone()),
      Some(80),
      Duration::ZERO,
      Duration::from_millis(15),
    );
    progress.emit(&ProgressEvent::BuildingGraph);
    sink.wait_paints(1);
    progress.stop();
    let ops = sink.ops();
    assert!(ops.iter().any(|op| matches!(op, IoOp::Paint(_))), "{ops:?}");
    assert!(!ops.iter().any(|op| matches!(op, IoOp::Clear)), "{ops:?}");
  }

  #[test]
  fn plain_controller_writes_phase_lines_through_sink() {
    let sink = RecordingIo::new();
    let mut progress = ProgressController::plain(Arc::new(sink.clone()));
    progress.emit(&ProgressEvent::Discovering);
    progress.emit(&ProgressEvent::BuildingGraph);
    progress.emit(&ProgressEvent::BuildingGraph);
    progress.emit(&ProgressEvent::CheckingCache);
    progress.stop();
    assert_eq!(
      sink.ops(),
      vec![
        IoOp::Line("vue-vet: discovering workspace".into()),
        IoOp::Line("vue-vet: building project graph".into()),
        IoOp::Line("vue-vet: checking cache".into()),
      ]
    );
  }
}
