//! A diff as the Changes and History views show it: the Kit `Diff` with whole-line red and green backgrounds from the
//! theme's palette (`Colors::diff_added`, `Colors::diff_removed`), instead of a tint of the status colors.

use crate::{editor::language_for, minimap::line_styles, theme::colors};
use gpui_kit::{
  base::input::Rope,
  component::{
    button::{Button, ButtonVariants as _},
    diff::{Diff, DiffDecoration, DiffFile, DiffHoverHighlight, DiffHunkSeparator, DiffInlineUnit, DiffLineRange, DiffMode, DiffSide, DiffState},
    h_flex,
    highlighter::{HighlightTheme, SyntaxHighlighter},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _, Theme,
  },
  *,
};
use std::{
  cell::{Cell, RefCell},
  collections::{hash_map::DefaultHasher, HashMap},
  hash::{Hash, Hasher},
  ops::Range,
  path::Path,
  rc::Rc,
  sync::{Arc, Mutex, MutexGuard},
  time::Duration,
};
use ws_index::highlight::{Highlights, Language};

/// Parses a patch using the same file-to-language mapping as the editor.
pub(crate) fn parse_patch(patch: &str) -> Result<Vec<DiffFile>, String> {
  let files = DiffFile::parse(patch).map_err(|error| error.to_string())?;
  Ok(files.into_iter().map(with_editor_language).collect())
}

fn with_editor_language(file: DiffFile) -> DiffFile {
  let language = language_for(Path::new(file.path().as_ref()));
  if language == "text" {
    file
  } else {
    file.with_language(language)
  }
}

type Styles = Arc<Vec<(Range<usize>, HighlightStyle)>>;

/// The colors of hunks that were already highlighted, so that showing a diff again (or showing it after [`warm`]) does not parse it again.
/// Keyed by the language and the text of the hunk: a file that changed has other hunks, and those are the only ones parsed anew.
struct Cache {
  theme: Arc<HighlightTheme>,
  styles: HashMap<u64, Styles>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The most hunks kept; the cache starts again past this.
const MAX_CACHED: usize = 2048;

thread_local! {
  /// One parser per language and thread: loading a grammar costs more than parsing a hunk with it.
  static PARSERS: RefCell<HashMap<String, SyntaxHighlighter>> = RefCell::new(HashMap::new());
}

fn cache() -> MutexGuard<'static, Option<Cache>> {
  CACHE.lock().unwrap_or_else(|error| error.into_inner())
}

fn styles_of(theme: &Arc<HighlightTheme>, language: &str, text: &str) -> Styles {
  let mut hasher = DefaultHasher::new();
  (language, text).hash(&mut hasher);
  let key = hasher.finish();
  if let Some(cache) = cache().as_ref().filter(|cache| Arc::ptr_eq(&cache.theme, theme)) {
    if let Some(styles) = cache.styles.get(&key) {
      return styles.clone();
    }
  }
  let range = 0..text.len();
  let styles = Arc::new(if let Some(known) = Language::from_name(language) {
    line_styles(known, &Highlights::compute(known, text), &range, &**theme)
  } else {
    PARSERS.with(|parsers| {
      let mut parsers = parsers.borrow_mut();
      let parser = parsers.entry(language.to_string()).or_insert_with(|| SyntaxHighlighter::new(language));
      parser.update(None, &Rope::from(text), None);
      parser.styles(&range, &**theme)
    })
  });
  let mut guard = cache();
  let cache = match &mut *guard {
    Some(cache) if Arc::ptr_eq(&cache.theme, theme) && cache.styles.len() < MAX_CACHED => cache,
    slot => slot.insert(Cache { theme: theme.clone(), styles: HashMap::new() }),
  };
  cache.styles.insert(key, styles.clone());
  styles
}

fn diff_highlighter(cx: &App) -> impl Fn(&str, &str) -> Vec<(Range<usize>, HighlightStyle)> + Send + Sync + 'static {
  let theme = cx.theme().highlight_theme.clone();
  move |language, text| styles_of(&theme, language, text).to_vec()
}

/// The theme the colors of diffs come from; take it on the interface thread and hand it to [`warm`].
pub(crate) fn highlight_theme(cx: &App) -> Arc<HighlightTheme> {
  cx.theme().highlight_theme.clone()
}

/// Colors every hunk of `patch` ahead of time, on the thread that calls it (the background one that read the patch), so that the first
/// paint of the diff finds its colors ready instead of showing plain text until they arrive. The hunks are cut the way the Kit cuts them:
/// one text per side, with its line endings, for each hunk.
pub(crate) fn warm(theme: &Arc<HighlightTheme>, patch: &str) {
  let mut language: Option<&'static str> = None;
  // The text of the original and modified side of the hunk being read, and which sides the last line was on.
  let mut sides = [String::new(), String::new()];
  let mut last = [false, false];
  let mut in_hunk = false;
  let flush = |language: Option<&str>, sides: &mut [String; 2]| {
    for text in sides.iter_mut() {
      if let (Some(language), false) = (language, text.is_empty()) {
        styles_of(theme, language, text);
      }
      text.clear();
    }
  };
  for line in patch.split('\n') {
    if let Some(rest) = line.strip_prefix("diff --git ") {
      flush(language, &mut sides);
      in_hunk = false;
      language = rest.rsplit_once(" b/").map(|(_, path)| language_for(Path::new(path))).filter(|language| *language != "text");
    } else if line.starts_with("@@ ") {
      flush(language, &mut sides);
      in_hunk = true;
    } else if in_hunk {
      match line.as_bytes().first() {
        Some(b' ') => {
          sides[0].push_str(&line[1..]);
          sides[0].push('\n');
          sides[1].push_str(&line[1..]);
          sides[1].push('\n');
          last = [true, true];
        }
        Some(b'-') => {
          sides[0].push_str(&line[1..]);
          sides[0].push('\n');
          last = [true, false];
        }
        Some(b'+') => {
          sides[1].push_str(&line[1..]);
          sides[1].push('\n');
          last = [false, true];
        }
        // "\ No newline at end of file": the line before it has no line ending.
        Some(b'\\') => {
          for (text, on) in sides.iter_mut().zip(last) {
            if on && text.ends_with('\n') {
              text.pop();
            }
          }
        }
        _ => {}
      }
    }
  }
  flush(language, &mut sides);
}

/// A run of lines the patch adds or removes.
struct ChangedRange {
  path: SharedString,
  side: DiffSide,
  start: usize,
  end: usize,
}

/// The longest a new diff is held back for its colors; past this it is shown as it is.
const COLORS_WAIT: Duration = Duration::from_millis(500);

/// How soon after the Kit reports a change of size the window is drawn again.
const REDRAW_EVERY: Duration = Duration::from_millis(16);

/// Whether long lines wrap in every diff (on) or scroll sideways (off). One setting for all the places that show a diff.
struct WrapLines(bool);

impl Global for WrapLines {}

fn wraps(cx: &App) -> bool {
  cx.try_global::<WrapLines>().is_none_or(|wrap| wrap.0)
}

/// A parsed patch with the lines it changes.
pub(crate) struct ShownDiff {
  state: Entity<DiffState>,
  changed: Vec<ChangedRange>,
  /// The digits of the longest line number, which sets the width of the line-number columns.
  digits: usize,
  /// Files whose colors (syntax, changed words) the Kit is still preparing; the diff is not shown before they are in, so that it
  /// does not paint as plain text first and change under the eyes.
  waiting: Rc<Cell<usize>>,
  _theme_subscription: Subscription,
  _prepared: Subscription,
  _give_up: Task<()>,
}

impl ShownDiff {
  /// Shows `files`, which were parsed from `patch`. The owner is redrawn as the colors come in; until then [`Self::is_ready`] is false.
  pub fn new<T: 'static>(files: Vec<DiffFile>, patch: &str, cx: &mut Context<T>) -> Self {
    let changed = changed_ranges(patch, &files);
    let digits = widest_line_digits(patch);
    let waiting = Rc::new(Cell::new(files.len()));
    let state = cx.new(|cx| DiffState::new(files, cx).with_mode(DiffMode::Unified));
    let theme_subscription = state.update(cx, |state, cx| {
      state.set_syntax_highlighter(diff_highlighter(cx), cx);
      cx.observe_global::<Theme>(|state, cx| state.set_syntax_highlighter(diff_highlighter(cx), cx))
    });
    // The Kit tells its observers once for each file it has prepared (and when other things change, which only makes this early).
    // It also tells them when the diff was measured at another size: the owner is drawn again then, so that the lines take the width
    // they have now and not the one they had when the diff was last drawn.
    // The Kit measures during the paint: a change it notices there (the window or a pane got another width) is not drawn by itself
    // until the next input, so a frame is asked for once the paint is over (at most one for each `REDRAW_EVERY`).
    let redraw_pending = Rc::new(Cell::new(false));
    let prepared = cx.observe(&state, {
      let waiting = waiting.clone();
      move |_, _, cx| {
        waiting.set(waiting.get().saturating_sub(1));
        cx.notify();
        if !redraw_pending.replace(true) {
          let redraw_pending = redraw_pending.clone();
          cx.spawn(async move |_, cx| {
            cx.background_executor().timer(REDRAW_EVERY).await;
            redraw_pending.set(false);
            cx.update(|cx| cx.refresh_windows());
          })
          .detach();
        }
      }
    });
    let give_up = cx.spawn({
      let waiting = waiting.clone();
      async move |this, cx| {
        cx.background_executor().timer(COLORS_WAIT).await;
        if waiting.replace(0) > 0 {
          let _ = this.update(cx, |_, cx| cx.notify());
        }
      }
    });
    Self { state, changed, digits, waiting, _theme_subscription: theme_subscription, _prepared: prepared, _give_up: give_up }
  }

  /// Whether the colors are in, so that the diff can be shown. A diff whose files were replaced ([`Self::set_files`]) stays shown.
  pub fn is_ready(&self) -> bool {
    self.waiting.get() == 0
  }

  pub fn set_files(&mut self, files: Vec<DiffFile>, patch: &str, cx: &mut App) {
    self.changed = changed_ranges(patch, &files);
    self.digits = widest_line_digits(patch);
    self.state.update(cx, |state, cx| state.set_files(files, cx));
  }

  pub fn state(&self) -> &Entity<DiffState> {
    &self.state
  }

  pub fn with_controls(&self, diff: Diff, cx: &App) -> impl IntoElement {
    v_flex()
      .size_full()
      .min_h_0()
      .child(h_flex().px_2().py_1().justify_end().border_b_1().border_color(cx.theme().border).child(diff_controls(&self.state)))
      .child(div().flex_1().min_h_0().min_w_0().overflow_hidden().relative().child(diff).child(self.number_rule(cx)))
  }

  /// Two thin rules, one between the old and the new line numbers and one after the new ones, to be laid over the diff (a `relative` parent of the same size).
  /// The Kit has no such rule, so it is placed from the Kit's own sizes: both are in rems, the columns being as wide as the digits
  /// of the longest line number plus a rem, after half a rem of padding. It stands still while the diff scrolls sideways,
  /// which a diff that wraps long lines (the default) never does.
  pub fn number_rule(&self, cx: &App) -> impl IntoElement {
    let digits = self.digits as f32;
    let column = f32::from(cx.theme().mono_font_size) / 16. * 0.7 * digits + 1.;
    let rule = |columns: f32| div().absolute().top_0().bottom_0().left(rems(0.5 + column * columns)).w_px().bg(cx.theme().border);
    // One after the old numbers, one after the new ones.
    div().absolute().inset_0().child(rule(1.)).child(rule(2.))
  }

  /// The Kit diff, to be given the sizes and options of the place that shows it.
  pub fn view(&self, cx: &App) -> Diff {
    let palette = colors(cx);
    let decorations = self.changed.iter().map(|range| {
      let background = if range.side == DiffSide::Original { palette.diff_removed } else { palette.diff_added };
      DiffDecoration::new(DiffLineRange::new(range.path.clone(), range.side, range.start, range.end)).with_background(background)
    });
    Diff::new(&self.state)
      .soft_wrap(wraps(cx))
      .change_background(false)
      .hover_highlight(DiffHoverHighlight::Both)
      // The `@@ -a,b +c,d @@ scope` line above each hunk, as GitHub Desktop shows it.
      .hunk_separator(DiffHunkSeparator::Metadata)
      .decorations(decorations)
  }
}

pub(crate) fn diff_controls(state: &Entity<DiffState>) -> impl IntoElement {
  let previous = state.clone();
  let next = state.clone();
  let options = state.clone();
  h_flex()
    .gap_1()
    .child(
      Button::new("diff-previous")
        .ghost()
        .xsmall()
        .icon(IconName::ChevronUp)
        .tooltip("Previous change")
        .accessibility_label("Previous change")
        .on_click(move |_, _, cx| {
          previous.update(cx, DiffState::previous_change);
        }),
    )
    .child(Button::new("diff-next").ghost().xsmall().icon(IconName::ChevronDown).tooltip("Next change").accessibility_label("Next change").on_click(
      move |_, _, cx| {
        next.update(cx, DiffState::next_change);
      },
    ))
    .child(
      Button::new("diff-options")
        .ghost()
        .xsmall()
        .icon(Icon::empty().path("icons/settings-2.svg"))
        .tooltip("Diff options")
        .accessibility_label("Diff options")
        .dropdown_menu(move |menu, _, cx| {
          let state = options.read(cx);
          let mode = state.mode();
          let emphasis = state.inline_unit();
          let mut menu = menu;
          let wrap = wraps(cx);
          for (label, choice) in [("Unified", DiffMode::Unified), ("Side by side", DiffMode::Split)] {
            let target = options.clone();
            menu = menu.item(PopupMenuItem::new(label).checked(mode == choice).on_click(move |_, _, cx| {
              target.update(cx, |state, cx| state.set_mode(choice, cx));
            }));
          }
          menu = menu.separator().item(PopupMenuItem::new("Wrap long lines").checked(wrap).on_click(move |_, _, cx| {
            cx.set_global(WrapLines(!wrap));
            cx.refresh_windows();
          }));
          menu = menu.separator();
          for (label, choice) in [
            ("Highlight changed words", Some(DiffInlineUnit::Word)),
            ("Highlight changed characters", Some(DiffInlineUnit::Character)),
            ("Highlight whole lines", None),
          ] {
            let target = options.clone();
            menu = menu.item(PopupMenuItem::new(label).checked(emphasis == choice).on_click(move |_, _, cx| {
              target.update(cx, |state, cx| state.set_inline_unit(choice, cx));
            }));
          }
          let action = |label, command: fn(&mut DiffState, &mut Context<DiffState>)| {
            let target = options.clone();
            PopupMenuItem::new(label).on_click(move |_, _, cx| {
              target.update(cx, command);
            })
          };
          let copy = options.clone();
          menu
            .separator()
            .item(action("Expand unchanged lines", DiffState::expand_unchanged))
            .item(action("Collapse unchanged lines", DiffState::collapse_unchanged))
            .separator()
            .item(PopupMenuItem::new("Copy selected lines").disabled(state.selected_lines().is_none()).on_click(move |_, _, cx| {
              let text = copy.read(cx).selected_text(cx);
              cx.write_to_clipboard(ClipboardItem::new_string(text));
            }))
        }),
    )
}

/// Visits source lines in patch order. Context is represented by its modified-side line number.
pub(crate) fn visit_patch_lines(patch: &str, files: &[DiffFile], mut visit: impl FnMut(&DiffFile, DiffSide, usize, bool, usize)) {
  let mut files = files.iter();
  let mut file = files.next();
  let mut saw_file_header = false;
  let mut hunk = None;
  for text in patch.lines() {
    if text.starts_with("diff --git ") {
      if saw_file_header {
        file = files.next();
      }
      saw_file_header = true;
      hunk = None;
    } else if text.starts_with("@@ ") {
      hunk = hunk_start(text).map(|(old, new)| (old, new, 0));
    } else if let (Some(file), Some((old, new, row))) = (file, &mut hunk) {
      let (side, line, changed) = match text.as_bytes().first() {
        Some(b'-') => {
          let line = *old;
          *old += 1;
          (DiffSide::Original, line, true)
        }
        Some(b'+') => {
          let line = *new;
          *new += 1;
          (DiffSide::Modified, line, true)
        }
        Some(b' ') => {
          let line = *new;
          *old += 1;
          *new += 1;
          (DiffSide::Modified, line, false)
        }
        _ => continue,
      };
      visit(file, side, line, changed, *row);
      *row += 1;
    }
  }
}

fn hunk_start(header: &str) -> Option<(usize, usize)> {
  let mut ranges = header.split_whitespace().skip(1);
  let start = |range: &str, prefix| range.strip_prefix(prefix)?.split(',').next()?.parse::<usize>().ok();
  Some((start(ranges.next()?, '-')?, start(ranges.next()?, '+')?))
}

/// The added and removed lines, grouped into runs by file and side.
fn changed_ranges(patch: &str, files: &[DiffFile]) -> Vec<ChangedRange> {
  let mut ranges: Vec<ChangedRange> = Vec::new();
  visit_patch_lines(patch, files, |file, side, line, changed, _| {
    if !changed {
      return;
    }
    let path = file.path();
    match ranges.last_mut() {
      Some(last) if &last.path == path && last.side == side && last.end + 1 == line => last.end = line,
      _ => ranges.push(ChangedRange { path: path.clone(), side, start: line, end: line }),
    }
  });
  ranges
}

/// The digits of the longest line number of a patch, read from its hunk headers (`@@ -start,count +start,count @@`).
fn widest_line_digits(patch: &str) -> usize {
  let end = |range: &str| {
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    start.parse::<usize>().unwrap_or(0) + count.parse::<usize>().unwrap_or(1)
  };
  let widest = patch
    .lines()
    .filter_map(|line| line.strip_prefix("@@ -"))
    .filter_map(|header| {
      let (old, rest) = header.split_once(" +")?;
      let new = rest.split_once(' ')?.0;
      Some(end(old).max(end(new)))
    })
    .max()
    .unwrap_or(1);
  widest.saturating_sub(1).max(1).to_string().len()
}
