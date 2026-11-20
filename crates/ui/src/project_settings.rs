//! The project settings view: how a project is named and looks in the project menu (name, icon, color), where its Git remote
//! points, and removing it. It takes the main area like the terminals do, as one page with a bar of actions at the bottom.

use crate::{
  components::{avatar_look, display_path, size_label, time_ago},
  project_setup::field,
  theme::{colors, project_color, FONT_SIZE, FONT_SIZE_SM, PROJECT_COLORS},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{InputEvent, InputState},
    label::Label,
    scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode},
    v_flex, ActiveTheme, Disableable as _, Icon, StyledExt as _,
  },
  *,
};
use std::path::{Path, PathBuf};
use vekro_index::{IndexInfo, ProjectIndex};
use vekro_storage::Project;

/// The icons a project can take instead of its first letter.
pub(crate) const PROJECT_ICONS: [&str; 20] = [
  "rocket",
  "code",
  "terminal",
  "globe",
  "database",
  "server",
  "package",
  "zap",
  "flame",
  "leaf",
  "bug",
  "cpu",
  "gamepad-2",
  "music",
  "camera",
  "book-open",
  "heart",
  "star",
  "cloud",
  "shield",
];

/// The asset path of a project icon, if `name` is one of [`PROJECT_ICONS`].
pub(crate) fn icon_path(name: &str) -> Option<SharedString> {
  PROJECT_ICONS.contains(&name).then(|| SharedString::from(format!("icons/{name}.svg")))
}

/// The look the user chose, sent when Save is pressed.
pub(crate) struct AppearanceChosen {
  pub path: PathBuf,
  pub name: String,
  pub icon: Option<String>,
  pub color: Option<String>,
  /// The new remote address, only when the user changed it.
  pub remote: Option<String>,
  /// Also rename the project's folder on disk to the display name.
  pub rename_folder: bool,
}

pub(crate) enum ProjectSettingsEvent {
  Save(AppearanceChosen),
  RemoveRequested(PathBuf),
  /// Leave the view without saving.
  Close,
}

/// A change to the saved index, with what to say while it runs: it gets the cache folder and the project folder.
type IndexWork = (&'static str, fn(&Path, &Path) -> std::io::Result<()>);

/// The pages listed on the left; the chosen one fills the right side.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
  General,
  Appearance,
  Git,
  Index,
  Remove,
}

impl Page {
  const ALL: [Self; 5] = [Self::General, Self::Appearance, Self::Git, Self::Index, Self::Remove];

  fn label(self) -> &'static str {
    match self {
      Self::General => "General",
      Self::Appearance => "Appearance",
      Self::Git => "Git",
      Self::Index => "Index",
      Self::Remove => "Remove project",
    }
  }

  fn icon(self) -> &'static str {
    match self {
      Self::General => "icons/sliders-horizontal.svg",
      Self::Appearance => "icons/palette.svg",
      Self::Git => "icons/git-branch.svg",
      Self::Index => "icons/database.svg",
      Self::Remove => "icons/trash.svg",
    }
  }

  fn description(self) -> &'static str {
    match self {
      Self::General => "The name shown in the project menu and on the project's tab.",
      Self::Appearance => "The avatar of the project: a color, and an icon instead of its first letter.",
      Self::Git => "Update the remote address when the repository moves.",
      Self::Index => "The list of the project's files, made once and kept, so that searching and opening the project start from a ready list.",
      Self::Remove => "The project leaves the list and its terminals stop. The folder and its files stay on disk.",
    }
  }
}

pub(crate) struct ProjectSettings {
  page: Page,
  project: Project,
  input: Entity<InputState>,
  remote_input: Entity<InputState>,
  remote: String,
  rename_folder: bool,
  icon: Option<String>,
  color: Option<String>,
  scroll: ScrollHandle,
  /// The saved index of the project; `None` while it is being read.
  index: Option<IndexInfo>,
  /// What the index is being made or deleted for, while that runs.
  index_work: Option<&'static str>,
  index_error: Option<SharedString>,
  index_task: Task<()>,
  _subscription: Subscription,
}

impl EventEmitter<ProjectSettingsEvent> for ProjectSettings {}

impl ProjectSettings {
  pub fn new(project: &Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let folder = project.folder_name();
    let current = project.name();
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(folder).default_value(current));
    let remote = vekro_git::remote_url(&project.path).unwrap_or_default();
    let remote_input = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repository.git").default_value(remote.clone()));
    let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| match event {
      InputEvent::PressEnter { .. } => this.save(cx),
      InputEvent::Change => cx.notify(),
      _ => {}
    });
    let mut this = Self {
      page: Page::General,
      project: project.clone(),
      input,
      remote_input,
      remote,
      rename_folder: false,
      icon: project.icon.clone(),
      color: project.color.clone(),
      scroll: ScrollHandle::new(),
      index: None,
      index_work: None,
      index_error: None,
      index_task: Task::ready(()),
      _subscription: subscription,
    };
    this.refresh_index(None, cx);
    this
  }

  /// Runs `work` on the index of the project in the background, then reads what the saved index looks like now. `label` says what is
  /// going on while it runs; `None` only reads.
  fn refresh_index(&mut self, work: Option<IndexWork>, cx: &mut Context<Self>) {
    let root = self.project.path.clone();
    self.index_work = work.map(|(label, _)| label);
    self.index_error = None;
    let job = cx.background_executor().spawn(async move {
      let cache = vekro_storage::index_dir();
      let failure = work.and_then(|(_, run)| run(&cache, &root).err());
      (ProjectIndex::info(&cache, &root), failure)
    });
    self.index_task = cx.spawn(async move |this, cx| {
      let (info, failure) = job.await;
      let _ = this.update(cx, |this, cx| {
        this.index = Some(info);
        this.index_work = None;
        this.index_error = failure.map(|error| SharedString::from(format!("Could not change the index: {error}")));
        cx.notify();
      });
    });
    cx.notify();
  }

  fn save(&self, cx: &mut Context<Self>) {
    let name = self.input.read(cx).value().to_string();
    let remote = self.remote_input.read(cx).value().trim().to_string();
    let remote = (remote != self.remote && !remote.is_empty()).then_some(remote);
    cx.emit(ProjectSettingsEvent::Save(AppearanceChosen {
      path: self.project.path.clone(),
      name,
      icon: self.icon.clone(),
      color: self.color.clone(),
      remote,
      rename_folder: self.rename_folder && self.folder_differs(cx),
    }));
  }

  /// Whether the display name is not the folder's name, so renaming the folder would change something.
  fn folder_differs(&self, cx: &Context<Self>) -> bool {
    let name = self.input.read(cx).value().trim().to_string();
    !name.is_empty() && name != self.project.folder_name()
  }

  /// Whether anything differs from what the project has now, which is what Save would change.
  fn is_dirty(&self, cx: &Context<Self>) -> bool {
    let name = self.input.read(cx).value().trim().to_string();
    let remote = self.remote_input.read(cx).value().trim().to_string();
    name != self.project.name()
      || self.icon != self.project.icon
      || self.color != self.project.color
      || (remote != self.remote && !remote.is_empty())
      || (self.rename_folder && self.folder_differs(cx))
  }

  fn swatch(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let (name, value) = PROJECT_COLORS[index];
    let chosen = self.color.as_deref() == Some(value);
    let palette = colors(cx);
    // A ring around the color marks the chosen one; the check keeps it readable on any color.
    div()
      .id(SharedString::from(format!("color-{name}")))
      .size_8()
      .flex()
      .items_center()
      .justify_center()
      .rounded_full()
      .border_2()
      .cursor_pointer()
      .border_color(if chosen { palette.text } else { gpui_kit::transparent_black() })
      .child(
        div()
          .size_5()
          .flex()
          .items_center()
          .justify_center()
          .rounded_full()
          .when_some(project_color(value), |swatch, color| swatch.bg(color))
          .when(chosen, |swatch| swatch.child(Icon::empty().path("icons/check.svg").size_3().text_color(white()))),
      )
      .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name).build(window, cx))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.color = Some(value.into());
        cx.notify();
      }))
  }

  /// One square tile of the icon grid: the project's initial when `name` is `None`.
  fn icon_button(&self, name: Option<&'static str>, cx: &mut Context<Self>) -> impl IntoElement {
    let chosen = self.icon.as_deref() == name;
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let tint = if chosen { palette.on_accent } else { theme.muted_foreground };
    div()
      .id(SharedString::from(format!("icon-{}", name.unwrap_or("letter"))))
      .size_9()
      .flex()
      .items_center()
      .justify_center()
      .rounded_md()
      .cursor_pointer()
      .when(chosen, |tile| tile.bg(palette.accent))
      .when(!chosen, |tile| tile.hover(|tile| tile.bg(palette.selected)))
      .text_color(tint)
      .map(|tile| match name {
        Some(name) => tile.child(Icon::empty().path(format!("icons/{name}.svg")).size_4()),
        None => tile.child(Label::new("Aa").text_size(FONT_SIZE_SM).font_semibold()),
      })
      .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.unwrap_or("Project initial")).build(window, cx))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.icon = name.map(str::to_owned);
        cx.notify();
      }))
  }

  /// The avatar, the name as typed and the folder: how the project looks in the project menu.
  fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let mut project = self.project.clone();
    let name = self.input.read(cx).value().trim().to_string();
    project.name = (!name.is_empty()).then_some(name);
    project.icon.clone_from(&self.icon);
    project.color.clone_from(&self.color);
    let avatar = avatar_look(&project, true, cx);
    h_flex()
      .gap_3()
      .items_center()
      .px_4()
      .py_4()
      .border_b_1()
      .border_color(palette.border)
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .size_10()
          .flex_none()
          .rounded_md()
          .font_semibold()
          .text_color(avatar.text)
          .when_some(avatar.fill, |badge, fill| badge.bg(fill))
          .map(|badge| match avatar.icon {
            Some(path) => badge.child(Icon::empty().path(path).size_5()),
            None => badge.child(avatar.letter),
          }),
      )
      .child(
        v_flex()
          .min_w_0()
          .child(div().truncate().font_semibold().child(project.name()))
          .child(div().truncate().text_size(FONT_SIZE_SM).text_color(palette.text_secondary).child(display_path(&self.project.path))),
      )
  }

  /// One entry of the list on the left, drawn like a row of the Explorer.
  fn nav_item(&self, page: Page, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let selected = self.page == page;
    let tint = if selected { palette.explorer_selected_text } else { cx.theme().muted_foreground };
    h_flex()
      .id(SharedString::from(format!("project-settings-page-{}", page.label())))
      .gap_2()
      .px_4()
      .py_2()
      .items_center()
      .cursor_pointer()
      .when(selected, |row| row.bg(palette.explorer_selected))
      .when(!selected, |row| row.hover(|row| row.bg(palette.hover)))
      .child(Icon::empty().path(page.icon()).size_4().text_color(tint))
      .child(Label::new(page.label()).when(selected, |label| label.text_color(palette.explorer_selected_text)))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.page = page;
        cx.notify();
      }))
  }

  /// The display name, and whether the folder on disk follows it.
  fn general(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let name_field = v_flex().gap_2().child(crate::components::soft_input(&self.input, cx)).when(self.folder_differs(cx), |column| {
      column
        .child(Checkbox::new("rename-folder").label("Also rename the folder on disk").checked(self.rename_folder).on_click(cx.listener(
          |this, checked: &bool, _, cx| {
            this.rename_folder = *checked;
            cx.notify();
          },
        )))
        .child(
          Label::new("Terminals of this project restart and its open files close. Unsaved files block the rename.")
            .text_size(FONT_SIZE_SM)
            .text_color(palette.text_secondary),
        )
    });
    section(Page::General, field("Display name", muted, name_field), cx)
  }

  /// The color and the icon of the project's avatar.
  fn appearance(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let default_color = self.color.is_none();
    let colors_row = h_flex()
      .gap_1()
      .items_center()
      .flex_wrap()
      .child(
        div()
          .id("color-none")
          .h_8()
          .px_3()
          .mr_1()
          .flex()
          .items_center()
          .rounded_full()
          .cursor_pointer()
          .text_size(FONT_SIZE_SM)
          .when(default_color, |pill| pill.bg(palette.accent).text_color(palette.on_accent))
          .when(!default_color, |pill| pill.bg(palette.surface_alt).text_color(palette.text_secondary).hover(|pill| pill.bg(palette.selected)))
          .child("Default")
          .on_click(cx.listener(|this, _, _, cx| {
            this.color = None;
            cx.notify();
          })),
      )
      .children((0..PROJECT_COLORS.len()).map(|index| self.swatch(index, cx)));
    let icons =
      h_flex().gap_0p5().flex_wrap().child(self.icon_button(None, cx)).children(PROJECT_ICONS.iter().map(|name| self.icon_button(Some(name), cx)));
    section(Page::Appearance, v_flex().gap_4().child(field("Color", muted, colors_row)).child(field("Icon", muted, icons)), cx)
  }

  fn git(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    section(Page::Git, field("Git remote", muted, crate::components::soft_input(&self.remote_input, cx)), cx)
  }

  /// The saved index: what it holds, how old it is, and the buttons to make it again or delete it.
  fn index_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let busy = self.index_work.is_some();
    let status = match (&self.index, self.index_work) {
      (_, Some(label)) => SharedString::from(label),
      (None, None) => "Reading…".into(),
      (Some(info), None) if !info.saved => "Not made yet".into(),
      (Some(info), None) if info.fresh => "Ready".into(),
      (Some(_), None) => "Out of date: the project changed since it was made".into(),
    };
    let row = |caption: &'static str, value: SharedString| {
      h_flex()
        .gap_4()
        .px_4()
        .py_2p5()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(palette.border)
        .child(Label::new(caption).text_color(muted))
        .child(div().min_w_0().truncate().child(value))
    };
    let info = self.index.as_ref();
    let number = |value: Option<String>| SharedString::from(value.unwrap_or_else(|| "–".into()));
    let saved = info.filter(|info| info.saved);
    let rows = v_flex()
      .w_full()
      .border_t_1()
      .border_color(palette.border)
      .child(row("Status", status))
      .child(row("Files", number(saved.map(|info| info.files.to_string()))))
      .child(row("Size of the files", number(saved.map(|info| size_label(info.total_bytes)))))
      .child(row("Size of the index", number(saved.map(|info| size_label(info.saved_bytes)))))
      .child(row("Made", number(saved.and_then(|info| info.saved_at).map(time_ago))))
      .child(row("Location", number(info.map(|info| display_path(&info.path)))));
    let buttons = h_flex()
      .gap_2()
      .child(
        Button::new("index-rebuild")
          .label(if saved.is_some() { "Rebuild index" } else { "Make index" })
          .icon(Icon::empty().path("icons/refresh-cw.svg"))
          .outline()
          .disabled(busy)
          .on_click(cx.listener(|this, _, _, cx| {
            this.refresh_index(Some(("Making the index…", |cache, root| ProjectIndex::rebuild(cache, root).map(drop))), cx);
          })),
      )
      .child(
        Button::new("index-delete")
          .label("Delete index")
          .icon(Icon::empty().path("icons/trash.svg"))
          .danger()
          .outline()
          .disabled(busy || saved.is_none())
          .on_click(cx.listener(|this, _, _, cx| {
            this.refresh_index(Some(("Deleting the index…", |cache, root| ProjectIndex::remove(cache, root).map(drop))), cx);
          })),
      );
    section(
      Page::Index,
      v_flex()
        .gap_4()
        .child(rows)
        .when_some(self.index_error.clone(), |column, error| column.child(Label::new(error).text_color(palette.deleted)))
        .child(buttons)
        .child(
          Label::new("Deleting it frees the space only; the project is indexed again the next time it is opened or searched.")
            .text_size(FONT_SIZE_SM)
            .text_color(palette.text_secondary),
        ),
      cx,
    )
  }

  /// Removing the project from Vekro; its folder stays on disk.
  fn danger(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let path = self.project.path.clone();
    section(
      Page::Remove,
      h_flex().child(
        Button::new("settings-remove")
          .label("Remove project")
          .icon(Icon::empty().path("icons/trash.svg"))
          .danger()
          .outline()
          .on_click(cx.listener(move |_, _, _, cx| cx.emit(ProjectSettingsEvent::RemoveRequested(path.clone())))),
      ),
      cx,
    )
  }
}

/// The right side of the settings: the page's title and what it is for, then its controls.
fn section(page: Page, content: impl IntoElement, cx: &App) -> Div {
  let palette = colors(cx);
  v_flex()
    .gap_5()
    .child(
      v_flex()
        .gap_1()
        .child(div().text_lg().font_semibold().child(page.label()))
        .child(Label::new(page.description()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary)),
    )
    .child(content)
}

impl Render for ProjectSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let dirty = self.is_dirty(cx);
    let content = match self.page {
      Page::General => self.general(cx).into_any_element(),
      Page::Appearance => self.appearance(cx).into_any_element(),
      Page::Git => self.git(cx).into_any_element(),
      Page::Index => self.index_page(cx).into_any_element(),
      Page::Remove => self.danger(cx).into_any_element(),
    };
    let sidebar = v_flex()
      .w_64()
      .flex_none()
      .h_full()
      .bg(cx.theme().sidebar)
      .border_r_1()
      .border_color(palette.border)
      .child(self.header(cx))
      .child(v_flex().pt_2().children(Page::ALL.map(|page| self.nav_item(page, cx))));
    let body = div()
      .relative()
      .flex_1()
      .min_w_0()
      .h_full()
      .child(
        div()
          .id("project-settings-body")
          .size_full()
          .overflow_y_scroll()
          .track_scroll(&self.scroll)
          .child(v_flex().w_full().max_w(rems(40.)).px_8().py_6().child(content)),
      )
      .child(
        div().absolute().inset_0().child(
          Scrollbar::new(&self.scroll)
            .id("project-settings-scrollbar")
            .axis(ScrollbarAxis::Vertical)
            .mode(ScrollbarMode::Always)
            .viewport_from_layout(),
        ),
      );
    v_flex().size_full().bg(palette.background).text_size(FONT_SIZE).child(h_flex().flex_1().min_h_0().child(sidebar).child(body)).child(
      h_flex()
        .justify_end()
        .gap_2()
        .px_6()
        .py_3()
        .border_t_1()
        .border_color(palette.border)
        .bg(palette.rail)
        .child(Button::new("settings-cancel").label(if dirty { "Discard" } else { "Close" }).on_click(cx.listener(|_, _, _, cx| {
          cx.emit(ProjectSettingsEvent::Close);
        })))
        .child(Button::new("settings-save").label("Save").primary().disabled(!dirty).on_click(cx.listener(|this, _, _, cx| this.save(cx)))),
    )
  }
}
