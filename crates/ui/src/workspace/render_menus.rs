//! Status bar, branch picker and repository switcher.

use super::*;
use gpui_kit::component::text::TextView;

impl Workspace {
  /// Shows repository status, the selectable project path, and the open file status.
  pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let file = self.editor_panel.read(cx).status(cx);
    let mut bar = StatusBar::new().bg(palette.surface_alt).border_color(palette.border).text_color(palette.text_secondary);
    if let Some(git) = &self.git {
      bar = bar
        .left(
          Button::new("status-branch")
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/git-branch.svg"))
            .label(git.branch.clone())
            .tooltip("Current branch"),
        )
        .left(if git.changed() == 0 { "No changes".to_string() } else { format!("{} changed", git.changed()) });
    }
    if let Some(path) = &self.projects.active {
      let path_text = display_path(path).replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
      bar = bar.child(
        TextView::html(SharedString::from(format!("status-project-{}", path.display())), format!("<p>{path_text}</p>")).selectable(true).max_lines(1),
      );
    }
    for item in file {
      bar = bar.right(item);
    }
    bar
  }

  /// The button that shows the current branch and opens the branch menu (see `render_branch_menu`).
  pub(super) fn render_branch_picker(&self, current: &str, cx: &mut Context<Self>) -> impl IntoElement {
    let workspace = cx.entity();
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let current = SharedString::from(current.to_string());
    let open = self.branch_menu_open;
    Button::new("branch-trigger")
      .custom(ButtonCustomVariant::new(cx).color(palette.surface).foreground(palette.text).hover(palette.hover).active(palette.hover))
      .selected(open)
      .cursor_pointer()
      .border_0()
      .h_full()
      .w_48()
      .rounded_none()
      .child(
        h_flex()
          .w_full()
          .gap_2()
          .items_center()
          .child(Icon::empty().path("icons/git-branch.svg").size_5().text_color(theme.muted_foreground))
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .items_start()
              .child(Label::new("Current branch").text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
              .child(
                Label::new(current.clone()).font_weight(FontWeight::SEMIBOLD).max_w_full().overflow_hidden().whitespace_nowrap().text_ellipsis(),
              ),
          )
          .child(Icon::new(IconName::ChevronDown).size_4().text_color(theme.muted_foreground)),
      )
      .tooltip("Switch branch")
      .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.set_branch_menu(!open, window, cx)))
  }

  /// The branch menu: the branches in sections (favorites, default, recent, others), and the pull requests when a connected
  /// account serves the repository. It floats over the window like the project menu, rounded, right under the branch button's slot
  /// in the Git bar; a click outside it, or Escape, closes it. It belongs to no popover, so nothing else decides where it goes: the
  /// position comes from the slot's measured bounds.
  pub(super) fn render_branch_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
    if !self.branch_menu_open {
      return None;
    }
    let workspace = cx.entity();
    let current = SharedString::from(self.git.as_ref()?.branch.to_string());
    let slot = self.branch_trigger_bounds.get();
    let theme = cx.theme().clone();
    let panel = {
      let theme = cx.theme().clone();
      let palette = colors(cx);
      let (filter, query, branches, tab, hosted, state, starred) = {
        let this = self;
        let starred: Vec<String> = this.projects.active.as_ref().map(|path| this.projects.favorite_branches(path).to_vec()).unwrap_or_default();
        (
          this.branch_filter.clone(),
          this.branch_filter.read(cx).value().to_string(),
          this.branches.clone(),
          this.branch_tab,
          this.hosted_remote(cx).map(|(_, repository)| repository.full_name()),
          this.pull_requests.clone(),
          starred,
        )
      };
      // A floating menu like the project menu: it fits its content up to a cap, and never runs past the window's bottom edge.
      let menu_height = (window.viewport_size().height - slot.bottom() - BAR_BORDER - MENU_GAP * 2.).max(MENU_LIST_MAX_HEIGHT);
      let lowered = query.trim().to_lowercase();
      let tab = if hosted.is_some() { tab } else { BranchTab::Branches };

      // The tabs: Branches, and Pull Requests when a connected account serves the repository.
      let count = match &state {
        PullRequests::Loaded(found) if !found.is_empty() => format!(" ({})", found.len()),
        _ => String::new(),
      };
      let tabs = hosted.as_ref().map(|_| {
        let workspace = workspace.clone();
        TabBar::new("branch-menu-tabs")
          .underline()
          .selected_index(usize::from(tab == BranchTab::PullRequests))
          .on_click(move |index: &usize, window, cx| {
            let tab = if *index == 1 { BranchTab::PullRequests } else { BranchTab::Branches };
            workspace.update(cx, |this, cx| this.set_branch_tab(tab, window, cx));
          })
          .children([Tab::new().cursor_pointer().label("Branches"), Tab::new().cursor_pointer().label(format!("Pull Requests{count}"))])
      });

      // The list takes what the header and the tabs leave of the cap.
      let list_height = (menu_height - px(64.)).min(BRANCH_LIST_MAX_HEIGHT);
      let body = match tab {
        BranchTab::Branches => {
          // Sections: the default branch, the starred branches, the five most recently changed, then the rest.
          let visible: Vec<ws_git::BranchEntry> =
            branches.into_iter().filter(|entry| lowered.is_empty() || entry.name.to_lowercase().contains(&lowered)).collect();
          // The default branch is always the first section, even when it is starred.
          let (defaults, rest): (Vec<_>, Vec<_>) = visible.into_iter().partition(|entry| entry.default);
          let (favorites, rest): (Vec<_>, Vec<_>) = rest.into_iter().partition(|entry| starred.contains(&entry.name));
          let mut newest: Vec<&ws_git::BranchEntry> = rest.iter().collect();
          newest.sort_by_key(|entry| std::cmp::Reverse(entry.time));
          let recent_names: Vec<String> = newest.into_iter().take(RECENT_BRANCHES).map(|entry| entry.name.clone()).collect();
          let (mut recent, mut others): (Vec<_>, Vec<_>) = rest.into_iter().partition(|entry| recent_names.contains(&entry.name));
          // The newest first, in the recent branches and among the others; a branch with no known time goes last.
          recent.sort_by_key(|entry| std::cmp::Reverse(entry.time));
          others.sort_by_key(|entry| std::cmp::Reverse(entry.time));
          let make_row = |entry: &ws_git::BranchEntry| -> AnyElement {
            let ws_git::BranchEntry { name, remote, time, .. } = entry.clone();
            let favorite = starred.contains(&name);
            let (star_workspace, star_name) = (workspace.clone(), name.clone());
            let selected = name == current.as_ref() && remote.is_none();
            let workspace = workspace.clone();
            let menu_workspace = workspace.clone();
            let (menu_name, menu_remote) = (name.clone(), remote.clone());
            h_flex()
              .id(SharedString::from(format!("branch-{name}")))
              .gap_2()
              .px_2()
              .py_1()
              .rounded_md()
              .items_center()
              .cursor_pointer()
              .when(selected, |row| row.bg(palette.selected))
              .hover(|row| row.bg(palette.hover))
              // The star, at the left: filled for a starred branch, an outline otherwise.
              .child(
                div()
                  .id(SharedString::from(format!("branch-star-{name}")))
                  .size_6()
                  .flex()
                  .flex_none()
                  .items_center()
                  .justify_center()
                  .rounded_md()
                  .cursor_pointer()
                  .hover(|star| star.bg(palette.hover))
                  .when(!favorite, |star| star.opacity(0.5))
                  .tooltip(move |window, cx| Tooltip::new(if favorite { "Remove from favorites" } else { "Add to favorites" }).build(window, cx))
                  .child(Icon::empty().path(if favorite { "icons/star-fill.svg" } else { "icons/star.svg" }).size_4().text_color(if favorite {
                    palette.modified
                  } else {
                    theme.muted_foreground
                  }))
                  .on_click(move |_, _, cx| {
                    // The click must not also switch the branch.
                    cx.stop_propagation();
                    star_workspace.update(cx, |this, cx| this.toggle_favorite_branch(&star_name, cx));
                  }),
              )
              .child(
                Label::new(name.clone()).font_weight(FontWeight::MEDIUM).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis(),
              )
              // When work last landed on the branch: its latest commit.
              .when_some(time, |row, time| {
                row.child(Label::new(ago_long(time)).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).flex_none())
              })
              .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.switch_branch(name.clone(), window, cx)))
              .context_menu(move |menu, _, _| {
                let (rename_workspace, rename_name) = (menu_workspace.clone(), menu_name.clone());
                let (delete_workspace, delete_name, delete_remote) = (menu_workspace.clone(), menu_name.clone(), menu_remote.clone());
                menu
                  .item(
                    PopupMenuItem::new("Rename…")
                      .icon(Icon::empty().path("icons/pencil.svg"))
                      .disabled(menu_remote.is_some())
                      .on_click(move |_, window, cx| rename_workspace.update(cx, |this, cx| this.rename_branch(rename_name.clone(), window, cx))),
                  )
                  .separator()
                  .item(PopupMenuItem::new("Delete…").icon(Icon::empty().path("icons/trash.svg")).on_click(move |_, window, cx| {
                    delete_workspace.update(cx, |this, cx| this.confirm_delete_branch(delete_name.clone(), delete_remote.clone(), window, cx))
                  }))
              })
              .into_any_element()
          };
          let mut rows: Vec<AnyElement> = Vec::new();
          let sections = [("Default Branch", defaults), ("Favorites", favorites), ("Recent Branches", recent), ("Other Branches", others)];
          for (label, entries) in sections {
            if entries.is_empty() {
              continue;
            }
            rows.push(
              Label::new(label)
                .px_2()
                .pt_2()
                .text_size(FONT_SIZE_SM)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .into_any_element(),
            );
            rows.extend(entries.iter().map(&make_row));
          }
          let new_branch = {
            let (workspace, suggestion) = (workspace.clone(), query.trim().to_string());
            Button::new("branch-create")
              .outline()
              .label("New Branch")
              .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.new_branch(suggestion.clone(), window, cx)))
          };
          v_flex()
            .gap_1()
            .min_h_0()
            .child(
              h_flex().gap_2().items_center().p_3().pb_0().child(div().flex_1().child(crate::components::soft_input(&filter, cx))).child(new_branch),
            )
            .child(
              div().id("branch-list").max_h(list_height).overflow_y_scrollbar().child(
                v_flex()
                  .gap_0p5()
                  .px_3()
                  .pb_3()
                  .when(rows.is_empty(), |list| list.child(Label::new("No matching branches").px_2().py_1().text_color(theme.muted_foreground)))
                  .children(rows),
              ),
            )
            .into_any_element()
        }
        BranchTab::PullRequests => {
          let refresh = {
            let workspace = workspace.clone();
            Button::new("pull-requests-refresh")
              .outline()
              .icon(Icon::empty().path("icons/refresh-cw.svg"))
              .tooltip("Refresh")
              .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.load_pull_requests(cx)))
          };
          let list = match state {
            PullRequests::Idle | PullRequests::Loading => h_flex()
              .gap_2()
              .p_3()
              .items_center()
              .justify_center()
              .child(Spinner::new().small())
              .child(Label::new("Loading pull requests…").text_color(theme.muted_foreground))
              .into_any_element(),
            PullRequests::Failed(message) => Label::new(message).p_2().text_color(theme.danger).into_any_element(),
            PullRequests::Loaded(found) => {
              let shown: Vec<_> = found
                .iter()
                .filter(|request| {
                  lowered.is_empty()
                    || request.title.to_lowercase().contains(&lowered)
                    || request.author.to_lowercase().contains(&lowered)
                    || format!("#{}", request.number).contains(&lowered)
                })
                .cloned()
                .collect();
              if found.is_empty() {
                // The empty state: nothing to do here, and a way to get going.
                let workspace = workspace.clone();
                v_flex()
                  .items_center()
                  .gap_1()
                  .py_4()
                  .child(Icon::empty().path("icons/circle-check.svg").size_8().text_color(palette.accent))
                  .child(Label::new("You're all set!").font_weight(FontWeight::SEMIBOLD))
                  .child(Label::new(format!("No open pull requests in {}", hosted.clone().unwrap_or_default())).text_color(theme.muted_foreground))
                  .child(
                    Label::new("Would you like to create a new branch and get going on your next project?")
                      .text_size(FONT_SIZE_SM)
                      .text_color(theme.muted_foreground)
                      .text_center()
                      .px_3(),
                  )
                  .child(
                    Button::new("pull-requests-new-branch")
                      .ghost()
                      .small()
                      .label("Create a new branch")
                      .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.new_branch(String::new(), window, cx))),
                  )
                  .into_any_element()
              } else if shown.is_empty() {
                Label::new("No matching pull requests").px_2().py_1().text_color(theme.muted_foreground).into_any_element()
              } else {
                div()
                  .id("pull-request-list")
                  .max_h(menu_height)
                  .overflow_y_scrollbar()
                  .child(v_flex().gap_0p5().px_3().children(shown.into_iter().map(|request| {
                    let url = request.url.clone();
                    v_flex()
                      .id(SharedString::from(format!("pull-request-{}", request.url)))
                      .px_2()
                      .py_1()
                      .rounded_md()
                      .cursor_pointer()
                      .hover(|row| row.bg(palette.hover))
                      .child(
                        Label::new(if request.draft { format!("{} (draft)", request.title) } else { request.title.clone() })
                          .overflow_hidden()
                          .whitespace_nowrap()
                          .text_ellipsis(),
                      )
                      .child(
                        Label::new(format!("#{} · {} · {} → {}", request.number, request.author, request.source, request.target))
                          .text_size(FONT_SIZE_SM)
                          .text_color(theme.muted_foreground)
                          .overflow_hidden()
                          .whitespace_nowrap()
                          .text_ellipsis(),
                      )
                      .on_click(move |_, _, cx| cx.open_url(&url))
                  })))
                  .into_any_element()
              }
            }
          };
          let create = {
            let workspace = workspace.clone();
            Button::new("pull-request-create")
              .outline()
              .w_full()
              .icon(Icon::empty().path("icons/git-pull-request-create.svg"))
              .label("Create pull request…")
              .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.open_pull_request_form(window, cx)))
          };
          v_flex()
            .gap_2()
            .child(h_flex().gap_2().items_center().px_3().child(div().flex_1().child(crate::components::soft_input(&filter, cx))).child(refresh))
            .child(div().child(list))
            .child(div().h_px().bg(palette.border))
            .child(div().px_3().pb_3().child(create))
            .into_any_element()
        }
      };
      v_flex().w(MENU_WIDTH).max_h(menu_height).gap_1().children(tabs.map(|tabs| div().px_3().pt_3().child(tabs))).child(body)
    };
    Some(
      deferred(
        div()
          .absolute()
          .inset_0()
          .occlude()
          .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.set_branch_menu(false, window, cx)))
          .child(
            div()
              .id("branch-menu")
              .occlude()
              .absolute()
              .left(slot.left())
              .top(slot.bottom() + BAR_BORDER + MENU_GAP)
              .bg(theme.popover)
              .border_1()
              .border_color(theme.border)
              .rounded_lg()
              .shadow_lg()
              .overflow_hidden()
              .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
              .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                  this.set_branch_menu(false, window, cx);
                  cx.stop_propagation();
                }
              }))
              .child(panel),
          ),
      )
      .with_priority(1)
      .into_any_element(),
    )
  }

  /// The title bar's project menu, grouped by category and filtered by typing, with a way to add a project.
  pub(super) fn render_repository_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let workspace = cx.entity();
    let theme = cx.theme().clone();
    let trigger = project_bar_button("repository-trigger", self.projects_menu_open, cx).tooltip("Projects").child(
      h_flex().gap_2().items_center().child(Icon::empty().path("icons/folder.svg").size_5()).child(
        Icon::new(if self.projects_menu_open { IconName::ChevronUp } else { IconName::ChevronDown }).size_4().text_color(theme.muted_foreground),
      ),
    );
    // The popover has no padding of its own: the list's scrollbar then runs along the menu's right edge, and the padding sits
    // inside the header and inside the list.
    Popover::new("repository-menu")
      .p_0()
      .anchor(Anchor::TopLeft)
      .open(self.projects_menu_open)
      .on_open_change({
        let workspace = workspace.clone();
        move |open, window, cx| workspace.update(cx, |this, cx| this.set_projects_menu(*open, window, cx))
      })
      .trigger(trigger)
      .content(move |_, _, cx| {
        let theme = cx.theme().clone();
        let (filter, query, list, statuses) = {
          let this = workspace.read(cx);
          (this.project_filter.clone(), this.project_filter.read(cx).value().to_lowercase(), this.projects.clone(), this.project_status.clone())
        };
        let active = list.active.clone();
        let matches = |project: &&Project| query.is_empty() || project.name().to_lowercase().contains(&query);
        // The starred projects come first, and are not listed again under their category.
        let mut groups: Vec<(SharedString, Vec<&Project>)> = vec![(SharedString::from("Favorites"), list.favorites().filter(matches).collect())];
        groups.extend(list.categories.iter().enumerate().map(|(index, category)| {
          (
            SharedString::from(category_label(index, category)),
            list.projects_in(Some(&category.id)).filter(|project| !project.favorite).filter(matches).collect(),
          )
        }));
        groups.push((SharedString::from("No category"), list.projects_in(None).filter(|project| !project.favorite).filter(matches).collect()));
        groups.retain(|(_, projects)| !projects.is_empty());
        let named = groups.len() > 1 || !list.categories.is_empty() || list.favorites().next().is_some();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (label, projects) in groups {
          if named {
            rows.push(
              Label::new(label)
                .px_2()
                .pt_2()
                .text_size(FONT_SIZE_SM)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .into_any_element(),
            );
          }
          rows.extend(
            projects
              .into_iter()
              .map(|project| project_list_row(&workspace, project, active.as_ref() == Some(&project.path), statuses.get(&project.path), cx)),
          );
        }
        let add = {
          let workspace = workspace.clone();
          Button::new("repository-add").outline().label("Add").icon(IconName::Plus).on_click(move |_, window, cx| {
            workspace.update(cx, |this, cx| {
              this.projects_menu_open = false;
              cx.notify();
            });
            window.dispatch_action(Box::new(AddProject), cx);
          })
        };
        // Sorting, grouping and renaming projects is done in Settings → Project list.
        let sort = {
          let workspace = workspace.clone();
          Button::new("repository-sort")
            .outline()
            .icon(Icon::empty().path("icons/arrow-up-down.svg"))
            .tooltip("Sort and group the projects (Settings → Project list)")
            .on_click(move |_, window, cx| {
              workspace.update(cx, |this, cx| {
                this.projects_menu_open = false;
                this.open_settings_page("Project list", window, cx);
              })
            })
        };
        v_flex()
          .w(PROJECT_MENU_WIDTH)
          .gap_1()
          .child(
            h_flex()
              .gap_2()
              .items_center()
              .p_3()
              .pb_0()
              .child(div().flex_1().child(crate::components::soft_input(&filter, cx)))
              .child(add)
              .child(sort),
          )
          .child(
            div().id("repository-list").max_h(MENU_LIST_MAX_HEIGHT).overflow_y_scrollbar().child(
              v_flex()
                .gap_0p5()
                .px_3()
                .pb_3()
                .when(rows.is_empty(), |list| list.child(Label::new("No matching projects").px_2().py_1().text_color(theme.muted_foreground)))
                .children(rows),
            ),
          )
      })
  }
}
