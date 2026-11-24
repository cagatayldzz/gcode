//! A diff as the Changes and History views show it: the Kit `Diff` with whole-line red and green backgrounds from the
//! theme's palette (`Colors::diff_added`, `Colors::diff_removed`), instead of a tint of the status colors.

use crate::{editor::language_for, minimap::line_styles, theme::colors};
use gpui_kit::{
  base::input::Rope,
  component::{
    button::{Button, ButtonVariants as _},
    diff::{Diff, DiffDecoration, DiffFile, DiffHoverHighlight, DiffInlineUnit, DiffLineRange, DiffMode, DiffSide, DiffState},
    h_flex,
    highlighter::SyntaxHighlighter,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _, Theme,
  },
  *,
};
use std::{ops::Range, path::Path};
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

fn diff_highlighter(cx: &App) -> impl Fn(&str, &str) -> Vec<(Range<usize>, HighlightStyle)> + Send + Sync + 'static {
  let theme = cx.theme().highlight_theme.clone();
  move |language, text| {
    let range = 0..text.len();
    if let Some(language) = Language::from_name(language) {
      return line_styles(language, &Highlights::compute(language, text), &range, &*theme);
    }
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, &Rope::from(text), None);
    highlighter.styles(&range, &*theme)
  }
}

/// A run of lines the patch adds or removes.
struct ChangedRange {
  path: SharedString,
  side: DiffSide,
  start: usize,
  end: usize,
}

/// A parsed patch with the lines it changes.
pub(crate) struct ShownDiff {
  state: Entity<DiffState>,
  changed: Vec<ChangedRange>,
  _theme_subscription: Subscription,
}

impl ShownDiff {
  /// Shows `files`, which were parsed from `patch`.
  pub fn new(files: Vec<DiffFile>, patch: &str, cx: &mut App) -> Self {
    let changed = changed_ranges(patch, &files);
    let state = cx.new(|cx| DiffState::new(files, cx).with_mode(DiffMode::Unified));
    let theme_subscription = state.update(cx, |state, cx| {
      state.set_syntax_highlighter(diff_highlighter(cx), cx);
      cx.observe_global::<Theme>(|state, cx| state.set_syntax_highlighter(diff_highlighter(cx), cx))
    });
    Self { state, changed, _theme_subscription: theme_subscription }
  }

  pub fn set_files(&mut self, files: Vec<DiffFile>, patch: &str, cx: &mut App) {
    self.changed = changed_ranges(patch, &files);
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
      .child(div().flex_1().min_h_0().min_w_0().overflow_hidden().child(diff))
  }

  /// The Kit diff, to be given the sizes and options of the place that shows it.
  pub fn view(&self, cx: &App) -> Diff {
    let palette = colors(cx);
    let decorations = self.changed.iter().map(|range| {
      let background = if range.side == DiffSide::Original { palette.diff_removed } else { palette.diff_added };
      DiffDecoration::new(DiffLineRange::new(range.path.clone(), range.side, range.start, range.end)).with_background(background)
    });
    Diff::new(&self.state).change_background(false).hover_highlight(DiffHoverHighlight::Both).decorations(decorations)
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
          for (label, choice) in [("Unified", DiffMode::Unified), ("Side by side", DiffMode::Split)] {
            let target = options.clone();
            menu = menu.item(PopupMenuItem::new(label).checked(mode == choice).on_click(move |_, _, cx| {
              target.update(cx, |state, cx| state.set_mode(choice, cx));
            }));
          }
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
