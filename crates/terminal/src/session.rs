use crate::{
  palette::Palette,
  search::{find_in_row, fold_query, Highlight, Match, RowCell},
  snapshot::{spans, Cursor, CursorShape, Glyph, Snapshot},
};
use alacritty_terminal::{
  event::{Event, EventListener, WindowSize},
  event_loop::{EventLoop, EventLoopSender, Msg},
  grid::{Dimensions, Scroll},
  index::{Column, Line},
  sync::FairMutex,
  term::{cell::Flags, Config, Term, TermMode},
  tty::{self, Options, Shell},
  vte::ansi::NamedColor,
};
use std::{
  collections::HashMap,
  path::PathBuf,
  sync::{Arc, Mutex, OnceLock},
};

/// What a session tells the interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEvent {
  /// The screen changed.
  Wakeup,
  /// The program rang the terminal bell.
  Bell,
  /// The program set a window title.
  Title(String),
  /// The shell reported its working directory.
  Directory(PathBuf),
  /// The program asked to put text on the clipboard (OSC 52).
  ClipboardStore(String),
  /// The shell exited.
  Exit(Option<i32>),
}

/// How to start a session.
#[derive(Clone, Debug, Default)]
pub struct SessionOptions {
  /// A program and its arguments; `None` starts the user's login shell.
  pub program: Option<(String, Vec<String>)>,
  pub working_directory: Option<PathBuf>,
  /// The initial colors used to answer terminal color queries.
  pub palette: Option<Palette>,
}

/// Terminal modes that change how input is encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modes {
  pub app_cursor: bool,
  pub bracketed_paste: bool,
  /// The program wants button presses and releases.
  pub mouse_click: bool,
  /// The program also wants motion while a button is held.
  pub mouse_drag: bool,
  /// The program also wants motion with no button held.
  pub mouse_motion: bool,
  /// Mouse reports use the SGR encoding.
  pub sgr_mouse: bool,
  /// The program runs on the alternate screen (a pager or editor), which has no history.
  pub alt_screen: bool,
  /// The wheel is sent as arrow keys on the alternate screen when the mouse is not reported.
  pub alternate_scroll: bool,
  /// The program wants to know when the terminal gains or loses focus.
  pub focus_reporting: bool,
}

impl Modes {
  /// Whether the program asked to receive mouse events.
  pub fn mouse_reporting(&self) -> bool {
    self.mouse_click || self.mouse_drag || self.mouse_motion
  }
}

type Notify = Arc<dyn Fn(SessionEvent) + Send + Sync>;

#[derive(Clone)]
struct Proxy {
  notify: Notify,
  exit_code: Arc<Mutex<Option<i32>>>,
  sender: Arc<OnceLock<EventLoopSender>>,
  palette: Arc<Mutex<Palette>>,
  #[cfg(unix)]
  foreground: Option<Arc<ForegroundActivity>>,
}

#[cfg(unix)]
struct ForegroundActivity {
  tty: std::fs::File,
  title_activity: Mutex<Option<(u32, bool)>>,
}

#[cfg(unix)]
impl ForegroundActivity {
  fn group_id(&self) -> Option<u32> {
    rustix::termios::tcgetpgrp(&self.tty).ok().map(|group| group.as_raw_pid() as u32)
  }

  fn observe_title(&self, title: &str) {
    // Codex and Claude Code show a standalone Braille frame while working. It disappears (Codex) or becomes
    // a static mark such as "✳" (Claude Code) when the task ends, though the program keeps the foreground.
    let working = title.split_whitespace().any(|part| {
      let mut chars = part.chars();
      matches!((chars.next(), chars.next()), (Some('\u{2800}'..='\u{28ff}'), None))
    });
    let idle = title.starts_with('✳');
    let waiting = title.contains("[ ! ] Action Required") || title.contains("[ . ] Action Required");
    let mut activity = self.title_activity.lock().unwrap_or_else(|error| error.into_inner());
    if !working && !waiting && !idle && activity.is_none() {
      return;
    }
    let Some(group) = self.group_id() else { return };
    if working || waiting || idle || activity.is_some_and(|(tracked_group, _)| tracked_group == group) {
      *activity = Some((group, working));
    }
  }

  fn is_working(&self, group: u32) -> bool {
    self.title_activity.lock().unwrap_or_else(|error| error.into_inner()).is_none_or(|(tracked_group, working)| tracked_group != group || working)
  }
}

impl EventListener for Proxy {
  fn send_event(&self, event: Event) {
    match event {
      Event::Wakeup => (self.notify)(SessionEvent::Wakeup),
      Event::Bell => (self.notify)(SessionEvent::Bell),
      Event::Title(title) => self.send_title(title),
      Event::ResetTitle => self.send_title(String::new()),
      Event::ClipboardStore(_, text) => (self.notify)(SessionEvent::ClipboardStore(text)),
      // ChildExit arrives before the PTY's remaining output is drained. Term::exit sends Exit
      // after that drain, so consumers can safely read the final screen before closing the pane.
      Event::ChildExit(status) => {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt as _;

        let code = status.code();
        #[cfg(unix)]
        let code = code.or_else(|| status.signal().map(|signal| 128 + signal));
        *self.exit_code.lock().unwrap_or_else(|error| error.into_inner()) = code;
      }
      Event::Exit => {
        let code = *self.exit_code.lock().unwrap_or_else(|error| error.into_inner());
        (self.notify)(SessionEvent::Exit(code));
      }
      // Replies the program asked for, such as cursor position reports.
      Event::PtyWrite(text) => {
        if let Some(sender) = self.sender.get() {
          let _ = sender.send(Msg::Input(text.into_bytes().into()));
        }
      }
      Event::ColorRequest(index, format) => {
        let palette = self.palette.lock().unwrap_or_else(|error| error.into_inner());
        let color = match index {
          0..=255 => palette.indexed(index as u8),
          index if index == NamedColor::Background as usize => palette.background,
          _ => palette.foreground,
        };
        if let Some(sender) = self.sender.get() {
          let _ = sender.send(Msg::Input(format(alacritty_terminal::vte::ansi::Rgb { r: color.r, g: color.g, b: color.b }).into_bytes().into()));
        }
      }
      _ => {}
    }
  }
}

impl Proxy {
  fn send_title(&self, title: String) {
    #[cfg(unix)]
    if let Some(foreground) = &self.foreground {
      foreground.observe_title(&title);
    }
    (self.notify)(SessionEvent::Title(title));
  }
}

struct Size {
  columns: usize,
  lines: usize,
}

impl Dimensions for Size {
  fn total_lines(&self) -> usize {
    self.lines
  }

  fn screen_lines(&self) -> usize {
    self.lines
  }

  fn columns(&self) -> usize {
    self.columns
  }
}

/// A running shell attached to a pseudo-terminal.
pub struct Session {
  #[cfg(unix)]
  exit_code: Arc<Mutex<Option<i32>>>,
  term: Arc<FairMutex<Term<Proxy>>>,
  sender: EventLoopSender,
  palette: Arc<Mutex<Palette>>,
  #[cfg(unix)]
  child_id: u32,
  #[cfg(unix)]
  foreground: Option<Arc<ForegroundActivity>>,
  #[cfg(target_os = "macos")]
  starts_through_login: bool,
  notify: Notify,
  #[cfg(unix)]
  directory_refresh: Arc<Mutex<DirectoryRefresh>>,
}

#[cfg(unix)]
#[derive(Default)]
struct DirectoryRefresh {
  running: bool,
  pending: bool,
  closed: bool,
}

impl Session {
  /// Starts the shell. `notify` is called from a background thread whenever
  /// something happens; it must be cheap and must not block.
  pub fn spawn(
    options: SessionOptions,
    columns: usize,
    lines: usize,
    notify: impl Fn(SessionEvent) + Send + Sync + 'static,
  ) -> std::io::Result<Self> {
    tty::setup_env();
    let (columns, lines) = (columns.max(2), lines.max(1));
    let palette = Arc::new(Mutex::new(options.palette.unwrap_or_else(Palette::dark)));
    #[cfg(target_os = "macos")]
    let starts_through_login = options.program.is_none();
    let pty_options = Options {
      shell: options.program.map(|(program, args)| Shell::new(program, args)),
      working_directory: options.working_directory,
      env: child_environment(),
      ..Options::default()
    };
    let pty = tty::new(&pty_options, window_size(columns, lines), 0)?;
    #[cfg(unix)]
    let child_id = pty.child().id();
    #[cfg(unix)]
    let foreground = pty.file().try_clone().ok().map(|tty| Arc::new(ForegroundActivity { tty, title_activity: Mutex::new(None) }));
    let proxy = Proxy {
      exit_code: Arc::default(),
      notify: Arc::new(notify),
      sender: Arc::new(OnceLock::new()),
      palette: palette.clone(),
      #[cfg(unix)]
      foreground: foreground.clone(),
    };
    let term = Arc::new(FairMutex::new(Term::new(Config::default(), &Size { columns, lines }, proxy.clone())));
    // Drain on exit: output the program wrote just before it exited still reaches the screen.
    let event_loop = EventLoop::new(term.clone(), proxy.clone(), pty, true, false)?;
    let sender = event_loop.channel();
    let _ = proxy.sender.set(sender.clone());
    event_loop.spawn();
    Ok(Self {
      #[cfg(unix)]
      exit_code: proxy.exit_code.clone(),
      term,
      sender,
      palette,
      #[cfg(unix)]
      child_id,
      #[cfg(unix)]
      foreground,
      #[cfg(target_os = "macos")]
      starts_through_login,
      notify: proxy.notify.clone(),
      #[cfg(unix)]
      directory_refresh: Arc::new(Mutex::new(DirectoryRefresh::default())),
    })
  }

  /// A background-safe check for a job that has taken control of the shell's terminal.
  /// An idle shell owns the foreground process group itself.
  pub fn foreground_job_probe(&self) -> Option<impl Fn() -> bool + Clone + Send + 'static> {
    #[cfg(unix)]
    {
      let foreground = self.foreground.clone()?;
      let child_id = self.child_id;
      #[cfg(target_os = "macos")]
      let login_shell_id = self.starts_through_login.then(|| Arc::new(OnceLock::new()));
      Some(move || {
        let Some(group_id) = foreground.group_id() else { return false };
        if group_id == child_id {
          return false;
        }
        #[cfg(target_os = "macos")]
        if let Some(shell_id) = &login_shell_id {
          let id = shell_id.get().copied().unwrap_or_else(|| macos_shell_id(child_id));
          if id == child_id {
            // The real shell may not exist yet while login is starting.
            return false;
          }
          let _ = shell_id.set(id);
          if group_id == id {
            return false;
          }
        }
        foreground.is_working(group_id)
      })
    }
    #[cfg(not(unix))]
    {
      None::<fn() -> bool>
    }
  }

  /// Sends bytes to the program, as if typed.
  #[cfg(unix)]
  pub fn terminate(&self) -> std::io::Result<()> {
    use rustix::process::{kill_process_group, Pid, Signal};

    let exit = self.exit_code.lock().unwrap_or_else(|error| error.into_inner());
    if exit.is_some() {
      return Ok(());
    }
    let pid = Pid::from_raw(self.child_id as i32).ok_or_else(|| std::io::Error::other("The terminal has no child process."))?;
    match kill_process_group(pid, Signal::KILL) {
      Err(rustix::io::Errno::SRCH) => Ok(()),
      result => result.map_err(Into::into),
    }
  }

  pub fn write(&self, bytes: impl Into<std::borrow::Cow<'static, [u8]>>) {
    let _ = self.sender.send(Msg::Input(bytes.into()));
    // Typing returns the view to the live output.
    self.term.lock().scroll_display(Scroll::Bottom);
  }

  /// Sends a report the program asked for, such as a mouse event, without leaving the history view.
  pub fn report(&self, bytes: impl Into<std::borrow::Cow<'static, [u8]>>) {
    let _ = self.sender.send(Msg::Input(bytes.into()));
  }

  /// Scrolls the history view by one screen; returns whether the viewport changed.
  pub fn scroll_page(&self, up: bool) -> bool {
    let mut term = self.term.lock();
    let previous = term.grid().display_offset();
    term.scroll_display(if up { Scroll::PageUp } else { Scroll::PageDown });
    term.grid().display_offset() != previous
  }

  /// Tells a program that asked for focus reports that the terminal gained or lost focus.
  pub fn report_focus(&self, focused: bool) {
    if self.term.lock().mode().contains(TermMode::FOCUS_IN_OUT) {
      let _ = self.sender.send(Msg::Input(if focused { &b"\x1b[I"[..] } else { &b"\x1b[O"[..] }.into()));
    }
  }

  /// Erases the scrollback history and returns to the live screen.
  pub fn clear_history(&self) {
    self.term.lock().grid_mut().clear_history();
  }

  /// Reads the shell's working directory right now. Blocks briefly, so it is meant for moments
  /// like quitting, not for every frame.
  pub fn current_directory(&self) -> Option<PathBuf> {
    #[cfg(unix)]
    {
      shell_directory(self.child_id).filter(|path| path.is_dir())
    }
    #[cfg(not(unix))]
    {
      None
    }
  }

  /// Refreshes the shell's working directory without blocking the interface.
  pub fn refresh_directory(&self) {
    #[cfg(unix)]
    {
      let mut state = self.directory_refresh.lock().unwrap_or_else(|error| error.into_inner());
      state.pending = true;
      if state.running {
        return;
      }
      state.running = true;
      drop(state);
      let id = self.child_id;
      let notify = self.notify.clone();
      let refresh = self.directory_refresh.clone();
      std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(300));
        {
          let mut state = refresh.lock().unwrap_or_else(|error| error.into_inner());
          if state.closed {
            return;
          }
          state.pending = false;
        }
        let directory = shell_directory(id);
        let mut state = refresh.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
          return;
        }
        if !state.pending {
          state.running = false;
          drop(state);
          if let Some(directory) = directory.filter(|path| path.is_dir()) {
            notify(SessionEvent::Directory(directory));
          }
          return;
        }
      });
    }
  }

  /// Resizes the terminal and the PTY. Returns whether the size changed.
  pub fn resize(&self, columns: usize, lines: usize) -> bool {
    let (columns, lines) = (columns.max(2), lines.max(1));
    let mut term = self.term.lock();
    if term.columns() == columns && term.screen_lines() == lines {
      return false;
    }
    if columns > term.columns() {
      unwrap_padding(&mut term);
    }
    term.resize(Size { columns, lines });
    drop(term);
    let _ = self.sender.send(Msg::Resize(window_size(columns, lines)));
    true
  }

  /// Scrolls the history view; positive lines go back in time.
  pub fn scroll(&self, lines: i32) -> bool {
    let mut term = self.term.lock();
    let previous = term.grid().display_offset();
    term.scroll_display(Scroll::Delta(lines));
    term.grid().display_offset() != previous
  }

  /// Finds `query` in the whole buffer, history included, oldest hit first.
  pub fn find(&self, query: &str, case_sensitive: bool) -> Vec<Match> {
    let query = fold_query(query, case_sensitive);
    let term = self.term.lock();
    let top = -(term.grid().history_size() as i32);
    (top..term.screen_lines() as i32)
      .flat_map(|line| find_in_row(&row_cells(&term, line), &query, case_sensitive).into_iter().map(move |(start, end)| Match { line, start, end }))
      .collect()
  }

  /// The hits on the visible screen, with `current` marked.
  pub fn highlights(&self, query: &str, case_sensitive: bool, current: Option<Match>) -> Vec<Highlight> {
    let query = fold_query(query, case_sensitive);
    let term = self.term.lock();
    let offset = term.grid().display_offset() as i32;
    (0..term.screen_lines())
      .flat_map(|row| {
        let line = row as i32 - offset;
        find_in_row(&row_cells(&term, line), &query, case_sensitive).into_iter().map(move |(start, end)| Highlight {
          line: row,
          start,
          end,
          current: current == Some(Match { line, start, end }),
        })
      })
      .collect()
  }

  /// Scrolls so `hit` is on screen, in the middle when it was out of view. Returns whether the viewport moved.
  pub fn reveal(&self, hit: Match) -> bool {
    let mut term = self.term.lock();
    let (offset, lines) = (term.grid().display_offset() as i32, term.screen_lines() as i32);
    if (0..lines).contains(&(hit.line + offset)) {
      return false;
    }
    let target = (lines / 2 - hit.line).clamp(0, term.grid().history_size() as i32);
    term.scroll_display(Scroll::Delta(target - offset));
    true
  }

  /// Moves to a history offset, returning whether the viewport changed.
  pub fn scroll_to(&self, offset: usize) -> bool {
    let mut term = self.term.lock();
    let previous = term.grid().display_offset();
    let target = offset.min(term.grid().history_size());
    let delta = (target as i64 - previous as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    term.scroll_display(Scroll::Delta(delta));
    term.grid().display_offset() != previous
  }

  pub fn modes(&self) -> Modes {
    let mode = *self.term.lock().mode();
    Modes {
      app_cursor: mode.contains(TermMode::APP_CURSOR),
      bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
      mouse_click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
      mouse_drag: mode.contains(TermMode::MOUSE_DRAG),
      mouse_motion: mode.contains(TermMode::MOUSE_MOTION),
      sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
      alt_screen: mode.contains(TermMode::ALT_SCREEN),
      alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
      focus_reporting: mode.contains(TermMode::FOCUS_IN_OUT),
    }
  }

  /// Updates the colors reported to applications querying the terminal.
  pub fn set_palette(&self, palette: Palette) {
    *self.palette.lock().unwrap_or_else(|error| error.into_inner()) = palette;
  }

  /// The current screen, colored with `palette`.
  pub fn snapshot(&self, palette: &Palette) -> Snapshot {
    let term = self.term.lock();
    let (columns, lines) = (term.columns(), term.screen_lines());
    let content = term.renderable_content();
    let offset = content.display_offset;
    let blank = Glyph { foreground: palette.foreground, ..Glyph::blank() };
    let mut rows = vec![vec![blank; columns]; lines];
    let mut hyperlinks = HashMap::new();
    let mut wrapped = vec![false; lines];
    for indexed in content.display_iter {
      let line = indexed.point.line.0 + offset as i32;
      let column = indexed.point.column.0;
      if line < 0 || line as usize >= lines || column >= columns {
        continue;
      }
      let cell = indexed.cell;
      if let Some(link) = cell.hyperlink() {
        hyperlinks.insert((line as usize, column), link.uri().to_owned());
      }
      if cell.flags.contains(Flags::WRAPLINE) {
        wrapped[line as usize] = true;
      }
      if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
        // The wide character before it already fills both cells.
        rows[line as usize][column] = Glyph { character: '\u{200b}', ..Glyph::blank() };
        continue;
      }
      let background = palette.resolve(cell.bg);
      let mut glyph = Glyph {
        character: if cell.flags.contains(Flags::HIDDEN) { ' ' } else { cell.c },
        foreground: palette.resolve(cell.fg),
        background: (background != palette.background).then_some(background),
        bold: cell.flags.contains(Flags::BOLD),
        italic: cell.flags.contains(Flags::ITALIC),
        underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
        strikeout: cell.flags.contains(Flags::STRIKEOUT),
      };
      if cell.flags.contains(Flags::INVERSE) {
        glyph.invert(palette);
      }
      if cell.flags.contains(Flags::DIM) {
        glyph.foreground = glyph.foreground.faint_on(glyph.background.unwrap_or(palette.background));
      }
      rows[line as usize][column] = glyph;
    }
    let cursor = alacritty_terminal::term::point_to_viewport(offset, content.cursor.point)
      .filter(|point| point.line < lines && point.column.0 < columns)
      .and_then(|point| {
        let shape = match content.cursor.shape {
          alacritty_terminal::vte::ansi::CursorShape::Hidden => return None,
          alacritty_terminal::vte::ansi::CursorShape::Underline => CursorShape::Underline,
          alacritty_terminal::vte::ansi::CursorShape::Beam => CursorShape::Beam,
          _ => CursorShape::Block,
        };
        Some(Cursor { line: point.line, column: point.column.0, shape })
      });
    if let Some(cursor) = cursor.filter(|cursor| cursor.shape == CursorShape::Block) {
      rows[cursor.line][cursor.column].invert(palette);
    }
    let lines = rows.iter().map(spans).collect();
    Snapshot { lines, cursor, columns, scrolled: offset, history_size: term.grid().history_size(), hyperlinks, wrapped }
  }
}

impl Drop for Session {
  fn drop(&mut self) {
    #[cfg(unix)]
    {
      self.directory_refresh.lock().unwrap_or_else(|error| error.into_inner()).closed = true;
    }
    let _ = self.sender.send(Msg::Shutdown);
  }
}

/// The environment every shell starts with.
fn child_environment() -> HashMap<String, String> {
  let mut environment = HashMap::from([
    // Do not advertise the terminal that launched the app to child programs.
    ("TERM_PROGRAM".to_owned(), ws_product::NAME.to_owned()),
    ("TERM_PROGRAM_VERSION".to_owned(), env!("CARGO_PKG_VERSION").to_owned()),
    // Remote hosts rarely know the `alacritty` terminfo entry that the emulator library prefers.
    ("TERM".to_owned(), "xterm-256color".to_owned()),
    ("COLORTERM".to_owned(), "truecolor".to_owned()),
  ]);
  // Apps started from Finder have no locale, which leaves shells unable to type or show non-ASCII text.
  let has_locale = ["LC_ALL", "LC_CTYPE", "LANG"].iter().any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
  if !has_locale {
    environment.insert("LANG".to_owned(), "en_US.UTF-8".to_owned());
  }
  environment
}

/// The characters of one row with the cells they cover; the filler cell after a wide character is left out.
fn row_cells(term: &Term<Proxy>, line: i32) -> Vec<RowCell> {
  let row = &term.grid()[Line(line)];
  (0..term.columns())
    .filter_map(|column| {
      let cell = &row[Column(column)];
      if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
        return None;
      }
      Some(RowCell { character: cell.c, column, width: if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 } })
    })
    .collect()
}

fn window_size(columns: usize, lines: usize) -> WindowSize {
  WindowSize { num_lines: lines as u16, num_cols: columns as u16, cell_width: 1, cell_height: 1 }
}

#[cfg(unix)]
fn shell_directory(id: u32) -> Option<PathBuf> {
  #[cfg(target_os = "macos")]
  {
    // The pty child is `/usr/bin/login`, which runs the shell as its own child. `login` is
    // privileged, so its directory cannot be read, and it never changes anyway.
    let id = macos_shell_id(id);
    std::process::Command::new("/usr/sbin/lsof")
      .args(["-a", "-p", &id.to_string(), "-d", "cwd", "-Fn"])
      .output()
      .ok()
      .and_then(|output| String::from_utf8(output.stdout).ok())
      .and_then(|text| text.lines().find_map(|line| line.strip_prefix('n').map(PathBuf::from)))
  }
  #[cfg(not(target_os = "macos"))]
  {
    std::fs::read_link(format!("/proc/{id}/cwd")).ok()
  }
}

/// The shell's process id: the first child when `id` is `login`, otherwise `id` itself.
#[cfg(target_os = "macos")]
fn macos_shell_id(id: u32) -> u32 {
  let run = |program: &str, args: &[&str]| {
    std::process::Command::new(program).args(args).output().ok().and_then(|output| String::from_utf8(output.stdout).ok())
  };
  let is_login = run("/bin/ps", &["-o", "comm=", "-p", &id.to_string()]).is_some_and(|name| name.trim().ends_with("/login"));
  if !is_login {
    return id;
  }
  run("/usr/bin/pgrep", &["-P", &id.to_string()]).and_then(|text| text.lines().next().and_then(|line| line.trim().parse().ok())).unwrap_or(id)
}

/// Cuts the soft wrap after rows that end in blank padding.
///
/// Shells pad a row with spaces to push a stray prompt marker aside (zsh does this after `^C`), which
/// can spill a few blanks onto the next row. Joining those rows when the pane grows would leave the
/// next prompt in the middle of the line, so they stay separate rows.
fn unwrap_padding<T>(term: &mut Term<T>) {
  let last = Column(term.columns() - 1);
  let top = -(term.grid().history_size() as i32);
  for line in top..term.screen_lines() as i32 {
    let row = &mut term.grid_mut()[Line(line)];
    let padded = row[last].flags.contains(Flags::WRAPLINE) && row[last].c == ' ' && row[Column(last.0.saturating_sub(1))].c == ' ';
    if padded {
      row[last].flags.remove(Flags::WRAPLINE);
    }
  }
}
