//! Project search actions, replacement workflow, and sidebar rendering.

use super::{SearchCancellation, SidebarTab, Workspace};
use crate::{
  project_search,
  theme::{colors, FONT_SIZE_SM},
  OpenSearchResults, SearchProject,
};
use gpui_kit::{
  base::Selectable as _,
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    label::Label,
    notification::Notification,
    scroll::{Scrollbar, ScrollbarAxis},
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
  },
  prelude::FluentBuilder as _,
  *,
};
use std::{
  ops::Range,
  path::{Path, PathBuf},
  sync::{atomic::AtomicBool, Arc},
  time::Duration,
};

enum SearchListRow {
  File { first: usize, count: usize },
  Match(usize),
}

impl Workspace {
  pub fn search_project(&mut self, _: &SearchProject, window: &mut Window, cx: &mut Context<Self>) {
    self.sidebar_tab = SidebarTab::Search;
    if self.full_area_open() {
      self.hide_terminal(window, cx);
    }
    if !self.search_open {
      self.search_state = cx.new(|cx| InputState::new(window, cx).placeholder("Search project files"));
      self.search_include = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. src/**, *.rs"));
      self.search_exclude = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. tests/**"));
      self.search_replace = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
      self.search_subscriptions = [&self.search_state, &self.search_include, &self.search_exclude]
        .into_iter()
        .map(|input| {
          cx.subscribe_in(input, window, |this, input, event, window, cx| match event {
            InputEvent::Change => this.run_project_search(window, cx),
            InputEvent::PressEnter { .. } if *input == this.search_state => {
              if let Some(found) = this.search_results.iter().find(|found| !this.search_collapsed.contains(&found.path)).cloned() {
                this.open_search_result(&found, window, cx);
              }
            }
            _ => {}
          })
        })
        .collect();
      self.search_open = true;
    }
    if self.projects.active.is_some() {
      self.search_state.read(cx).focus_handle(cx).focus(window, cx);
    }
    cx.notify();
  }

  fn run_project_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.search_epoch += 1;
    let epoch = self.search_epoch;
    self.search_task = None;
    self.search_cancellation = None;
    self.search_results.clear();
    self.search_scroll.scroll_to_item(0, ScrollStrategy::Top);
    self.search_collapsed.clear();
    self.search_occurrences = 0;
    self.search_files = 0;
    self.search_error = None;
    self.search_options.query = self.search_state.read(cx).value().to_string();
    self.search_options.include = self.search_include.read(cx).value().to_string();
    self.search_options.exclude = self.search_exclude.read(cx).value().to_string();
    let Some(root) = self.projects.active.clone().filter(|_| !self.search_options.query.is_empty()) else {
      cx.notify();
      return;
    };
    let options = self.search_options.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    self.search_cancellation = Some(SearchCancellation(cancelled.clone()));
    self.search_task = Some(cx.spawn_in(window, async move |this, cx| {
      cx.background_executor().timer(Duration::from_millis(200)).await;
      let work = cx.background_executor().spawn(async move { project_search::search(&root, &options, &cancelled) });
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.search_epoch != epoch {
          return;
        }
        this.search_cancellation = None;
        match result {
          Ok(results) => {
            this.search_occurrences = results.occurrences;
            this.search_files = results.files;
            this.search_results = results.matches;
          }
          Err(message) => this.search_error = Some(message.into()),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  pub(super) fn open_search_results(&mut self, _: &OpenSearchResults, window: &mut Window, cx: &mut Context<Self>) {
    if self.sidebar_tab != SidebarTab::Search || self.search_results.is_empty() {
      return;
    }
    use std::fmt::Write as _;
    let mut text = format!("{}\n", self.search_status());
    let root = self.projects.active.as_deref().unwrap_or_else(|| Path::new(""));
    for group in self.search_results.chunk_by(|left, right| left.path == right.path) {
      let path = &group[0].path;
      let path = path.strip_prefix(root).unwrap_or(path);
      let _ = writeln!(text, "\n{}", path.display());
      for found in group {
        let _ = writeln!(text, "  {}:{}  {}", found.line + 1, found.column + 1, found.preview);
      }
    }
    self.editor_panel.update(cx, |editor, cx| editor.show_search_report(&self.search_options.query, text, window, cx));
  }

  fn prepare_search_replacement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone().filter(|_| self.search_occurrences > 0 && self.replacement_task.is_none()) else { return };
    let options = self.search_options.clone();
    let replacement = self.search_replace.read(cx).value().to_string();
    let preserve_case = self.search_preserve_case;
    let epoch = self.search_epoch;
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { project_search::replacement_plan(&root, &options, &replacement, preserve_case) }
    });
    self.replacement_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.replacement_task = None;
        cx.notify();
        if this.projects.active.as_ref() != Some(&root) || this.search_epoch != epoch {
          return;
        }
        let edits = match result {
          Ok(edits) if !edits.is_empty() => Arc::new(edits),
          Ok(_) => {
            window.push_notification(Notification::info("The replacement would leave the files unchanged."), cx);
            return;
          }
          Err(message) => {
            window.push_notification(Notification::error(message), cx);
            return;
          }
        };
        let title: SharedString = format!("Replace matches in {} file{}?", edits.len(), if edits.len() == 1 { "" } else { "s" }).into();
        let workspace = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
          let (workspace, root, edits) = (workspace.clone(), root.clone(), edits.clone());
          alert.title(title.clone()).description("This writes the replacement to the project files on disk.").confirm().ok_text("Replace all").on_ok(
            move |_, window, cx| {
              let _ = workspace.update(cx, |this, cx| this.apply_search_replacement(root.clone(), edits.clone(), window, cx));
              true
            },
          )
        });
      });
    }));
    cx.notify();
  }

  fn apply_search_replacement(&mut self, root: PathBuf, edits: Arc<Vec<project_search::Replacement>>, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&root) || self.replacement_task.is_some() {
      return;
    }
    if edits.iter().any(|edit| self.editor_panel.read(cx).is_dirty(&edit.path)) {
      window.push_notification(Notification::error("Save the affected editor tabs and search again before replacing."), cx);
      return;
    }
    self.editor_panel.update(cx, |editor, cx| {
      editor.lock_replacement_files(edits.iter().map(|edit| edit.path.clone()), cx);
    });
    let work = cx.background_executor().spawn({
      let root = root.clone();
      let edits = edits.clone();
      async move { edits.iter().map(|edit| project_search::apply_replacement(&root, edit)).collect::<Vec<_>>() }
    });
    self.replacement_task = Some(cx.spawn_in(window, async move |this, cx| {
      let results = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.replacement_task = None;
        let mut written = 0;
        let mut first_error = None;
        this.editor_panel.update(cx, |editor, cx| {
          editor.lock_replacement_files([], cx);
          for (edit, result) in edits.iter().zip(results) {
            match result {
              Ok(()) => {
                written += 1;
                editor.update_replaced_text(&edit.path, &edit.updated, window, cx);
              }
              Err(error) => {
                first_error.get_or_insert_with(|| format!("{}: {error}", edit.path.display()));
              }
            }
          }
        });
        if let Some(error) = first_error {
          window.push_notification(Notification::error(format!("Updated {written} of {} files. {error}", edits.len())), cx);
        } else {
          window.push_notification(Notification::info(format!("Updated {written} file{}.", if written == 1 { "" } else { "s" })), cx);
        }
        if this.projects.active.as_ref() == Some(&root) && this.search_open {
          this.run_project_search(window, cx);
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  pub(super) fn close_project_search(&mut self, cx: &mut Context<Self>) {
    self.search_open = false;
    self.search_subscriptions.clear();
    self.search_options = project_search::Options::default();
    self.search_occurrences = 0;
    self.search_files = 0;
    self.search_error = None;
    if self.sidebar_tab == SidebarTab::Search {
      self.sidebar_tab = SidebarTab::Explorer;
    }
    self.search_task = None;
    self.search_cancellation = None;
    self.search_results.clear();
    self.search_collapsed.clear();
    self.search_epoch += 1;
    cx.notify();
  }

  fn open_search_result(&mut self, result: &project_search::Match, window: &mut Window, cx: &mut Context<Self>) {
    self.open_file(result.path.clone(), window, cx);
    self.editor_panel.update(cx, |editor, cx| editor.reveal_position(&result.path, result.line, result.column, window, cx));
  }

  fn search_status(&self) -> String {
    let count = self.search_occurrences;
    let files = self.search_files;
    if self.projects.active.is_none() {
      "Open a project to search its files.".into()
    } else if self.search_cancellation.is_some() {
      "Searching…".into()
    } else if self.search_options.query.is_empty() {
      String::new()
    } else if count == 0 {
      "No matches found".into()
    } else {
      let result_word = if count == 1 { "match" } else { "matches" };
      let file_word = if files == 1 { "file" } else { "files" };
      let limit = if self.search_results.len() < count { format!(" · showing {}", self.search_results.len()) } else { String::new() };
      format!("{count} {result_word} in {files} {file_word}{limit}")
    }
  }

  pub(super) fn render_project_search(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let loading = self.search_cancellation.is_some();
    let status = self.search_error.as_ref().map_or_else(|| self.search_status(), ToString::to_string);
    let option = |label, title, selected| Button::new(title).ghost().xsmall().label(label).tooltip(title).selected(selected);
    let focus = self.focus_handle.clone();
    let flags = h_flex()
      .gap_0p5()
      .child(option("Aa", "Match case", self.search_options.case_sensitive).on_click(cx.listener(|this, _, window, cx| {
        this.search_options.case_sensitive = !this.search_options.case_sensitive;
        this.run_project_search(window, cx);
      })))
      .child(option("ab", "Match whole word", self.search_options.whole_word).on_click(cx.listener(|this, _, window, cx| {
        this.search_options.whole_word = !this.search_options.whole_word;
        this.run_project_search(window, cx);
      })))
      .child(option(".*", "Use regular expression", self.search_options.regex).on_click(cx.listener(|this, _, window, cx| {
        this.search_options.regex = !this.search_options.regex;
        this.run_project_search(window, cx);
      })));
    let action = |id, icon, tooltip| Button::new(id).ghost().xsmall().icon(Icon::empty().path(icon)).tooltip(tooltip);
    v_flex()
      .id("project-search")
      .key_context("ProjectSearch")
      .flex_1()
      .min_h_0()
      .gap_2()
      .child(
        h_flex().justify_between().gap_2().child(Label::new("Search").font_weight(FontWeight::SEMIBOLD)).child(
          h_flex()
            .gap_1()
            .child(
              action("search-refresh", "icons/refresh-cw.svg", "Refresh results")
                .on_click(cx.listener(|this, _, window, cx| this.run_project_search(window, cx))),
            )
            .child(action("search-collapse", "icons/list-chevrons-down-up.svg", "Collapse or expand results").on_click(cx.listener(
              |this, _, _, cx| {
                if this.search_collapsed.is_empty() {
                  this.search_collapsed.extend(this.search_results.iter().map(|found| found.path.clone()));
                } else {
                  this.search_collapsed.clear();
                }
                cx.notify();
              },
            ))),
        ),
      )
      .child(
        h_flex()
          .gap_1()
          .child(
            Button::new("toggle-search-replace")
              .ghost()
              .xsmall()
              .icon(if self.search_show_replace { IconName::ChevronDown } else { IconName::ChevronRight })
              .accessibility_label("Toggle replace")
              .tooltip("Toggle replace")
              .on_click(cx.listener(|this, _, _, cx| {
                this.search_show_replace = !this.search_show_replace;
                cx.notify();
              })),
          )
          .child(crate::components::soft_input(&self.search_state, cx).small().w_full().disabled(self.projects.active.is_none()).suffix(flags)),
      )
      .when(self.search_show_replace, |panel| {
        panel.child(
          h_flex()
            .gap_1()
            .child(crate::components::soft_input(&self.search_replace, cx).small().w_full().disabled(self.projects.active.is_none()).suffix(
              option("AB", "Preserve case", self.search_preserve_case).on_click(cx.listener(|this, _, _, cx| {
                this.search_preserve_case = !this.search_preserve_case;
                cx.notify();
              })),
            ))
            .child(
              action("search-replace-all", "icons/replace-all.svg", "Replace all matches")
                .accessibility_label("Replace all matches")
                .disabled(self.search_occurrences == 0 || loading || self.replacement_task.is_some())
                .loading(self.replacement_task.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.prepare_search_replacement(window, cx))),
            ),
        )
      })
      .child(
        h_flex().justify_end().child(
          action("search-filters", "icons/list-filter.svg", "Toggle file filters")
            .selected(self.search_show_filters)
            .accessibility_label("Toggle file filters")
            .on_click(cx.listener(|this, _, _, cx| {
              this.search_show_filters = !this.search_show_filters;
              cx.notify();
            })),
        ),
      )
      .when(self.search_show_filters, |panel| {
        panel.child(
          v_flex()
            .gap_2()
            .child(
              v_flex().gap_1().child(Label::new("Files to include").text_xs()).child(crate::components::soft_input(&self.search_include, cx).small()),
            )
            .child(
              v_flex().gap_1().child(Label::new("Files to exclude").text_xs()).child(crate::components::soft_input(&self.search_exclude, cx).small()),
            ),
        )
      })
      .when(!status.is_empty(), |panel| {
        panel.child(
          h_flex()
            .gap_2()
            .when(loading, |row| row.child(Spinner::new().small()))
            .flex_wrap()
            .child(Label::new(status).text_size(FONT_SIZE_SM).text_color(if self.search_error.is_some() {
              theme.danger
            } else {
              theme.muted_foreground
            }))
            .when(!self.search_results.is_empty(), |row| {
              row.child(
                Button::new("open-search-results")
                  .link()
                  .xsmall()
                  .label("Open in editor")
                  .on_click(move |_, window, cx| focus.dispatch_action(&OpenSearchResults, window, cx)),
              )
            }),
        )
      })
      .child(self.render_search_results(cx))
      .into_any_element()
  }

  fn render_search_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let mut rows = Vec::new();
    let mut first = 0;
    for group in self.search_results.chunk_by(|left, right| left.path == right.path) {
      rows.push(SearchListRow::File { first, count: group.len() });
      if !self.search_collapsed.contains(&group[0].path) {
        rows.extend((first..first + group.len()).map(SearchListRow::Match));
      }
      first += group.len();
    }
    let list = uniform_list(
      "project-search-results",
      rows.len(),
      cx.processor(move |this, range: Range<usize>, _, cx| {
        rows
          .get(range)
          .unwrap_or_default()
          .iter()
          .map(|row| match *row {
            SearchListRow::File { first, count } => this.render_search_file(first, count, cx),
            SearchListRow::Match(index) => this.render_search_match(index, cx),
          })
          .collect::<Vec<_>>()
      }),
    )
    .track_scroll(&self.search_scroll)
    .size_full();
    div()
      .relative()
      .flex_1()
      .min_h_0()
      .child(list)
      .child(div().absolute().inset_0().child(Scrollbar::new(&self.search_scroll).axis(ScrollbarAxis::Vertical).viewport_from_layout()))
  }

  fn render_search_file(&self, first: usize, count: usize, cx: &mut Context<Self>) -> AnyElement {
    let path = self.search_results[first].path.clone();
    let root = self.projects.active.as_deref().unwrap_or_else(|| Path::new(""));
    let relative = path.strip_prefix(root).unwrap_or(&path);
    let filename = SharedString::from(relative.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let folder = SharedString::from(relative.parent().unwrap_or_else(|| Path::new("")).display().to_string());
    let collapsed = self.search_collapsed.contains(&path);
    let palette = colors(cx);
    h_flex()
      .id(SharedString::from(format!("search-file-{}", path.display())))
      .h_7()
      .gap_1()
      .pr_3()
      .cursor_pointer()
      .rounded_md()
      .hover(|row| row.bg(palette.hover))
      .child(Icon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }).xsmall())
      .child(Label::new(filename).font_weight(FontWeight::SEMIBOLD).truncate())
      .child(Label::new(folder).flex_1().min_w_0().text_xs().text_color(palette.text_secondary).truncate())
      .child(div().rounded_full().bg(palette.selected).px_2().text_xs().text_color(palette.text).child(count.to_string()))
      .on_click(cx.listener(move |this, _, _, cx| {
        if !this.search_collapsed.remove(&path) {
          this.search_collapsed.insert(path.clone());
        }
        cx.notify();
      }))
      .into_any_element()
  }

  fn render_search_match(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
    let found = self.search_results[index].clone();
    let palette = colors(cx);
    h_flex()
      .id(SharedString::from(format!("search-match-{}-{}-{}", found.path.display(), found.line, found.column)))
      .h_7()
      .gap_2()
      .pl_4()
      .pr_3()
      .cursor_pointer()
      .rounded_md()
      .hover(|row| row.bg(palette.hover))
      .child(Label::new((found.line + 1).to_string()).text_xs().text_color(palette.text_secondary))
      .child(div().min_w_0().truncate().child(StyledText::new(SharedString::from(found.preview.clone())).with_highlights([(
        found.highlight.clone(),
        HighlightStyle { background_color: Some(cx.theme().primary.opacity(0.28)), ..Default::default() },
      )])))
      .on_click(cx.listener(move |this, _, window, cx| this.open_search_result(&found, window, cx)))
      .into_any_element()
  }
}
