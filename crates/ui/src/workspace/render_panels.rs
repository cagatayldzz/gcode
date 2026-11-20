//! Explorer, sidebar, activity bar, terminals and AI agent panes.

use super::*;

impl Workspace {
  /// One line of the folder tree.
  pub(super) fn explorer_row(&self, row: &explorer::Row, font_size: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let path = row.path.clone();
    let selected = self.selected_entry.as_ref() == Some(&row.path);
    let dim = if row.ignored && !selected { 0.55 } else { 1. };
    let icon_color = if selected { palette.explorer_selected_text } else { theme.muted_foreground };
    let open = self.expanded.contains(&row.path);
    let menu = self.explorer_menu(Some(row), cx);
    let highlighted_guide = self
      .hovered_folder
      .as_ref()
      .and_then(|(folder, depth)| (row.depth > *depth && row.path.starts_with(folder) && self.expanded.contains(folder)).then_some(*depth));
    h_flex()
      .id(SharedString::from(format!("entry-{}", row.path.display())))
      .w_full()
      .min_h_6()
      .gap_1p5()
      .pr_2()
      .py_0p5()
      .pl(INDENT * row.depth as f32 + rems(0.5))
      .relative()
      .rounded_sm()
      .items_center()
      .cursor_pointer()
      .when(selected, |entry| entry.bg(palette.explorer_selected))
      .when(!selected, |entry| entry.hover(|entry| entry.bg(palette.hover)))
      .child(div().size_4().flex_none().when(row.is_dir, |chevron| {
        chevron.child(Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight }).size_4().text_color(icon_color))
      }))
      .when(row.is_dir, |entry| entry.child(Icon::new(IconName::FolderClosed).size_4().text_color(icon_color).opacity(dim)))
      .when(!row.is_dir, |entry| entry.child(explorer_file_icon(&row.path, &palette).opacity(dim)))
      .child(
        Label::new(row.name.clone())
          .opacity(dim)
          .text_size(font_size)
          .line_height(relative(1.25))
          .when(selected, |label| label.text_color(palette.explorer_selected_text))
          .flex_1()
          .min_w_0()
          .overflow_hidden()
          .whitespace_nowrap()
          .text_ellipsis(),
      )
      .children((0..row.depth).map(|depth| {
        let opacity = if highlighted_guide == Some(depth) { 0.5 } else { 0.15 };
        div().absolute().top_0().bottom_0().left(INDENT * (depth + 1) as f32).w_px().bg(palette.control_border.opacity(opacity))
      }))
      .when(row.is_dir, |entry| {
        let folder = path.clone();
        let depth = row.depth;
        entry.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
          if *hovered {
            this.hovered_folder = Some((folder.clone(), depth));
            cx.notify();
          } else if this.hovered_folder.as_ref().is_some_and(|(path, _)| *path == folder) {
            this.hovered_folder = None;
            cx.notify();
          }
        }))
      })
      .when(!row.is_dir, |entry| {
        let hovered_path = path.clone();
        entry.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
          if !*hovered {
            if this.hovered_file.as_ref() == Some(&hovered_path) {
              this.hovered_file = None;
            }
            return;
          }
          this.hovered_file = Some(hovered_path.clone());
          let wanted = hovered_path.clone();
          cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(HOVER_PREPARE_DELAY).await;
            let _ = this.update_in(cx, |this, window, cx| {
              if this.hovered_file.as_ref() == Some(&wanted) {
                this.editor_panel.update(cx, |editor, cx| editor.prefetch(&wanted, window, cx));
              }
            });
          })
          .detach();
        }))
      })
      .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
        if this.rows.iter().any(|row| row.path == path && row.is_dir) {
          this.selected_entry = Some(path.clone());
          this.toggle_folder(path.clone(), cx);
        } else {
          // A single click previews the file; a double click keeps it.
          this.open_file_as(path.clone(), event.click_count() < 2, window, cx);
        }
      }))
      .context_menu(menu)
  }

  pub(super) fn render_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
    // Only the lines in view are built, laid out and drawn: a project can have thousands of entries.
    let list = uniform_list(
      "explorer-rows",
      self.rows.len(),
      cx.processor(|this, range: Range<usize>, _, cx| {
        let rows: Vec<explorer::Row> = this.rows.get(range).map(<[explorer::Row]>::to_vec).unwrap_or_default();
        let font_size = px(this.settings.read(cx).preferences.explorer_font_size as f32);
        // Inset the rows and keep the scrollbar at the edge of the sidebar.
        rows.iter().map(|row| div().w_full().px_1p5().child(this.explorer_row(row, font_size, cx)).into_any_element()).collect::<Vec<_>>()
      }),
    )
    .track_scroll(&self.explorer_scroll)
    .size_full();
    let rows = div()
      .id("explorer-viewport")
      .relative()
      .flex_1()
      .min_h_0()
      .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
        if !*hovered && this.hovered_folder.take().is_some() {
          cx.notify();
        }
      }))
      .child(list)
      .child(
        div()
          .absolute()
          .inset_0()
          .child(Scrollbar::new(&self.explorer_scroll).id("explorer-scrollbar").axis(ScrollbarAxis::Vertical).viewport_from_layout()),
      );
    let name = self
      .projects
      .projects
      .iter()
      .find(|project| self.projects.active.as_ref() == Some(&project.path))
      .map_or_else(|| SharedString::from("Vekro"), |project| SharedString::from(project.name()));
    let action =
      |id: &'static str, icon: &'static str, tooltip: &'static str| Button::new(id).ghost().xsmall().icon(Icon::empty().path(icon)).tooltip(tooltip);
    let header = h_flex()
      .gap_1()
      .items_center()
      .pl_5()
      .pr_3()
      .id("explorer-header")
      .context_menu(self.explorer_menu(None, cx))
      .child(Label::new(name).flex_1().min_w_0().font_weight(FontWeight::SEMIBOLD).whitespace_nowrap().overflow_hidden().text_ellipsis())
      .child(
        action("explorer-new-file", "icons/file-plus.svg", "New file").on_click(cx.listener(|this, _, window, cx| this.new_entry(false, window, cx))),
      )
      .child(
        action("explorer-new-folder", "icons/folder-plus.svg", "New folder")
          .on_click(cx.listener(|this, _, window, cx| this.new_entry(true, window, cx))),
      )
      .child(action("explorer-refresh", "icons/refresh-cw.svg", "Refresh").on_click(cx.listener(|this, _, _, cx| {
        this.refresh_project();
        cx.notify();
      })))
      .child(
        action("explorer-collapse", "icons/list-chevrons-down-up.svg", "Collapse folders")
          .on_click(cx.listener(|this, _, _, cx| this.collapse_folders(cx))),
      );
    v_flex().flex_1().min_h_0().gap_2().child(header).child(rows).into_any_element()
  }

  /// The repository summary and the folder tree.
  pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let content = match self.sidebar_tab {
      SidebarTab::Explorer => self.render_explorer(cx).into_any_element(),
      SidebarTab::Search => self.render_project_search(cx),
      SidebarTab::Git => match self.render_git(cx) {
        Some(git) => git.into_any_element(),
        None => Label::new("Not a Git repository").text_color(cx.theme().muted_foreground).p_3().into_any_element(),
      },
    };
    v_flex().size_full().bg(cx.theme().sidebar).child(
      v_flex()
        .flex_1()
        .min_h_0()
        .when(self.sidebar_tab == SidebarTab::Explorer, |sidebar| sidebar.bg(colors(cx).explorer_background).pt_3())
        .when(self.sidebar_tab == SidebarTab::Search, |sidebar| sidebar.px_3().pt_3())
        .child(content),
    )
  }

  /// The narrow bar at the far left: one button for each view of the sidebar (Explorer, Git), the open one marked; the terminal
  /// button under them; and project settings, with a small warning at its corner while an account needs attention.
  pub(super) fn render_activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let workspace = cx.entity();
    let changed = self.git.as_ref().map_or(0, Summary::changed);
    let mut buttons: HashMap<&'static str, AnyElement> = SidebarTab::ALL
      .into_iter()
      .map(|tab| {
        let workspace = workspace.clone();
        let focus = self.focus_handle.clone();
        let button = rail_button(
          tab.label(),
          tab.icon(),
          tab.label(),
          tab == self.sidebar_tab && !self.full_area_open(),
          true,
          &palette,
          move |_, window, cx| {
            if tab == SidebarTab::Search {
              focus.dispatch_action(&SearchProject, window, cx);
              return;
            }
            workspace.update(cx, |this, cx| {
              let entering_explorer = tab == SidebarTab::Explorer && this.sidebar_tab != tab;
              this.sidebar_tab = tab;
              this.hovered_folder = None;
              if entering_explorer {
                let active_file = this.editor_panel.read(cx).active_file().map(Path::to_path_buf);
                if let (Some(root), Some(path)) = (this.projects.active.clone(), active_file) {
                  this.show_explorer_entry(&root, path, ScrollStrategy::Nearest, cx);
                }
              }
              // The view of the sidebar and the editor come back when the terminals or the AI view were taking the main area.
              if this.full_area_open() {
                this.hide_terminal(window, cx);
              }
              cx.notify();
            })
          },
        );
        // The Git button shows how many files have changes, in a small blue circle at its top right.
        let badge = (tab == SidebarTab::Git && changed > 0).then(|| count_badge(changed, cx).absolute().top_0().right(-BADGE_OVERHANG));
        (tab.id(), div().relative().child(button).children(badge).into_any_element())
      })
      .collect();
    let attention = self.settings.read(cx).accounts.items.iter().any(|account| account.issue.is_some());
    let terminal_attention = self.terminal_tabs.values().any(|tabs| tabs.panes().iter().any(|terminal| terminal.read(cx).needs_attention()));
    let terminal_active = self.projects.active.as_ref().is_some_and(|root| self.project_has_terminal_activity(root, cx));
    let terminal = rail_button(
      "terminal-toggle",
      Icon::empty().path("icons/terminal.svg"),
      if terminal_active { "Terminal — command running" } else { "Terminal" },
      self.terminal_open,
      true,
      &palette,
      |_, window, cx| window.dispatch_action(Box::new(ToggleTerminal), cx),
    );
    let agents = rail_button(
      "agents-toggle",
      Icon::empty().path("icons/bot-message-square.svg"),
      "AI agents",
      self.agent_open,
      true,
      &palette,
      |_, window, cx| window.dispatch_action(Box::new(ToggleAgents), cx),
    );
    let settings = div()
      .relative()
      .child(rail_button("project-settings-open", Icon::empty().path("icons/folder-cog.svg"), "Project settings", false, true, &palette, {
        let workspace = workspace.clone();
        move |_, window, cx| {
          workspace.update(cx, |this, cx| {
            if let Some(path) = this.projects.active.clone() {
              this.open_project_settings(&path, window, cx);
            }
          })
        }
      }))
      // While an account has a permission problem, the warning badge opens Accounts.
      .when(attention, |gear| {
        gear.child(
          div()
            .id("account-attention")
            .absolute()
            .bottom_0()
            .right_0()
            .size_4()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .bg(theme.warning)
            .tooltip(|window, cx| Tooltip::new("An account needs attention").build(window, cx))
            .child(Icon::empty().path("icons/triangle-alert.svg").size_3().text_color(palette.on_accent))
            .on_click(cx.listener(|this, _, window, cx| this.open_settings_page("Accounts", window, cx))),
        )
      });
    buttons.insert(
      "terminal",
      div()
        .relative()
        .child(terminal)
        .when(terminal_active, |button| {
          button.child(status_badge(cx).absolute().top_0().right(-BADGE_OVERHANG).child(terminal_activity_spinner(cx).color(palette.on_accent)))
        })
        .when(terminal_attention, |button| {
          button.child(
            div()
              .absolute()
              .bottom_0()
              .right_0()
              .size_4()
              .flex()
              .items_center()
              .justify_center()
              .rounded_full()
              .bg(theme.warning)
              .child(Icon::empty().path("icons/triangle-alert.svg").size_3().text_color(palette.on_accent)),
          )
        })
        .into_any_element(),
    );
    buttons.insert("agents", agents.into_any_element());
    let layout = self.settings.read(cx).preferences.activity_bar.clone();
    v_flex()
      .w(ACTIVITY_BAR_WIDTH)
      .h_full()
      .flex_none()
      .items_center()
      .py_2()
      .gap_1()
      .bg(palette.rail)
      .border_r_1()
      .border_color(palette.border)
      .children(layout.visible(&activity_ids()).into_iter().filter_map(|id| buttons.remove(id)))
      .child(settings)
      .child(div().flex_1())
      .child(self.render_profile_picker())
  }

  /// The terminals, filling the main area like a terminal window: a tab strip when there are several tabs, and the panes of the
  /// shown tab, split side by side or one above the other, each with a small header when there is more than one.
  pub(super) fn render_terminal_area(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let (Some(root), Some(arrangement)) = (self.projects.active.clone(), self.arrangement()) else {
      return Label::new("No project is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let Some(node) = arrangement.current().cloned() else {
      return Label::new("No terminal is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let strip = (arrangement.tabs.len() > 1).then(|| {
      let tabs = arrangement.tabs.iter().enumerate().map(|(index, tab)| {
        let first = tab.panes().first().cloned();
        let label = first.as_ref().map(|terminal| terminal.read(cx).title()).unwrap_or_default();
        let key = first.map_or_else(|| index.to_string(), |terminal| terminal.entity_id().to_string());
        let close = Button::new(SharedString::from(format!("close-terminal-tab-{key}")))
          .ghost()
          .xsmall()
          .icon(Icon::empty().path("icons/x.svg").size_4())
          .tooltip("Close")
          .on_click(cx.listener(move |this, _, window, cx| {
            // The click must not also reach the tab and select it.
            cx.stop_propagation();
            this.close_terminal_tab(index, window, cx);
          }));
        strip_tab(SharedString::from(format!("terminal-tab-{key}")), label, index == arrangement.active, close, cx)
          .on_click(cx.listener(move |this, _, window, cx| this.select_terminal_tab(index, window, cx)))
      });
      strip(cx).child(h_flex().flex_1().min_w_0().h_full().children(tabs)).child(
        Button::new("terminal-new-tab")
          .ghost()
          .xsmall()
          .icon(Icon::empty().path("icons/plus.svg").size_4())
          .tooltip("New terminal tab")
          .mx_2()
          .on_click(cx.listener(|this, _, window, cx| this.new_terminal(&NewTerminal, window, cx))),
      )
    });
    let headers = node.panes().len() > 1;
    v_flex()
      .size_full()
      .children(strip)
      .child(div().flex_1().min_h_0().child(self.render_pane_node(&node, headers, &root.to_string_lossy(), cx)))
      .child(shortcut_bar(
        vec![
          shortcut_hint("New tab", &NewTerminal, window),
          shortcut_hint("Split right", &SplitRight, window),
          shortcut_hint("Split down", &SplitDown, window),
          shortcut_hint("Clear", &ClearBuffer, window),
          shortcut_hint("Mention projects", &MentionProjects, window),
          shortcut_hint("Close", &CloseTab, window),
          shortcut_hint("Hide", &ToggleTerminal, window),
        ],
        cx,
      ))
      .into_any_element()
  }

  /// The AI view, filling the main area. It is not a terminal window: a header names the project and holds one tab per
  /// session (agent icon and name) and a menu to start another agent, and with no session it shows the choice of agents.
  pub(super) fn render_agent_area(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let Some(root) = self.projects.active.clone() else {
      return Label::new("No project is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let project = SharedString::from(root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default());
    let workspace = cx.entity();
    let picker =
      Button::new("agent-new").ghost().xsmall().icon(Icon::empty().path("icons/plus.svg").size_4()).tooltip("New agent session").dropdown_menu(
        move |menu, _, _| {
          AgentKind::ALL.into_iter().fold(menu, |menu, kind| {
            let workspace = workspace.clone();
            menu.item(
              PopupMenuItem::new(kind.label())
                .icon(Icon::empty().path(kind.icon()))
                .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.new_agent(kind, window, cx))),
            )
          })
        },
      );
    let Some(tabs) = self.agents() else {
      return self.render_agent_empty(&project, cx);
    };
    let strip_tabs = tabs.tabs.iter().enumerate().map(|(index, tab)| {
      let key = tab.session.entity_id().to_string();
      let close = Button::new(SharedString::from(format!("close-agent-tab-{key}")))
        .ghost()
        .xsmall()
        .icon(Icon::empty().path("icons/x.svg").size_4())
        .tooltip("Close")
        .on_click(cx.listener(move |this, _, window, cx| {
          // The click must not also reach the tab and select it.
          cx.stop_propagation();
          this.close_agent_tab(index, window, cx);
        }));
      let label = h_flex().gap_1p5().items_center().child(Icon::empty().path(tab.kind.icon()).size_4()).child(tab.kind.label());
      strip_tab(SharedString::from(format!("agent-tab-{key}")), label, index == tabs.active, close, cx)
        .on_click(cx.listener(move |this, _, window, cx| this.select_agent_tab(index, window, cx)))
    });
    let header = strip(cx)
      .child(h_flex().flex_1().min_w_0().h_full().children(strip_tabs))
      .child(div().px_1().child(picker))
      .child(Label::new(project).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).pr_3());
    let shown = tabs.current().map(|tab| tab.session.clone());
    v_flex()
      .size_full()
      .child(header)
      .child(div().flex_1().min_h_0().bg(palette.background).children(shown))
      .child(shortcut_bar(
        vec![
          shortcut_hint("Mention projects", &MentionProjects, window),
          shortcut_hint("Close session", &CloseTab, window),
          shortcut_hint("Hide", &ToggleAgents, window),
        ],
        cx,
      ))
      .into_any_element()
  }

  /// The AI view with no session: the agents to start.
  pub(super) fn render_agent_empty(&self, project: &SharedString, cx: &mut Context<Self>) -> AnyElement {
    let buttons = AgentKind::ALL.map(|kind| {
      let button = Button::new(SharedString::from(format!("agent-start-{}", kind.command())))
        .outline()
        .icon(Icon::empty().path(kind.icon()).size_4())
        .label(SharedString::from(format!("Start {}", kind.label())))
        .tooltip(kind.description());
      match kind {
        AgentKind::Codex => button.on_click(cx.listener(|this, _, window, cx| this.new_codex(&NewCodex, window, cx))),
        AgentKind::Claude => button.on_click(cx.listener(|this, _, window, cx| this.new_claude(&NewClaude, window, cx))),
      }
    });
    Empty::new()
      .size_full()
      .child(
        EmptyHeader::new()
          .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::empty().path("icons/bot-message-square.svg")))
          .title(EmptyTitle::new().child("AI agents"))
          .description(EmptyDescription::new().child(SharedString::from(format!("Start Codex or Claude in {project}.")))),
      )
      .child(h_flex().gap_2().children(buttons))
      .into_any_element()
  }

  /// A part of a tab: a pane, or two parts split by a divider that can be dragged.
  pub(super) fn render_pane_node(&self, node: &Node<Entity<Terminal>>, headers: bool, key: &str, cx: &mut Context<Self>) -> AnyElement {
    match node {
      Node::Pane(terminal) => self.render_pane(terminal, headers, cx),
      Node::Split { id, direction, first, second } => {
        let id = SharedString::from(format!("terminal-split-{key}-{id}"));
        let group = match direction {
          Direction::Columns => h_resizable(id),
          Direction::Rows => v_resizable(id),
        };
        group
          .child(resizable_panel().child(self.render_pane_node(first, headers, key, cx)))
          .child(resizable_panel().child(self.render_pane_node(second, headers, key, cx)))
          .into_any_element()
      }
    }
  }

  /// One shell, with a header (close, title, menu) when the tab has several panes.
  pub(super) fn render_pane(&self, terminal: &Entity<Terminal>, headers: bool, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let workspace = cx.entity();
    let header = headers.then(|| {
      let (close, menu_terminal) = (terminal.clone(), terminal.clone());
      let (close_workspace, menu_workspace) = (workspace.clone(), workspace.clone());
      h_flex()
        .id(SharedString::from(format!("terminal-header-{}", terminal.entity_id())))
        .flex_none()
        .gap_1()
        .px_1()
        .py_0p5()
        .items_center()
        .bg(palette.surface_alt)
        .border_b_1()
        .border_color(palette.border)
        .child(
          Button::new(SharedString::from(format!("terminal-close-{}", terminal.entity_id())))
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/x.svg").size_4())
            .tooltip("Close")
            .on_click(move |_, window, cx| close_workspace.update(cx, |this, cx| this.close_pane(close.clone(), window, cx))),
        )
        .child(
          Label::new(terminal.read(cx).title())
            .flex_1()
            .min_w_0()
            .text_size(FONT_SIZE_SM)
            .text_color(theme.muted_foreground)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis(),
        )
        .child(
          Button::new(SharedString::from(format!("terminal-menu-{}", terminal.entity_id())))
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/ellipsis.svg").size_4())
            .tooltip("More")
            .dropdown_menu(move |menu, _, _| {
              let (tab, vertical, horizontal, close) =
                (menu_workspace.clone(), menu_workspace.clone(), menu_workspace.clone(), menu_workspace.clone());
              let (vertical_terminal, horizontal_terminal, close_terminal) = (menu_terminal.clone(), menu_terminal.clone(), menu_terminal.clone());
              menu
                .item(
                  PopupMenuItem::new("New Tab").on_click(move |_, window, cx| tab.update(cx, |this, cx| this.new_terminal(&NewTerminal, window, cx))),
                )
                .separator()
                .item(PopupMenuItem::new("Split Pane Vertically").on_click(move |_, window, cx| {
                  vertical.update(cx, |this, cx| this.split_pane(vertical_terminal.clone(), Direction::Columns, window, cx))
                }))
                .item(PopupMenuItem::new("Split Pane Horizontally").on_click(move |_, window, cx| {
                  horizontal.update(cx, |this, cx| this.split_pane(horizontal_terminal.clone(), Direction::Rows, window, cx))
                }))
                .separator()
                .item(
                  PopupMenuItem::new("Close")
                    .on_click(move |_, window, cx| close.update(cx, |this, cx| this.close_pane(close_terminal.clone(), window, cx))),
                )
            }),
        )
    });
    v_flex().size_full().children(header).child(div().flex_1().min_h_0().child(terminal.clone())).into_any_element()
  }
}
