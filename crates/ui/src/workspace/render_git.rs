//! Favorite projects, Git toolbar, history and compare views.

use super::*;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::ElementExt as _;

/// The fixed height of a line in the Changes list: the virtualized list needs every row's height before it draws.
const CHANGE_ROW_HEIGHT: Pixels = px(32.);

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
        div()
          .id(SharedString::from(format!("favorite-drag-{}", path.display())))
          // The tab itself slides along the bar, so nothing follows the pointer up or down.
          .when(held, |tab| tab.opacity(0.85).shadow_md())
          .on_drag(DraggedProjectTab(path.clone()), move |_, _, _, cx| {
            picked_up.update(cx, |this, _| this.dragged_tab = Some(picked.clone()));
            cx.new(|_| NoDragPreview)
          })
          .on_drag_move::<DraggedProjectTab>(move |event, _, cx| {
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
                  this.swap_projects(&dragged, &target, cx);
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
          })
          .into_any_element()
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
        format!("Send it to {}", sync.remote)
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

  /// One line of the Changes list: a checkbox, the file name with its folder, and the kind of change. Its height is fixed
  /// ([`CHANGE_ROW_HEIGHT`]) because the list is virtualized, and its width is the list's own: a virtual list lays its rows out
  /// at their content width, so a percentage would leave short rows narrow and long ones unclipped.
  fn change_row(&self, change: &ws_git::Change, width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let workspace = cx.entity();
    let root = self.projects.active.clone().unwrap_or_default();
    let file = change.path.clone();
    let selected = self.selected_change.as_ref() == Some(&change.path);
    let included = !self.excluded.contains(&change.path);
    h_flex()
      .id(SharedString::from(format!("change-row-{}", change.path.display())))
      .when(width.is_zero(), |row| row.w_full())
      .when(!width.is_zero(), |row| row.w(width))
      .min_w_0()
      .overflow_hidden()
      .h(CHANGE_ROW_HEIGHT)
      .gap_1p5()
      .px_3()
      .border_b_1()
      .border_color(palette.border)
      .items_center()
      .when(selected, |row| row.bg(palette.selected))
      .hover(|row| row.bg(palette.hover))
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
            // One path, the file name last, cut in the middle when the row is narrow: the room is the row less its padding,
            // the checkbox, the status icon and the gaps.
            div().flex_1().min_w_0().child(
              Label::new(crate::components::shortened_path(&change.path.to_string_lossy(), width.as_f32() - 88.))
                .min_w_0()
                .whitespace_nowrap()
                .overflow_hidden(),
            ),
          )
          .child(Icon::empty().path(change_kind_icon(change.kind)).size_4().flex_none().text_color(kind_color(change.kind, &palette)))
          .on_click(cx.listener({
            let file = file.clone();
            // A double click opens the file itself, in the editor with the Explorer; one click shows its diff.
            move |this, event: &ClickEvent, window, cx| {
              if event.click_count() >= 2 {
                this.open_change_in_editor(&file, window, cx);
              } else {
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
      let changes: Vec<ws_git::Change> =
        this.visible_changes(&query).get(range).map(|rows| rows.iter().map(|change| (*change).clone()).collect()).unwrap_or_default();
      let width = this.changes_scroll.bounds().size.width;
      changes.iter().map(|change| this.change_row(change, width, cx)).collect()
    })
    .track_scroll(&self.changes_scroll);
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
          .child(
            h_flex()
              .absolute()
              .right_1()
              .bottom_1()
              .gap_0p5()
              .child(
                Button::new("commit-settings")
                  .custom(ButtonCustomVariant::new(cx).color(palette.surface).foreground(palette.text).hover(palette.hover).active(palette.hover))
                  .cursor_pointer()
                  .small()
                  .icon(Icon::empty().path("icons/settings-2.svg"))
                  .tooltip("commit message settings")
                  .on_click(cx.listener(|this, _, window, cx| this.open_commit_settings(window, cx))),
              )
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
              ),
          ),
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
          div().relative().flex_1().min_h_0().when(self.switching_branch.is_some(), |area| area.child(crate::components::skeleton_list(8))).when(
            git.changed() > 0 && self.switching_branch.is_none(),
            |list| {
              list.child(changes_list).child(
                div().absolute().inset_0().child(
                  Scrollbar::new(&self.changes_scroll)
                    .id("changes-scrollbar")
                    .axis(ScrollbarAxis::Vertical)
                    .mode(ScrollbarMode::Always)
                    .viewport_from_layout(),
                ),
              )
            },
          ),
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
        .hover(|row| row.bg(palette.hover))
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
    div()
      .relative()
      .flex_1()
      .min_h_0()
      .child(
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
      )
      .child(div().absolute().inset_0().child(
        Scrollbar::new(&self.history_scroll).id("history-scrollbar").axis(ScrollbarAxis::Vertical).mode(ScrollbarMode::Always).viewport_from_layout(),
      ))
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
    let (default, recent, other) = self.matching_compare_branches(cx);
    if default.is_none() && recent.is_empty() && other.is_empty() {
      return Label::new("No matching branches").px_2().py_1().text_color(theme.muted_foreground).into_any_element();
    }
    let heading = |text: &'static str| Label::new(text).px_2().pt_2().pb_1().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground);
    let row = |branch: &BranchRef, cx: &mut Context<Self>| {
      let name = branch.name.clone();
      h_flex()
        .id(SharedString::from(format!("compare-branch-{}", branch.name)))
        .gap_2()
        .px_2()
        .py_1p5()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|row| row.bg(palette.hover))
        .child(Icon::empty().path("icons/git-branch.svg").size_4().text_color(theme.muted_foreground))
        .child(Label::new(branch.name.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
        .child(Label::new(format!("{}↓ {}↑", branch.behind, branch.ahead)).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
        .on_click(cx.listener(move |this, _, window, cx| this.choose_compare_branch(name.clone(), window, cx)))
        .into_any_element()
    };
    let mut list = v_flex().id("compare-list").flex_1().min_h_0().pr_3().overflow_y_scrollbar();
    if let Some(branch) = &default {
      list = list.child(heading("Default Branch")).child(row(branch, cx));
    }
    if !recent.is_empty() {
      list = list.child(heading("Recent Branches")).children(recent.iter().map(|branch| row(branch, cx)).collect::<Vec<_>>());
    }
    if !other.is_empty() {
      list = list.child(heading("Other Branches")).children(other.iter().map(|branch| row(branch, cx)).collect::<Vec<_>>());
    }
    list.into_any_element()
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
