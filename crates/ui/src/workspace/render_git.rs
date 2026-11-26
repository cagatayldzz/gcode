//! Favorite projects, Git toolbar, history and compare views.

use super::*;
use crate::{SelectNextRow, SelectPreviousRow};
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::ElementExt as _;

/// The fixed height of a line in the Changes list: the virtualized list needs every row's height before it draws.
/// How long a project tab takes to glide to its place after a swap.
pub(super) const TAB_SLIDE: Duration = Duration::from_millis(180);

const CHANGE_ROW_HEIGHT: Pixels = px(28.);

impl Workspace {
  /// The row sizes of the Changes list, rebuilt only when the number of files changes.
  fn change_sizes(&self, count: usize) -> Rc<Vec<Size<Pixels>>> {
    let mut cached = self.change_sizes.borrow_mut();
    if cached.len() != count {
      *cached = Rc::new(vec![size(px(0.), CHANGE_ROW_HEIGHT); count]);
    }
    cached.clone()
  }

  pub(super) fn render_favorite_projects(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let workspace = cx.entity();
    self
      .projects
      .favorites()
      .map(|project| {
        let path = project.path.clone();
        let selected = self.projects.active.as_ref() == Some(&path);
        let look = avatar_look(project, selected, cx);
        let status = self.status_of(&path);
        let terminal_active = self.project_has_terminal_activity(&path, cx);
        let workspace = workspace.clone();
        let held = self.dragged_tab.as_ref() == Some(&path) && cx.has_active_drag();
        let picked_up = workspace.clone();
        let picked = path.clone();
        let target = path.clone();
        let swapper = workspace.clone();
        let menu_workspace = workspace.clone();
        let menu_path = path.clone();
        let bounds_slot = self.tab_bounds.clone();
        let slide = self.tab_slides.get(&path).copied();
        let bounds_key = path.clone();
        let outline = theme.primary;
        let button = project_bar_button(SharedString::from(format!("favorite-project-{}", path.display())), selected, cx)
          .child(
            h_flex()
              .gap_2()
              .items_center()
              .when_some(look.icon.clone(), |row, icon| {
                row.child(
                  div()
                    .size_6()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .text_color(if selected { palette.explorer_selected_text } else { look.text })
                    .when_some(look.fill.filter(|_| !selected), |avatar, fill| avatar.bg(fill))
                    .child(Icon::empty().path(icon).size_4()),
                )
              })
              .child(
                Label::new(project.name())
                  .when(selected, |label| label.text_color(palette.explorer_selected_text))
                  .min_w_0()
                  .font_weight(FontWeight::SEMIBOLD)
                  .overflow_hidden()
                  .whitespace_nowrap()
                  .text_ellipsis(),
              )
              .children(project_status_marks("favorite", &path, status.as_ref(), false, cx))
              .when(terminal_active, |row| row.child(terminal_activity_icon(cx))),
          )
          .when(terminal_active, |button| button.tooltip("Terminal command running"))
          .on_click({
            let path = path.clone();
            move |_, window, cx| workspace.update(cx, |this, cx| this.select_project(path.clone(), window, cx))
          });
        // Dragging a tab past the middle of its neighbor swaps them at once, like the tabs of a browser.
        let tab = div()
          .id(SharedString::from(format!("favorite-drag-{}", path.display())))
          .relative()
          .on_prepaint(move |bounds, _, _| {
            bounds_slot.borrow_mut().insert(bounds_key.clone(), bounds);
          })
          // The tab itself slides along the bar, so nothing follows the pointer up or down. The tab that is held gets an outline.
          .when(held, |tab| tab.child(div().absolute().inset_0().rounded_md().border_1().border_color(outline)))
          .on_drag(DraggedProjectTab(path.clone()), move |_, _, _, cx| {
            picked_up.update(cx, |this, _| this.dragged_tab = Some(picked.clone()));
            cx.new(|_| NoDragPreview)
          })
          .on_drag_move::<DraggedProjectTab>(move |event, window, cx| {
            let gap = window.rem_size() * 0.5;
            let center = event.bounds.center().x;
            let dragged = event.drag(cx).0.clone();
            if dragged == target || !event.bounds.contains(&event.event.position) {
              return;
            }
            let pointer = event.event.position.x;
            // Only swap once the pointer has crossed the middle of the target, toward the side the dragged tab came from.
            let _ = center;
            let (dragged, target) = (dragged, target.clone());
            swapper.update(cx, |this, cx| {
              let position = |path: &Path| this.projects.projects.iter().position(|project| project.path == path);
              if let (Some(from), Some(to)) = (position(&dragged), position(&target)) {
                let crossed = if from < to { pointer > center } else { pointer < center };
                if crossed {
                  this.swap_projects(&dragged, &target, gap, cx);
                }
              }
            });
          })
          .child(button)
          .context_menu(move |menu, _, _| {
            let (workspace, path) = (menu_workspace.clone(), menu_path.clone());
            menu.item(
              PopupMenuItem::new("Remove from favorites")
                .icon(Icon::empty().path("icons/star.svg"))
                .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.toggle_favorite(&path, cx))),
            )
          });
        // After a swap the tab starts where it was and glides to its place.
        match slide {
          Some((shift, swap)) => tab
            .with_animation(
              SharedString::from(format!("favorite-slide-{}-{swap}", path.display())),
              Animation::new(TAB_SLIDE).with_easing(ease_out_quint()),
              move |tab, delta| tab.left(px(shift * (1. - delta))),
            )
            .into_any_element(),
          None => tab.into_any_element(),
        }
      })
      .collect()
  }

  /// The bar above the editor in the Git tab: the branch with its menu, and the one button that keeps the branch in step with
  /// its remote: publish, push, pull or fetch, whichever the branch needs.
  pub(super) fn render_git_toolbar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let git = self.git.as_ref()?;
    let theme = cx.theme().clone();
    let accent = theme.primary;
    let palette = colors(cx);
    let picker = self.render_branch_picker(&git.branch, cx);
    let sync = git.sync.as_ref().map(|sync| {
      let (icon, title, count) = if !sync.upstream {
        ("icons/upload.svg", "Publish branch".to_string(), 0)
      } else if sync.behind > 0 {
        ("icons/arrow-down.svg", format!("Pull {}", sync.remote), sync.behind)
      } else if sync.ahead > 0 {
        ("icons/arrow-up.svg", format!("Push {}", sync.remote), sync.ahead)
      } else {
        ("icons/refresh-cw.svg", format!("Fetch {}", sync.remote), 0)
      };
      let detail = if let Some(work) = self.remote_work_here() {
        work.progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
      } else if !sync.upstream {
        format!("Publish this branch to {}", sync.remote)
      } else {
        match sync.last_fetched {
          Some(time) => format!("Last fetched {}", time_ago(time)),
          None => "Never fetched".to_string(),
        }
      };
      Button::new("sync-remote")
        .custom(ButtonCustomVariant::new(cx).color(palette.surface).foreground(palette.text).hover(palette.hover).active(palette.hover))
        .cursor_pointer()
        .border_0()
        .h_12()
        .w_56()
        .rounded_none()
        .disabled(self.remote_work.is_some() || git.merging || self.merge_task.is_some())
        .child(
          h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(match self.remote_work_here() {
              Some(_) => Spinner::new().small().into_any_element(),
              None => Icon::empty().path(icon).size_5().text_color(theme.muted_foreground).into_any_element(),
            })
            .child(
              v_flex()
                .flex_1()
                .min_w_0()
                .items_start()
                .child(
                  Label::new(self.remote_work_here().map_or(title.into(), |work| {
                    if work.fetching_after_push.load(Ordering::Relaxed) {
                      format!("Fetching {}", sync.remote).into()
                    } else {
                      work.title.clone()
                    }
                  }))
                  .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                  Label::new(detail)
                    .text_size(FONT_SIZE_SM)
                    .text_color(theme.muted_foreground)
                    .max_w_full()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
                ),
            )
            // How many commits are waiting to be pushed (or pulled): a filled blue circle, easy to see.
            .when(count > 0, |row| {
              row.child(
                div()
                  .min_w_5()
                  .h_5()
                  .px_1p5()
                  .flex()
                  .flex_none()
                  .items_center()
                  .justify_center()
                  .rounded_full()
                  .bg(theme.primary)
                  .text_color(palette.on_accent)
                  .text_size(FONT_SIZE_SM)
                  .font_weight(FontWeight::SEMIBOLD)
                  .child(count.to_string()),
              )
            }),
        )
        .on_click(cx.listener(|this, _, window, cx| this.sync_remote(window, cx)))
    });
    Some(
      h_flex()
        .flex_none()
        .h_12()
        .bg(palette.surface)
        .border_b_1()
        .border_color(palette.border)
        .child({
          // The slot is measured: the branch menu opens at its bottom left corner.
          let slot = self.branch_trigger_bounds.clone();
          div().h_12().border_r_1().border_color(palette.border).child(picker).on_prepaint(move |bounds, _, _| slot.set(bounds))
        })
        .children(sync.map(|button| {
          // While a push, pull or fetch runs, a soft fill of the accent color grows across the button from left to right as
          // far as git has got.
          let fill = div().absolute().top_0().bottom_0().left_0().bg(accent.opacity(0.15));
          let fill = if let Some(work) = self.remote_work_here() {
            Some(fill.w(relative(work.shown())).into_any_element())
          } else {
            self.remote_finish.clone().filter(|(_, _, root)| job_scope::shown_in(root, self.projects.active.as_deref())).map(|(from, run, _)| {
              fill
                .with_animation(("remote-finish", run), Animation::new(REMOTE_FINISH), move |fill, delta| {
                  fill.w(relative(from + (1. - from) * delta))
                })
                .into_any_element()
            })
          };
          div().relative().h_12().overflow_hidden().border_r_1().border_color(palette.border).child(button).children(fill)
        })),
    )
  }

  /// The changed files whose path contains the filter text (all of them without one).
  fn visible_changes(&self, query: &str) -> Vec<&ws_git::Change> {
    let Some(git) = self.git.as_ref() else { return Vec::new() };
    git.changes.iter().filter(|change| query.is_empty() || change.path.to_string_lossy().to_lowercase().contains(query)).collect()
  }

  /// ↑ in the Changes or History list: the row above the chosen one.
  pub(super) fn select_previous_row(&mut self, _: &SelectPreviousRow, _: &mut Window, cx: &mut Context<Self>) {
    self.select_adjacent_row(-1, cx);
  }

  /// ↓ in the Changes or History list: the row below the chosen one.
  pub(super) fn select_next_row(&mut self, _: &SelectNextRow, _: &mut Window, cx: &mut Context<Self>) {
    self.select_adjacent_row(1, cx);
  }

  /// Chooses the row `step` places from the chosen one in the list that is shown, and brings it into view. With none chosen, ↓ takes the
  /// first row and ↑ the last. Stops at the ends.
  fn select_adjacent_row(&mut self, step: isize, cx: &mut Context<Self>) {
    let next = |current: Option<usize>, len: usize| -> Option<usize> {
      match current {
        _ if len == 0 => None,
        None if step > 0 => Some(0),
        None => Some(len - 1),
        Some(index) => Some(index.saturating_add_signed(step).min(len - 1)),
      }
    };
    match self.git_view {
      GitView::Changes => {
        let query = self.changes_filter.read(cx).value().trim().to_lowercase();
        let paths: Vec<PathBuf> = self.visible_changes(&query).into_iter().map(|change| change.path.clone()).collect();
        let current = self.selected_change.as_ref().and_then(|selected| paths.iter().position(|path| path == selected));
        let Some(index) = next(current, paths.len()) else { return };
        self.chosen_changes.clear();
        self.choice_anchor = Some(paths[index].clone());
        self.open_change(paths[index].clone(), cx);
        // Scrolls only when the row is out of view, so that the list does not jump on every key.
        let (viewport, offset) = (self.changes_scroll.bounds().size.height, -self.changes_scroll.base_handle().offset().y);
        let (top, bottom) = (CHANGE_ROW_HEIGHT * index as f32, CHANGE_ROW_HEIGHT * (index + 1) as f32);
        if top < offset {
          self.changes_scroll.scroll_to_item(index, ScrollStrategy::Top);
        } else if bottom > offset + viewport {
          self.changes_scroll.scroll_to_item(index, ScrollStrategy::Bottom);
        }
      }
      GitView::History if self.compare.is_none() && !self.compare_open => {
        let current = self.selected_commit.as_ref().and_then(|selected| self.history.iter().position(|commit| commit.id == *selected));
        let Some(index) = next(current, self.history.len()) else { return };
        self.select_commit(self.history[index].id.clone(), cx);
        self.history_scroll.scroll_to_item(index);
      }
      GitView::History => {}
    }
  }

  /// ⌘-click: adds a file to the chosen ones, or takes it out. The file that is open starts the choice.
  fn toggle_change_choice(&mut self, file: PathBuf, cx: &mut Context<Self>) {
    if self.chosen_changes.is_empty() {
      self.chosen_changes.extend(self.selected_change.clone());
    }
    if !self.chosen_changes.remove(&file) {
      self.chosen_changes.insert(file.clone());
    }
    self.choice_anchor = Some(file);
    cx.notify();
  }

  /// ⇧-click: chooses every file of the list between the one chosen last and this one.
  fn extend_change_choice(&mut self, file: PathBuf, cx: &mut Context<Self>) {
    let query = self.changes_filter.read(cx).value().trim().to_lowercase();
    let paths: Vec<PathBuf> = self.visible_changes(&query).into_iter().map(|change| change.path.clone()).collect();
    let anchor = self.choice_anchor.clone().or_else(|| self.selected_change.clone()).unwrap_or_else(|| file.clone());
    if let (Some(from), Some(to)) = (paths.iter().position(|path| *path == anchor), paths.iter().position(|path| *path == file)) {
      self.chosen_changes = paths[from.min(to)..=from.max(to)].iter().cloned().collect();
    }
    cx.notify();
  }

  /// The files chosen together that are still changed, in the order of the list; more than one only when several were chosen.
  fn chosen_changed_files(&self) -> Vec<PathBuf> {
    let Some(git) = self.git.as_ref() else { return Vec::new() };
    git.changes.iter().filter(|change| self.chosen_changes.contains(&change.path)).map(|change| change.path.clone()).collect()
  }

  /// The right-click menu of several chosen files: what can be done to all of them at once.
  fn chosen_menu(menu: PopupMenu, workspace: &Entity<Self>, files: &[PathBuf], busy: bool) -> PopupMenu {
    let count = files.len();
    let extensions: HashSet<String> =
      files.iter().filter_map(|file| file.extension().map(|extension| extension.to_string_lossy().into_owned())).collect();
    let common_extension =
      (extensions.len() == 1 && files.iter().all(|file| file.extension().is_some())).then(|| extensions.into_iter().next()).flatten();
    let (discard, ignore, ignore_extension, include, exclude) =
      (workspace.clone(), workspace.clone(), workspace.clone(), workspace.clone(), workspace.clone());
    let (discard_files, ignore_files, include_files, exclude_files) = (files.to_vec(), files.to_vec(), files.to_vec(), files.to_vec());
    let copied = files.iter().map(|file| file.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\n");
    menu
      .item(PopupMenuItem::new(format!("Discard {count} Selected Changes…")).icon(Icon::empty().path("icons/trash.svg")).disabled(busy).on_click(
        move |_, window, cx| {
          discard.update(cx, |this, cx| {
            this.confirm_discard_changes(discard_files.clone(), window, cx);
            this.chosen_changes.clear();
          })
        },
      ))
      .separator()
      .item(PopupMenuItem::new(format!("Ignore {count} Selected Files (Add to .gitignore)")).icon(Icon::empty().path("icons/eye-off.svg")).on_click(
        move |_, window, cx| {
          let patterns = ignore_files.iter().map(|file| format!("/{}", file.display())).collect();
          ignore.update(cx, |this, cx| {
            this.ignore_patterns(patterns, window, cx);
            this.chosen_changes.clear();
          })
        },
      ))
      .when_some(common_extension, |menu, extension| {
        menu.item(
          PopupMenuItem::new(format!("Ignore All .{extension} Files (Add to .gitignore)"))
            .icon(Icon::empty().path("icons/eye-off.svg"))
            .on_click(move |_, window, cx| ignore_extension.update(cx, |this, cx| this.ignore_patterns(vec![format!("*.{extension}")], window, cx))),
        )
      })
      .separator()
      .item(PopupMenuItem::new("Include Selected Files").icon(Icon::empty().path("icons/check.svg")).on_click(move |_, _, cx| {
        include.update(cx, |this, cx| {
          for file in &include_files {
            this.excluded.remove(file);
          }
          cx.notify();
        })
      }))
      .item(PopupMenuItem::new("Exclude Selected Files").icon(Icon::empty().path("icons/x.svg")).on_click(move |_, _, cx| {
        exclude.update(cx, |this, cx| {
          this.excluded.extend(exclude_files.iter().cloned());
          cx.notify();
        })
      }))
      .separator()
      .item(
        PopupMenuItem::new("Copy Relative Paths")
          .icon(Icon::empty().path("icons/copy.svg"))
          .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))),
      )
  }

  /// One line of the Changes list: a checkbox, the file name with its folder, and the kind of change. Its height is fixed
  /// ([`CHANGE_ROW_HEIGHT`]) because the list is virtualized, and its width is the list's own: a virtual list lays its rows out
  /// at their content width, so a percentage would leave short rows narrow and long ones unclipped.
  fn change_row(&self, change: &ws_git::Change, width: Pixels, last: bool, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let workspace = cx.entity();
    let root = self.projects.active.clone().unwrap_or_default();
    let file = change.path.clone();
    let selected = self.selected_change.as_ref() == Some(&change.path) || self.chosen_changes.contains(&change.path);
    // Right-clicking one of several chosen files acts on all of them.
    let chosen = Some(self.chosen_changes.len())
      .filter(|count| *count > 1)
      .filter(|_| self.chosen_changes.contains(&change.path))
      .map(|_| self.chosen_changed_files())
      .filter(|files| files.len() > 1);
    let included = !self.excluded.contains(&change.path);
    // The file name first, then its folder in a smaller, secondary color; a renamed file shows where it was before the folder. Both
    // are cut with an ellipsis by the row's width.
    let name = change.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = change.path.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
    let detail = match (&change.from, folder) {
      (Some(from), Some(folder)) => Some(format!("{folder}  ← {}", from.display())),
      (Some(from), None) => Some(format!("← {}", from.display())),
      (None, folder) => folder,
    };
    h_flex()
      .id(SharedString::from(format!("change-row-{}", change.path.display())))
      .when(width.is_zero(), |row| row.w_full())
      .when(!width.is_zero(), |row| row.w(width))
      .min_w_0()
      .overflow_hidden()
      .h(CHANGE_ROW_HEIGHT)
      .gap_1p5()
      .px_3()
      .when(!last, |row| row.border_b_1())
      .border_color(crate::theme::row_divider(&palette))
      .items_center()
      // A chosen row is blue, and a little deeper under the pointer.
      .when(selected, |row| row.bg(palette.selected))
      .hover(|row| row.bg(if selected { palette.selected_hover } else { palette.hover }))
      .child(Checkbox::new(SharedString::from(format!("include-{}", change.path.display()))).checked(included).on_click(cx.listener({
        let file = file.clone();
        move |this, checked: &bool, _, cx| {
          if *checked {
            this.excluded.remove(&file);
          } else {
            this.excluded.insert(file.clone());
          }
          cx.notify();
        }
      })))
      .child(
        h_flex()
          .id(SharedString::from(format!("change-{}", change.path.display())))
          .flex_1()
          .min_w_0()
          .gap_1p5()
          .items_center()
          .cursor_pointer()
          .child(
            // One path, the file name last, cut when the row is narrow; a renamed file shows where it was before. The room is
            // the row less its padding, the checkbox, the status icon and the gaps.
            h_flex()
              .flex_1()
              .min_w_0()
              .gap_2()
              .items_baseline()
              .child(
                Label::new(name)
                  .text_size(crate::components::CHANGE_PATH_FONT_SIZE)
                  .font_weight(FontWeight::MEDIUM)
                  .flex_none()
                  .max_w_3_4()
                  .truncate(),
              )
              .children(
                detail.map(|detail| Label::new(detail).flex_1().min_w_0().text_size(FONT_SIZE_SM).text_color(palette.text_secondary).truncate()),
              ),
          )
          .child(Icon::empty().path(change_kind_icon(change.kind)).size_4().flex_none().text_color(kind_color(change.kind, &palette)))
          .on_click(cx.listener({
            let file = file.clone();
            // A double click opens the file itself, in the editor with the Explorer; one click shows its diff.
            move |this, event: &ClickEvent, window, cx| {
              let modifiers = event.modifiers();
              if modifiers.platform {
                this.toggle_change_choice(file.clone(), cx);
              } else if modifiers.shift {
                this.extend_change_choice(file.clone(), cx);
              } else if event.click_count() >= 2 {
                this.open_change_in_editor(&file, window, cx);
              } else {
                this.chosen_changes.clear();
                this.choice_anchor = Some(file.clone());
                this.open_change(file.clone(), cx);
              }
            }
          })),
      )
      .context_menu({
        let (workspace, deleted) = (workspace.clone(), change.kind == ws_git::ChangeKind::Deleted);
        let root = root.clone();
        let busy = self.committing || self.discarding;
        let ignore_folder = file.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
        let ignore_extension = file.extension().map(|extension| extension.to_string_lossy().into_owned());
        move |menu, _, _| {
          if let Some(files) = &chosen {
            return Self::chosen_menu(menu, &workspace, files, busy);
          }
          let ignore_workspace = workspace.clone();
          let ignore_file = file.clone();
          let (ignore_folder, ignore_extension) = (ignore_folder.clone(), ignore_extension.clone());
          let (open_workspace, open_file) = (workspace.clone(), file.clone());
          let (discard_workspace, discard_file) = (workspace.clone(), file.clone());
          let (finder_root, finder_file) = (root.clone(), file.clone());
          let copy_path = file.to_string_lossy().into_owned();
          menu
            .item(
              PopupMenuItem::new("Open in Editor")
                .icon(Icon::empty().path("icons/file-pen-line.svg"))
                .disabled(deleted)
                .on_click(move |_, window, cx| open_workspace.update(cx, |this, cx| this.open_change_in_editor(&open_file, window, cx))),
            )
            .item(
              PopupMenuItem::new(if deleted { "Open Containing Folder" } else { "Show in Finder" })
                .icon(Icon::empty().path("icons/folder-open.svg"))
                .on_click(move |_, window, cx| Self::show_change_in_finder(finder_root.clone(), finder_file.clone(), window, cx)),
            )
            .item(
              PopupMenuItem::new("Copy Relative Path")
                .icon(Icon::empty().path("icons/copy.svg"))
                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()))),
            )
            .separator()
            .item(PopupMenuItem::new("Ignore File (Add to .gitignore)").icon(Icon::empty().path("icons/eye-off.svg")).on_click({
              let (workspace, pattern) = (ignore_workspace.clone(), format!("/{}", ignore_file.display()));
              move |_, window, cx| workspace.update(cx, |this, cx| this.ignore_patterns(vec![pattern.clone()], window, cx))
            }))
            .when_some(ignore_folder, |menu, folder| {
              let workspace = ignore_workspace.clone();
              menu.item(
                PopupMenuItem::new("Ignore Folder (Add to .gitignore)")
                  .icon(Icon::empty().path("icons/eye-off.svg"))
                  .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.ignore_patterns(vec![format!("/{folder}/")], window, cx))),
              )
            })
            .when_some(ignore_extension, |menu, extension| {
              let workspace = ignore_workspace.clone();
              menu.item(
                PopupMenuItem::new(format!("Ignore All .{extension} Files (Add to .gitignore)"))
                  .icon(Icon::empty().path("icons/eye-off.svg"))
                  .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.ignore_patterns(vec![format!("*.{extension}")], window, cx))),
              )
            })
            .separator()
            .item(PopupMenuItem::new("Discard Changes…").icon(Icon::empty().path("icons/trash.svg")).disabled(busy).on_click(move |_, window, cx| {
              discard_workspace.update(cx, |this, cx| this.confirm_discard_changes(vec![discard_file.clone()], window, cx))
            }))
        }
      })
  }

  pub(super) fn render_git(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let palette = colors(cx);
    let git = self.git.as_ref()?;
    self.projects.active.as_ref()?;
    let changed = match git.changed() {
      0 => "No changes".to_string(),
      1 => "1 changed file".to_string(),
      count => format!("{count} changed files"),
    };
    let chosen = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).count();
    let workspace = cx.entity();
    // Only the rows in view are built: a project can have thousands of changed files.
    let query = self.changes_filter.read(cx).value().trim().to_lowercase();
    let shown = self.visible_changes(&query).len();
    let changes_list = v_virtual_list(workspace.clone(), "git-changes", self.change_sizes(shown), |this, range, _, cx| {
      let query = this.changes_filter.read(cx).value().trim().to_lowercase();
      let all = this.visible_changes(&query);
      let total = all.len();
      let first = range.start;
      let changes: Vec<ws_git::Change> = all.get(range).map(|rows| rows.iter().map(|change| (*change).clone()).collect()).unwrap_or_default();
      let width = this.changes_scroll.bounds().size.width;
      // The last row has no line under it: the one of what follows the list (the commit box) is there.
      changes.iter().enumerate().map(|(offset, change)| this.change_row(change, width, first + offset + 1 == total, cx)).collect()
    })
    .track_scroll(&self.changes_scroll);
    // The scrollbar strip is shown only when the rows are taller than the list. The height of the list is known once it was drawn, so
    // the first time a frame is asked for again.
    let viewport = self.changes_scroll.bounds().size.height;
    let overflows = !viewport.is_zero() && CHANGE_ROW_HEIGHT * shown as f32 > viewport;
    if viewport.is_zero() {
      let this = workspace.clone();
      cx.defer(move |cx| this.update(cx, |_, cx| cx.notify()));
    }
    let summary_filled = !self.commit_summary.read(cx).value().trim().is_empty();
    let label = match chosen {
      1 => format!("Commit 1 file to {}", git.branch),
      count => format!("Commit {count} files to {}", git.branch),
    };
    let commit_box = v_flex()
      .flex_none()
      .gap_1p5()
      .px_3()
      .py_2()
      .border_t_1()
      .border_color(palette.border)
      .bg(palette.surface)
      .child(Input::new(&self.commit_summary).bg(palette.background).border_color(palette.border))
      .child(
        div()
          .relative()
          .child(crate::components::soft_textarea(&self.commit_description, cx).h(rems(4.5)).bg(palette.background).border_color(palette.border))
          // The AI buttons belong to the AI Agent extension: without it the box is just the text.
          .when(crate::extensions::is_enabled(ws_extensions::AI_AGENT, cx), |area| {
            let workspace = workspace.clone();
            area.child(
              div()
                .id("generate-message-area")
                .absolute()
                .right_1()
                .bottom_1()
                .child(
                  Button::new("generate-message")
                    .custom(
                      ButtonCustomVariant::new(cx).color(palette.surface).foreground(cx.theme().primary).hover(palette.hover).active(palette.hover),
                    )
                    .cursor_pointer()
                    .small()
                    .icon(Icon::empty().path("icons/sparkles.svg"))
                    .loading(self.generating)
                    .disabled(chosen == 0 || self.generating)
                    .tooltip("Generate the summary and description with AI")
                    .on_click(cx.listener(|this, _, window, cx| this.generate_commit_message(window, cx))),
                )
                // Right click: the settings of the generator, and switching the extension off or removing it.
                .context_menu(move |menu, _, _| {
                  let (settings, disable, remove) = (workspace.clone(), workspace.clone(), workspace.clone());
                  menu
                    .item(
                      PopupMenuItem::new("Settings")
                        .icon(Icon::empty().path("icons/settings-2.svg"))
                        .on_click(move |_, window, cx| settings.update(cx, |this, cx| this.open_commit_settings(window, cx))),
                    )
                    .separator()
                    .item(
                      PopupMenuItem::new("Disable")
                        .icon(Icon::empty().path("icons/circle-slash.svg"))
                        .on_click(move |_, window, cx| disable.update(cx, |this, cx| this.confirm_disable_ai_agent(window, cx))),
                    )
                    .item(
                      PopupMenuItem::new("Remove")
                        .icon(Icon::empty().path("icons/trash.svg"))
                        .on_click(move |_, window, cx| remove.update(cx, |this, cx| this.confirm_remove_ai_agent(window, cx))),
                    )
                }),
            )
          }),
      )
      .child(
        div()
          .id("commit-wrapper")
          .w_full()
          .child(
            Button::new("commit")
              .primary()
              .small()
              .w_full()
              .label(label)
              .disabled(chosen == 0 || !summary_filled || self.committing || self.discarding || git.merging || self.merge_task.is_some())
              .on_click(cx.listener(|this, _, window, cx| this.commit_changes(window, cx))),
          )
          .context_menu({
            let workspace = workspace.clone();
            let disabled = self.committing || self.discarding || git.merging || self.merge_task.is_some();
            let root = self.projects.active.clone().unwrap_or_default();
            move |menu, window, cx| {
              let (amend_workspace, author_workspace, author_root) = (workspace.clone(), workspace.clone(), root.clone());
              menu
                .item(
                  PopupMenuItem::new("Amend Last Commit")
                    .icon(Icon::empty().path("icons/undo-2.svg"))
                    .disabled(disabled)
                    .on_click(move |_, window, cx| amend_workspace.update(cx, |this, cx| this.amend_commit(window, cx))),
                )
                .submenu("Add Co-author", window, cx, move |submenu, _, _| {
                  let people = ws_git::recent_authors(&author_root, 500, 12);
                  if people.is_empty() {
                    return submenu.item(PopupMenuItem::new("No other authors in this repository").disabled(true));
                  }
                  people.into_iter().fold(submenu, |submenu, (name, email)| {
                    let (workspace, label) = (author_workspace.clone(), format!("{name} <{email}>"));
                    submenu.item(
                      PopupMenuItem::new(label)
                        .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.add_co_author(&name, &email, window, cx))),
                    )
                  })
                })
            }
          }),
      );
    let tabs = TabBar::new("git-view-tabs")
      .underline()
      .selected_index(usize::from(self.git_view == GitView::History))
      .on_click(cx.listener(|this, index: &usize, _, cx| this.set_git_view(if *index == 1 { GitView::History } else { GitView::Changes }, cx)))
      .children([
        Tab::new()
          .cursor_pointer()
          .flex_1()
          .justify_center()
          .label("Changes")
          .when(git.changed() > 0, |tab| tab.suffix(count_badge(git.changed(), cx))),
        Tab::new().cursor_pointer().flex_1().justify_center().label("History"),
      ]);
    let body = match self.git_view {
      GitView::Changes => v_flex()
        .flex_1()
        .min_h_0()
        .child(div().flex_none().px_3().py_2().child(crate::components::soft_input(&self.changes_filter, cx).bg(palette.background)))
        .child(
          h_flex()
            .id("changes-header")
            .justify_between()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(palette.border)
            .child(
              Checkbox::new("include-all-changes")
                .label(changed)
                .checked(git.changed() > 0 && chosen == git.changed())
                .indeterminate(chosen > 0 && chosen < git.changed())
                .disabled(git.changed() == 0)
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                  if let Some(git) = &this.git {
                    for change in &git.changes {
                      if *checked {
                        this.excluded.remove(&change.path);
                      } else {
                        this.excluded.insert(change.path.clone());
                      }
                    }
                    cx.notify();
                  }
                })),
            )
            .child(
              Button::new("toggle-all-changes")
                .ghost()
                .xsmall()
                .selected(self.all_changes_open)
                .disabled(git.changes.is_empty() && !self.all_changes_open)
                .icon(Icon::empty().path("icons/layout-list.svg").size_4())
                .accessibility_label(if self.all_changes_open { "Close all changes" } else { "Show all changes" })
                .tooltip(if self.all_changes_open { "Close all changes" } else { "Show all changes" })
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleAllChanges), cx)),
            )
            .context_menu({
              let disabled = git.changes.is_empty() || self.committing || self.discarding;
              move |menu, _, _| {
                let workspace = workspace.clone();
                menu.item(PopupMenuItem::new("Discard All Changes…").icon(Icon::empty().path("icons/trash.svg")).disabled(disabled).on_click(
                  move |_, window, cx| {
                    workspace.update(cx, |this, cx| {
                      if let Some(git) = &this.git {
                        let files = git.changes.iter().map(|change| change.path.clone()).collect();
                        this.confirm_discard_changes(files, window, cx);
                      }
                    });
                  },
                ))
              }
            }),
        )
        .child(
          div()
            .relative()
            .flex_1()
            .min_h_0()
            .key_context("ChangesList")
            .track_focus(&self.changes_focus)
            .when(self.switching_branch.is_some(), |area| area.child(crate::components::skeleton_list(8)))
            .when(git.changed() > 0 && self.switching_branch.is_none(), |list| {
              // The scrollbar has a column of its own beside the list, so that it never covers the status icons of the rows.
              list.child(
                h_flex()
                  .size_full()
                  .bg(palette.background)
                  .child(div().relative().flex_1().min_w_0().h_full().child(changes_list))
                  .when(overflows, |row| row.child(crate::components::scrollbar_strip(&self.changes_scroll, "changes-scrollbar", cx))),
              )
            }),
        )
        .child(commit_box)
        .into_any_element(),
      GitView::History => v_flex().flex_1().min_h_0().pt_2().child(self.render_history(cx)).into_any_element(),
    };
    Some(v_flex().size_full().child(div().px_3().child(tabs)).child(body))
  }

  /// The commits of the open project, newest first: the subject, who made it and when, the tags on it, and an arrow while the
  /// remote does not have it yet. Choosing one shows it in the main area.
  pub(super) fn render_history(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let body = if self.compare_open {
      self.render_compare_choices(cx)
    } else if let Some(view) = &self.compare {
      self.render_compare(view, cx)
    } else if self.switching_branch.is_some() {
      // What the list holds belongs to the branch that is being left: a skeleton stands in until the new branch's commits are read.
      crate::components::skeleton_list(10)
    } else if self.history_loading {
      let muted = cx.theme().muted_foreground;
      h_flex()
        .gap_2()
        .px_3()
        .py_2()
        .items_center()
        .child(Spinner::new().small())
        .child(Label::new("Loading commits…").text_color(muted))
        .into_any_element()
    } else {
      self.commit_list(&self.history, true, cx)
    };
    v_flex().flex_1().min_h_0().gap_2().child(div().px_3().child(self.render_compare_field(cx))).child(v_flex().flex_1().min_h_0().child(body))
  }

  /// A list of commits: the subject, who made it and when, the tags on it, and an arrow while the remote does not have it yet.
  /// With `paged` the next page of the History view is loaded when the list is scrolled to its end.
  pub(super) fn commit_list(&self, commits: &[CommitInfo], paged: bool, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    if commits.is_empty() {
      return Label::new(if paged { "No commits yet" } else { "No commits" }).px_2().py_2().text_color(theme.muted_foreground).into_any_element();
    }
    let workspace = cx.entity();
    let tip = self.history.first().map(|commit| commit.id.clone());
    let git_branch = self.git.as_ref().map(|git| git.branch.clone()).unwrap_or_default();
    let rows = commits.iter().map(|commit| {
      let id = commit.id.clone();
      let selected = self.selected_commit.as_deref() == Some(commit.id.as_str());
      // Only the last commit, and only while the remote does not have it, can be undone.
      let undoable = tip.as_deref() == Some(commit.id.as_str()) && commit.unpushed;
      let (menu_workspace, menu_id) = (workspace.clone(), commit.id.clone());
      let (menu_root, menu_current) = (self.projects.active.clone().unwrap_or_default(), git_branch.clone());
      // Only the first letter of the author: the avatar takes two letters from a name of several words.
      let initial: SharedString = commit.author.trim().chars().take(1).flat_map(char::to_uppercase).collect::<String>().into();
      h_flex()
        .id(SharedString::from(format!("commit-{}", commit.id)))
        .gap_2()
        .px_3()
        .py_1p5()
        .items_center()
        .border_b_1()
        .border_color(crate::theme::row_divider(&palette))
        .cursor_pointer()
        .when(selected, |row| row.bg(palette.selected))
        .hover(|row| row.bg(if selected { palette.selected_hover } else { palette.hover }))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(Label::new(commit.summary.clone()).font_weight(FontWeight::MEDIUM).overflow_hidden().whitespace_nowrap().text_ellipsis())
            .child(
              h_flex().gap_1p5().items_center().child(Avatar::new().name(initial).xsmall()).child(
                Label::new(format!("{} · {}", commit.author, ago_long(commit.time)))
                  .text_size(FONT_SIZE_SM)
                  .text_color(theme.muted_foreground)
                  .overflow_hidden()
                  .whitespace_nowrap()
                  .text_ellipsis(),
              ),
            ),
        )
        .children(
          commit
            .tags
            .iter()
            .take(2)
            .map(|tag| div().px_1p5().rounded_md().bg(palette.hover).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).child(tag.clone())),
        )
        .when(commit.unpushed, |row| {
          row.child(
            div()
              .id(SharedString::from(format!("unpushed-{}", commit.id)))
              .size_5()
              .flex()
              .flex_none()
              .items_center()
              .justify_center()
              .rounded_full()
              .bg(palette.hover)
              .tooltip(|window, cx| Tooltip::new("Not pushed yet").build(window, cx))
              .child(Icon::empty().path("icons/arrow-up.svg").size_4().text_color(theme.muted_foreground)),
          )
        })
        .on_click(cx.listener(move |this, _, _, cx| this.select_commit(id.clone(), cx)))
        .context_menu(move |menu, window, cx| {
          let (undo_workspace, undo_id) = (menu_workspace.clone(), menu_id.clone());
          let (tag_workspace, tag_id) = (menu_workspace.clone(), menu_id.clone());
          let copy_id = menu_id.clone();
          let (revert_workspace, revert_id) = (menu_workspace.clone(), menu_id.clone());
          let (branch_workspace, branch_id) = (menu_workspace.clone(), menu_id.clone());
          let (pick_workspace, pick_id, pick_root, pick_current) = (menu_workspace.clone(), menu_id.clone(), menu_root.clone(), menu_current.clone());
          menu
            .item(
              PopupMenuItem::new("Revert Changes in Commit…")
                .icon(Icon::empty().path("icons/undo-2.svg"))
                .on_click(move |_, window, cx| revert_workspace.update(cx, |this, cx| this.confirm_revert_commit(revert_id.clone(), window, cx))),
            )
            .item(
              PopupMenuItem::new("Create Branch from Commit…")
                .icon(Icon::empty().path("icons/git-branch.svg"))
                .on_click(move |_, window, cx| branch_workspace.update(cx, |this, cx| this.branch_from_commit(branch_id.clone(), window, cx))),
            )
            .submenu("Cherry-pick Commit to", window, cx, move |submenu, _, _| {
              let branches = ws_git::branch_entries(&pick_root).unwrap_or_default();
              branches.into_iter().filter(|entry| entry.remote.is_none() && entry.name != pick_current).fold(submenu, |submenu, entry| {
                let (workspace, id, branch) = (pick_workspace.clone(), pick_id.clone(), entry.name.clone());
                submenu.item(
                  PopupMenuItem::new(entry.name)
                    .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.cherry_pick_commit(id.clone(), branch.clone(), window, cx))),
                )
              })
            })
            .separator()
            .item(
              PopupMenuItem::new("Undo Commit…")
                .disabled(!undoable)
                .on_click(move |_, window, cx| undo_workspace.update(cx, |this, cx| this.undo_commit(undo_id.clone(), window, cx))),
            )
            .separator()
            .item(
              PopupMenuItem::new("Create Tag…")
                .on_click(move |_, window, cx| tag_workspace.update(cx, |this, cx| this.create_tag(tag_id.clone(), window, cx))),
            )
            .separator()
            .item(PopupMenuItem::new("Copy SHA").on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy_id.clone()))))
        })
    });
    // The next page of commits is loaded when the list is scrolled to its end (after the wheel stops), or with the button there.
    let more = (paged && self.history.len() >= self.history_limit).then(|| {
      Button::new("history-more")
        .ghost()
        .small()
        .label(format!("Load {HISTORY_PAGE} more commits"))
        .on_click(cx.listener(|this, _, _, cx| this.load_more_history(false, cx)))
    });
    // The scrollbar has a strip of its own beside the list, so that it never covers the rows.
    h_flex()
      .flex_1()
      .min_h_0()
      .bg(colors(cx).background)
      .child(
        div().relative().flex_1().min_w_0().h_full().key_context("HistoryList").track_focus(&self.history_focus).child(
          v_flex()
            .id("history-list")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.history_scroll)
            .on_scroll(cx.listener(move |_, _: &ScrollEvent, _, cx| {
              if !paged {
                return;
              }
              cx.spawn(async move |this, cx| {
                cx.background_executor().timer(HISTORY_SCROLL_SETTLE).await;
                let _ = this.update(cx, |this, cx| this.load_more_history(true, cx));
              })
              .detach();
            }))
            .children(rows)
            .children(more),
        ),
      )
      // The strip is shown only while the rows are taller than the list.
      .when(self.history_scroll.max_offset().y > px(0.), |row| {
        row.child(crate::components::scrollbar_strip(&self.history_scroll, "history-scrollbar", cx))
      })
      .into_any_element()
  }

  /// The field above the History list: choose a branch to compare with, or the branch chosen, with a button to stop comparing.
  pub(super) fn render_compare_field(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let branch_icon = || Icon::empty().path("icons/git-branch.svg").size_4().text_color(theme.muted_foreground);
    let field = match &self.compare {
      Some(view) => h_flex()
        .h_8()
        .px_2()
        .gap_2()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(palette.border)
        .child(branch_icon())
        .child(Label::new(view.branch.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
        .child(
          Button::new("compare-clear")
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/x.svg").size_4())
            .tooltip("Stop comparing")
            .on_click(cx.listener(|this, _, _, cx| this.clear_compare(cx))),
        )
        .into_any_element(),
      None => {
        // The close button sits inside the field, at its right edge.
        let input = crate::components::soft_input(&self.compare_filter, cx).prefix(branch_icon()).when(self.compare_open, |input| {
          input.suffix(
            Button::new("compare-close")
              .ghost()
              .xsmall()
              .icon(Icon::empty().path("icons/x.svg").size_4())
              .tooltip("Close")
              .on_click(cx.listener(|this, _, window, cx| this.close_compare_list(window, cx))),
          )
        });
        div().w_full().child(input).into_any_element()
      }
    };
    div().child(field).into_any_element()
  }

  /// The branches under the compare field: the default one, the ones used lately and all the others, each with how many commits
  /// it is behind (↓) and ahead (↑) of the current branch.
  pub(super) fn render_compare_choices(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    if self.compare_choices.is_none() {
      return h_flex()
        .gap_2()
        .p_3()
        .items_center()
        .justify_center()
        .child(Spinner::new().small())
        .child(Label::new("Reading branches…").text_color(theme.muted_foreground))
        .into_any_element();
    }
    let groups = self.matching_compare_branches(cx);
    if groups.default.is_none() && groups.favorites.is_empty() && groups.recent.is_empty() && groups.other.is_empty() {
      return Label::new("No matching branches").px_2().py_1().text_color(theme.muted_foreground).into_any_element();
    }
    let heading = |text: &'static str| Label::new(text).px_3().pt_2().pb_1().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground);
    let row = |branch: &BranchRef, cx: &mut Context<Self>| {
      let name = branch.name.clone();
      h_flex()
        .id(SharedString::from(format!("compare-branch-{}", branch.name)))
        .gap_2()
        .px_3()
        .py_1p5()
        .items_center()
        .cursor_pointer()
        .hover(|row| row.bg(palette.hover))
        .child(Icon::empty().path("icons/git-branch.svg").size_4().text_color(theme.muted_foreground))
        .child(Label::new(branch.name.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
        .child(Label::new(format!("{}↓ {}↑", branch.behind, branch.ahead)).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
        .on_click(cx.listener(move |this, _, window, cx| this.choose_compare_branch(name.clone(), window, cx)))
        .into_any_element()
    };
    let mut list = v_flex()
      .id("compare-list")
      .size_full()
      .overflow_y_scroll()
      .track_scroll(&self.compare_scroll)
      .when(groups.default.is_some(), |list| list.pt_2());
    // The default branch is the first row and needs no heading, as in the branch menu.
    if let Some(branch) = &groups.default {
      list = list.child(row(branch, cx));
    }
    for (label, branches) in [("Favorites", &groups.favorites), ("Recent Branches", &groups.recent), ("Other Branches", &groups.other)] {
      if !branches.is_empty() {
        list = list.child(heading(label)).children(branches.iter().map(|branch| row(branch, cx)).collect::<Vec<_>>());
      }
    }
    // The scrollbar has a strip of its own beside the list, as in History, shown only while the rows are taller than the list.
    h_flex()
      .flex_1()
      .min_h_0()
      .bg(palette.background)
      .child(div().flex_1().min_w_0().h_full().child(list))
      .when(self.compare_scroll.max_offset().y > px(0.), |row| {
        row.child(crate::components::scrollbar_strip(&self.compare_scroll, "compare-scrollbar", cx))
      })
      .into_any_element()
  }

  /// The comparison with the chosen branch: its two sides as tabs, the commits of the shown side, and what merging would do.
  pub(super) fn render_compare(&self, view: &CompareView, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let current = self.git.as_ref().map(|git| git.branch.clone()).unwrap_or_default();
    let (behind, ahead) = view.found.as_ref().map_or((0, 0), |found| (found.behind_count, found.ahead_count));
    let tabs = TabBar::new("compare-sides")
      .underline()
      .selected_index(usize::from(view.side == CompareSide::Ahead))
      .on_click(
        cx.listener(|this, index: &usize, _, cx| this.set_compare_side(if *index == 1 { CompareSide::Ahead } else { CompareSide::Behind }, cx)),
      )
      .children([
        Tab::new().cursor_pointer().flex_1().justify_center().label(format!("Behind ({behind})")),
        Tab::new().cursor_pointer().flex_1().justify_center().label(format!("Ahead ({ahead})")),
      ]);
    let list = match &view.found {
      None => h_flex()
        .gap_2()
        .p_3()
        .items_center()
        .justify_center()
        .child(Spinner::new().small())
        .child(Label::new("Comparing…").text_color(theme.muted_foreground))
        .into_any_element(),
      Some(found) => {
        let commits = match view.side {
          CompareSide::Behind => &found.behind,
          CompareSide::Ahead => &found.ahead,
        };
        div().flex_1().min_h_0().flex().flex_col().child(self.commit_list(commits, false, cx)).into_any_element()
      }
    };
    let bold = |text: String| Label::new(text).font_weight(FontWeight::SEMIBOLD);
    let note = |parts: Vec<AnyElement>| {
      h_flex().flex_wrap().justify_center().gap_1().text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).children(parts)
    };
    let footer = view.found.as_ref().map(|found| {
      let conflicts = found.conflict_count.unwrap_or_default();
      let message = if conflicts > 0 {
        note(vec![
          bold(format!("{conflicts} conflicted file{}", if conflicts == 1 { "" } else { "s" })).into_any_element(),
          Label::new(format!("when merging {} into {current}", view.branch)).into_any_element(),
        ])
      } else if behind > 0 {
        note(vec![
          Label::new("This will merge").into_any_element(),
          bold(format!("{behind} commit{}", if behind == 1 { "" } else { "s" })).into_any_element(),
          Label::new("from").into_any_element(),
          bold(view.branch.clone()).into_any_element(),
          Label::new("into").into_any_element(),
          bold(current.clone()).into_any_element(),
        ])
      } else {
        note(vec![Label::new("This branch is up to date with").into_any_element(), bold(view.branch.clone()).into_any_element()])
      };
      v_flex()
        .gap_2()
        .px_3()
        .pb_3()
        .pt_2()
        .items_center()
        .child(if conflicts > 0 {
          Icon::empty().path("icons/triangle-alert.svg").size_4().text_color(palette.conflicted)
        } else {
          Icon::new(IconName::Check).size_4().text_color(palette.new)
        })
        .child(message)
        .when(behind > 0, |footer| {
          footer.child(
            div().w_full().child(
              Button::new("compare-merge")
                .primary()
                .w_full()
                .label(if self.git.as_ref().is_some_and(|git| git.merging) { "Resolve merge conflicts" } else { "Create a merge commit" })
                .loading(self.merge_task.is_some())
                .disabled(self.merge_task.is_some() || self.remote_work.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.merge_compare(window, cx))),
            ),
          )
        })
    });
    v_flex().flex_1().min_h_0().gap_2().child(div().px_3().child(tabs)).child(list).children(footer).into_any_element()
  }
}
