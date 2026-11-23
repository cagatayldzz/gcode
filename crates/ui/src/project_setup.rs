//! The "Add project" dialog: create a new project, clone a repository or import an existing folder.

use crate::{
  connections::credentials_for_url,
  repository_picker::{RepositoryChosen, RepositoryPicker},
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    spinner::Spinner,
    v_flex, ActiveTheme, Disableable as _, Icon, Sizable as _, WindowExt as _,
  },
  *,
};
use std::path::PathBuf;
use ws_hosting::Accounts;

/// Sent when a project is ready to be added to the list.
pub(crate) struct ProjectReady(pub PathBuf);

impl EventEmitter<ProjectReady> for ProjectSetup {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
  New,
  Clone,
  Import,
}

const MODES: [Mode; 3] = [Mode::New, Mode::Clone, Mode::Import];

impl Mode {
  fn label(self) -> &'static str {
    match self {
      Mode::New => "New",
      Mode::Clone => "Clone",
      Mode::Import => "Import",
    }
  }

  fn icon(self) -> Icon {
    Icon::empty().path(match self {
      Mode::New => "icons/folder-plus.svg",
      Mode::Clone => "icons/git-fork.svg",
      Mode::Import => "icons/folder-input.svg",
    })
  }

  /// A few words under the label on the mode's card.
  fn subtitle(self) -> &'static str {
    match self {
      Mode::New => "Empty Git repository",
      Mode::Clone => "From a Git address",
      Mode::Import => "Existing folder",
    }
  }
}

pub(crate) struct ProjectSetup {
  mode: Mode,
  /// The project name (New) or repository URL (Clone).
  input: Entity<InputState>,
  /// The folder a new or cloned project goes into.
  location: PathBuf,
  busy: bool,
  error: Option<SharedString>,
  /// The connected accounts: their repositories can be browsed, and their tokens are used to clone.
  accounts: Accounts,
  /// The list of repositories of the connected accounts, while it is open.
  picker: Option<(Entity<RepositoryPicker>, Subscription)>,
  _subscription: Subscription,
}

impl ProjectSetup {
  /// `location` is where new and cloned projects go unless the user picks another folder.
  pub fn new(location: PathBuf, accounts: Accounts, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
    let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| match event {
      InputEvent::PressEnter { .. } => this.submit(cx),
      InputEvent::Change => cx.notify(),
      _ => {}
    });
    Self { mode: Mode::New, input, location, busy: false, error: None, accounts, picker: None, _subscription: subscription }
  }

  fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
    if self.busy {
      return;
    }
    self.mode = mode;
    self.error = None;
    self.picker = None;
    if mode == Mode::Clone && !self.accounts.items.is_empty() {
      self.open_picker(window, cx);
    }
    let placeholder = if mode == Mode::Clone { "https://github.com/user/repository.git" } else { "Project name" };
    self.input.update(cx, |input, cx| {
      input.set_value("", window, cx);
      input.set_placeholder(placeholder, window, cx);
    });
    cx.notify();
  }

  /// Asks for a folder: the location of a new or cloned project, or the project to import.
  fn choose_folder(&mut self, cx: &mut Context<Self>) {
    if self.busy {
      return;
    }
    let importing = self.mode == Mode::Import;
    let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
    cx.spawn(async move |this, cx| {
      let Ok(Ok(Some(paths))) = picked.await else { return };
      let Some(path) = paths.into_iter().next() else { return };
      let _ = this.update(cx, |this, cx| {
        if importing {
          cx.emit(ProjectReady(path));
        } else {
          this.location = path;
          cx.notify();
        }
      });
    })
    .detach();
  }

  /// Shows the list of the connected accounts' repositories.
  fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let picker = cx.new(|cx| RepositoryPicker::new(self.accounts.items.clone(), self.accounts.keychain(), window, cx));
    let subscription = cx.subscribe_in(&picker, window, |this, _, chosen: &RepositoryChosen, window, cx| {
      this.input.update(cx, |input, cx| input.set_value(chosen.0.clone(), window, cx));
      cx.notify();
    });
    self.picker = Some((picker, subscription));
  }

  fn submit(&mut self, cx: &mut Context<Self>) {
    if self.busy || self.mode == Mode::Import {
      return;
    }
    let text = self.input.read(cx).value().to_string();
    let (mode, location, accounts) = (self.mode, self.location.clone(), self.accounts.clone());
    self.busy = true;
    self.error = None;
    cx.notify();
    let work = cx.background_executor().spawn(async move {
      match mode {
        Mode::Clone => ws_git::clone(&text, &location, credentials_for_url(&accounts, &text).as_ref()),
        _ => ws_git::create_project(&location, &text),
      }
    });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.busy = false;
        match result {
          Ok(path) => cx.emit(ProjectReady(path)),
          Err(message) => this.error = Some(message.into()),
        }
        cx.notify();
      });
    })
    .detach();
  }
}

/// A field with a small caption above it.
pub(crate) fn field(caption: &'static str, muted: Hsla, content: impl IntoElement) -> impl IntoElement {
  v_flex().gap_1p5().child(Label::new(caption).text_color(muted).text_size(FONT_SIZE_SM).font_weight(FontWeight::MEDIUM)).child(content)
}

impl ProjectSetup {
  /// Where the project will end up, written out as the user types.
  fn destination(&self, cx: &App) -> Option<PathBuf> {
    let text = self.input.read(cx).value();
    let name = match self.mode {
      Mode::New => Some(text.trim().to_string()).filter(|name| !name.is_empty()),
      Mode::Clone => ws_git::repository_name(&text),
      Mode::Import => None,
    }?;
    Some(self.location.join(name))
  }

  fn mode_card(&self, mode: Mode, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let chosen = mode == self.mode;
    h_flex()
      .id(mode.label())
      .flex_1()
      .gap_2p5()
      .p_2p5()
      .items_center()
      .rounded_lg()
      .border_1()
      .cursor_pointer()
      .when(chosen, |card| card.border_color(palette.accent).bg(palette.selected))
      .when(!chosen, |card| card.border_color(palette.border).hover(|card| card.bg(palette.hover).border_color(palette.control_border)))
      .child(
        div()
          .flex_none()
          .size_8()
          .rounded_md()
          .flex()
          .items_center()
          .justify_center()
          .bg(if chosen { palette.accent } else { palette.surface_alt })
          .child(mode.icon().size_4().text_color(if chosen { palette.on_accent } else { theme.muted_foreground })),
      )
      .child(
        v_flex()
          .min_w_0()
          .child(Label::new(mode.label()).font_weight(FontWeight::SEMIBOLD))
          .child(Label::new(mode.subtitle()).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).whitespace_nowrap()),
      )
      .on_click(cx.listener(move |this, _, window, cx| this.set_mode(mode, window, cx)))
  }
}

impl Render for ProjectSetup {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let importing = self.mode == Mode::Import;
    let (caption, action, action_icon) = match self.mode {
      Mode::Clone => ("Repository URL", "Clone", "icons/download.svg"),
      _ => ("Project name", "Create project", "icons/folder-plus.svg"),
    };
    let cards = h_flex().gap_2().children(MODES.map(|mode| self.mode_card(mode, cx)));
    let location = h_flex()
      .gap_2()
      .items_center()
      .pl_3()
      .pr_1()
      .py_1()
      .rounded_md()
      .border_1()
      .border_color(palette.border)
      .child(Icon::empty().path("icons/folder.svg").size_4().flex_none().text_color(theme.muted_foreground))
      .child(Label::new(crate::components::display_path(&self.location)).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
      .child(Button::new("choose-location").label("Change…").small().ghost().on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx))));
    let destination = self.destination(cx).map(|path| {
      let (icon, text) = if self.mode == Mode::Clone {
        ("icons/download.svg", format!("Clones into {}", crate::components::display_path(&path)))
      } else {
        ("icons/git-branch.svg", format!("Creates {} with an empty Git repository on main", crate::components::display_path(&path)))
      };
      h_flex()
        .gap_2()
        .items_center()
        .px_3()
        .py_2()
        .rounded_md()
        .bg(palette.surface_alt)
        .child(Icon::empty().path(icon).size_4().flex_none().text_color(theme.muted_foreground))
        .child(
          Label::new(text)
            .flex_1()
            .min_w_0()
            .text_size(FONT_SIZE_SM)
            .text_color(theme.muted_foreground)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis(),
        )
    });
    let choose = v_flex()
      .id("import-card")
      .items_center()
      .gap_2()
      .py_8()
      .rounded_lg()
      .border_1()
      .border_dashed()
      .border_color(palette.control_border)
      .cursor_pointer()
      .hover(|card| card.bg(palette.hover).border_color(palette.accent))
      .child(
        div()
          .size_12()
          .rounded_full()
          .flex()
          .items_center()
          .justify_center()
          .bg(palette.selected)
          .child(Icon::empty().path("icons/folder-open.svg").size_6().text_color(palette.accent)),
      )
      .child(Label::new("Choose a folder…").font_weight(FontWeight::SEMIBOLD))
      .child(Label::new("It opens as a project at once and stays where it is.").text_color(theme.muted_foreground).text_size(FONT_SIZE_SM))
      .on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx)));
    let footer = h_flex()
      .gap_2()
      .justify_end()
      .px_5()
      .py_3()
      .border_t_1()
      .border_color(palette.border)
      .child(Button::new("cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
      .when(!importing, |footer| {
        footer.child(
          Button::new("submit").primary().disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.submit(cx))).child(
            h_flex()
              .gap_2()
              .items_center()
              .child(if self.busy { Spinner::new().small().into_any_element() } else { Icon::empty().path(action_icon).size_4().into_any_element() })
              .child(if self.busy { "Working…" } else { action }),
          ),
        )
      });
    // The title is drawn here (the dialog has no padding) so that it has the design's size and spacing.
    v_flex()
      .child(Label::new("Add project").text_base().font_weight(FontWeight::SEMIBOLD).px_5().pt_4().pb_2())
      .child(
        v_flex()
          .text_size(crate::theme::FONT_SIZE)
          .gap_4()
          .px_5()
          .pb_5()
          .pt_2()
          .child(cards)
          .when(!importing, |form| {
            form.child(field(caption, theme.muted_foreground, Input::new(&self.input).border_color(palette.border))).child(field(
              "Location",
              theme.muted_foreground,
              location,
            ))
          })
          .when_some(destination.filter(|_| !importing), |form, summary| form.child(summary))
          .when(self.mode == Mode::Clone, |form| match &self.picker {
            Some((picker, _)) => form.child(field("Your repositories", theme.muted_foreground, picker.clone())),
            None => form.child(
              h_flex()
                .gap_2()
                .items_start()
                .child(Icon::empty().path("icons/info.svg").size_4().flex_none().text_color(theme.muted_foreground))
                .child(
                  Label::new("Connect GitHub, GitLab or Azure DevOps in Settings → Accounts to browse your repositories and clone private ones.")
                    .flex_1()
                    .min_w_0()
                    .text_size(FONT_SIZE_SM)
                    .text_color(theme.muted_foreground),
                ),
            ),
          })
          .when(importing, |form| form.child(choose))
          .when_some(self.error.clone(), |form, error| {
            form.child(
              h_flex()
                .gap_2()
                .items_start()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(theme.danger.opacity(0.4))
                .bg(theme.danger.opacity(0.1))
                .child(Icon::empty().path("icons/triangle-alert.svg").size_4().flex_none().text_color(theme.danger))
                .child(Label::new(error).flex_1().min_w_0().text_color(theme.danger)),
            )
          }),
      )
      .child(footer)
  }
}
