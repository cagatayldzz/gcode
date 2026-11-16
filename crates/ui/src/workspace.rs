use crate::{explorer, AddProject};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    v_flex, ActiveTheme, Icon, IconName, Sizable as _, TitleBar,
  },
  *,
};
use std::{collections::HashSet, path::PathBuf};
use vekro_git::Summary;
use vekro_storage::ProjectList;

/// Height of the top band: the traffic lights and the title.
pub const TOOLBAR_HEIGHT: Pixels = px(44.);

/// Width of the icon rail on the far left.
const RAIL_WIDTH: Pixels = px(56.);

/// Width of the panel next to the rail.
const PANEL_WIDTH: Pixels = px(264.);

/// Indentation of each tree level.
const INDENT: Pixels = px(16.);

/// One window: the rail and panel on the left and the open project on the right.
pub struct Workspace {
  projects: ProjectList,
  git: Option<Summary>,
  expanded: HashSet<PathBuf>,
  rows: Vec<explorer::Row>,
  selected_file: Option<PathBuf>,
  focus_handle: FocusHandle,
}

impl Workspace {
  pub fn new(cx: &mut Context<Self>) -> Self {
    let mut this = Self {
      projects: ProjectList::load(&ProjectList::default_path()),
      git: None,
      expanded: HashSet::new(),
      rows: Vec::new(),
      selected_file: None,
      focus_handle: cx.focus_handle(),
    };
    this.refresh_project();
    this
  }

  /// Asks for a folder and adds it as a project.
  pub fn add_project(&mut self, _: &AddProject, _: &mut Window, cx: &mut Context<Self>) {
    let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
    cx.spawn(async move |this, cx| {
      let Ok(Ok(Some(paths))) = picked.await else { return };
      let Some(path) = paths.into_iter().next() else { return };
      let _ = this.update(cx, |this, cx| {
        this.projects.add(path);
        this.project_changed(cx);
      });
    })
    .detach();
  }

  fn select_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    self.projects.active = Some(path);
    self.project_changed(cx);
  }

  fn project_changed(&mut self, cx: &mut Context<Self>) {
    if let Err(error) = self.projects.save(&ProjectList::default_path()) {
      eprintln!("Could not save the project list: {error}");
    }
    self.expanded.clear();
    self.selected_file = None;
    self.refresh_project();
    cx.notify();
  }

  /// Reads the repository summary and the folder tree of the active project.
  fn refresh_project(&mut self) {
    match &self.projects.active {
      Some(root) => {
        self.git = vekro_git::summary(root);
        self.rows = explorer::rows(root, &self.expanded);
      }
      None => {
        self.git = None;
        self.rows.clear();
      }
    }
  }

  fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    if !self.expanded.remove(&path) {
      self.expanded.insert(path);
    }
    self.refresh_project();
    cx.notify();
  }

  fn select_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    self.selected_file = Some(path);
    cx.notify();
  }

  fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let item = |id: &'static str, icon: Icon, selected: bool| {
      div()
        .id(id)
        .size_10()
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .when(selected, |item| item.bg(theme.sidebar_accent))
        .text_color(if selected { theme.foreground } else { theme.muted_foreground })
        .child(icon.size_5())
    };
    v_flex()
      .w(RAIL_WIDTH)
      .h_full()
      .flex_none()
      .items_center()
      .py_3()
      .gap_2()
      .bg(theme.sidebar)
      .border_r_1()
      .border_color(theme.sidebar_border)
      .child(item("rail-files", IconName::FolderClosed.into(), true))
      .child(item("rail-git", Icon::empty().path("icons/git-branch.svg"), false))
      .child(item("rail-terminal", IconName::SquareTerminal.into(), false))
      .child(item("rail-settings", IconName::Settings.into(), false))
  }

  fn render_projects(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let rows = self.projects.projects.iter().map(|project| {
      let path = project.path.clone();
      let selected = self.projects.active.as_ref() == Some(&project.path);
      h_flex()
        .id(SharedString::from(format!("project-{}", project.path.display())))
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded_md()
        .items_center()
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.sidebar_accent))
        .hover(|row| row.bg(theme.sidebar_accent))
        .child(Icon::new(IconName::FolderClosed).size_4().text_color(theme.muted_foreground))
        .child(Label::new(project.name()).text_sm())
        .on_click(cx.listener(move |this, _, _, cx| this.select_project(path.clone(), cx)))
    });
    v_flex()
      .gap_1()
      .child(h_flex().justify_between().items_center().px_2().child(Label::new("Projects").text_sm().font_weight(FontWeight::MEDIUM)).child(
        Button::new("add-project").icon(IconName::Plus).ghost().small().on_click(|_, window, cx| window.dispatch_action(Box::new(AddProject), cx)),
      ))
      .children(rows)
  }

  fn render_git(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let theme = cx.theme().clone();
    let git = self.git.as_ref()?;
    let changed = match git.changed {
      0 => "No changes".to_string(),
      1 => "1 changed file".to_string(),
      count => format!("{count} changed files"),
    };
    Some(
      v_flex()
        .gap_1()
        .child(Label::new("Git").text_sm().font_weight(FontWeight::MEDIUM).px_2())
        .child(
          h_flex()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .child(Icon::empty().path("icons/git-branch.svg").size_4().text_color(theme.muted_foreground))
            .child(Label::new(git.branch.clone()).text_sm().flex_1())
            .child(Icon::new(IconName::ChevronDown).size_4().text_color(theme.muted_foreground)),
        )
        .child(
          h_flex()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .child(div().size_2().rounded_full().bg(if git.changed > 0 { theme.primary } else { theme.muted_foreground }).mx_1())
            .child(Label::new(changed).text_sm().text_color(theme.muted_foreground)),
        ),
    )
  }

  fn render_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let rows = self.rows.iter().map(|row| {
      let path = row.path.clone();
      let selected = self.selected_file.as_ref() == Some(&row.path);
      let open = self.expanded.contains(&row.path);
      let icon = if row.is_dir { IconName::FolderClosed } else { IconName::File };
      h_flex()
        .id(SharedString::from(format!("entry-{}", row.path.display())))
        .gap_1p5()
        .pr_2()
        .py_1()
        .pl(INDENT * row.depth as f32 + px(8.))
        .rounded_md()
        .items_center()
        .cursor_pointer()
        .when(selected, |entry| entry.bg(theme.sidebar_accent))
        .hover(|entry| entry.bg(theme.sidebar_accent))
        .child(div().size_4().flex_none().when(row.is_dir, |chevron| {
          chevron.child(Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight }).size_4().text_color(theme.muted_foreground))
        }))
        .child(Icon::new(icon).size_4().text_color(theme.muted_foreground))
        .child(Label::new(row.name.clone()).text_sm().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
        .on_click(cx.listener(move |this, _, _, cx| {
          if this.rows.iter().any(|row| row.path == path && row.is_dir) {
            this.toggle_folder(path.clone(), cx);
          } else {
            this.select_file(path.clone(), cx);
          }
        }))
    });
    let name = self.projects.active.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned());
    v_flex()
      .flex_1()
      .min_h_0()
      .gap_1()
      .child(Label::new("Explorer").text_sm().font_weight(FontWeight::MEDIUM).px_2())
      .when_some(name, |section, name| {
        section.child(
          h_flex()
            .px_2()
            .py_1()
            .gap_1p5()
            .items_center()
            .child(div().size_4().flex_none())
            .child(Icon::new(IconName::FolderOpen).size_4().text_color(theme.muted_foreground))
            .child(Label::new(name).text_sm()),
        )
      })
      .child(v_flex().id("explorer-rows").flex_1().min_h_0().overflow_y_scroll().children(rows))
  }

  fn render_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let divider = || div().h_px().w_full().bg(theme.sidebar_border);
    v_flex()
      .w(PANEL_WIDTH)
      .h_full()
      .flex_none()
      .p_3()
      .gap_3()
      .bg(theme.sidebar)
      .border_r_1()
      .border_color(theme.sidebar_border)
      .child(self.render_projects(cx))
      .child(divider())
      .when_some(self.render_git(cx), |panel, git| panel.child(git).child(divider()))
      .child(self.render_explorer(cx))
  }
}

impl Render for Workspace {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let title = self
      .projects
      .active
      .as_ref()
      .and_then(|path| path.file_name())
      .map(|name| name.to_string_lossy().into_owned())
      .unwrap_or_else(|| "Vekro".into());
    v_flex()
      .id("workspace")
      .key_context("Workspace")
      .track_focus(&self.focus_handle)
      .on_action(cx.listener(Self::add_project))
      .size_full()
      .bg(theme.background)
      .text_color(theme.foreground)
      .child(TitleBar::new().h(TOOLBAR_HEIGHT).pl(px(80.)).child(Label::new(title).text_sm().font_weight(FontWeight::SEMIBOLD)))
      .child(h_flex().flex_1().min_h_0().child(self.render_rail(cx)).child(self.render_panel(cx)).child(div().flex_1()))
  }
}
