//! The "Add project" dialog: create a new project, clone a repository or import an existing folder.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    tab::{Tab, TabBar},
    v_flex, ActiveTheme, Disableable as _, Icon, Sizable as _, WindowExt as _,
  },
  *,
};
use std::path::PathBuf;

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

  /// What the mode does, shown under the tabs.
  fn hint(self) -> &'static str {
    match self {
      Mode::New => "Creates a new folder with an empty Git repository.",
      Mode::Clone => "Downloads a repository with your Git login (SSH key or credential helper).",
      Mode::Import => "Adds a folder that already exists on this Mac. The folder stays where it is.",
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
  _subscription: Subscription,
}

impl ProjectSetup {
  /// `location` is where new and cloned projects go unless the user picks another folder.
  pub fn new(location: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
    let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
      if let InputEvent::PressEnter { .. } = event {
        this.submit(cx);
      }
    });
    Self { mode: Mode::New, input, location, busy: false, error: None, _subscription: subscription }
  }

  fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
    if self.busy {
      return;
    }
    self.mode = mode;
    self.error = None;
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

  fn submit(&mut self, cx: &mut Context<Self>) {
    if self.busy || self.mode == Mode::Import {
      return;
    }
    let text = self.input.read(cx).value().to_string();
    let (mode, location) = (self.mode, self.location.clone());
    self.busy = true;
    self.error = None;
    cx.notify();
    let work = cx.background_executor().spawn(async move {
      match mode {
        Mode::Clone => vekro_git::clone(&text, &location),
        _ => vekro_git::create_project(&location, &text),
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
fn field(caption: &'static str, muted: Hsla, content: impl IntoElement) -> impl IntoElement {
  v_flex().gap_1p5().child(Label::new(caption).text_color(muted).text_size(crate::theme::FONT_SIZE_SM)).child(content)
}

impl Render for ProjectSetup {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let importing = self.mode == Mode::Import;
    let (caption, action) = match self.mode {
      Mode::Clone => ("Repository URL", "Clone"),
      _ => ("Name", "Create project"),
    };
    let selected = MODES.iter().position(|mode| *mode == self.mode).unwrap_or(0);
    let tabs = TabBar::new("add-project-mode")
      .segmented()
      .selected_index(selected)
      .on_click(cx.listener(|this, index: &usize, window, cx| {
        if let Some(mode) = MODES.get(*index) {
          this.set_mode(*mode, window, cx);
        }
      }))
      .children(MODES.map(|mode| Tab::new().prefix(mode.icon().size_4()).label(mode.label())));
    let location = h_flex()
      .gap_2()
      .items_center()
      .pl_3()
      .pr_1()
      .py_1()
      .rounded_md()
      .border_1()
      .border_color(theme.input)
      .child(Icon::empty().path("icons/folder.svg").size_4().text_color(theme.muted_foreground))
      .child(Label::new(crate::components::display_path(&self.location)).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
      .child(Button::new("choose-location").label("Change…").small().ghost().on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx))));
    let choose = v_flex()
      .id("import-card")
      .items_center()
      .gap_2()
      .py_6()
      .rounded_lg()
      .border_1()
      .border_color(theme.input)
      .cursor_pointer()
      .hover(|card| card.bg(theme.muted))
      .child(Icon::empty().path("icons/folder-open.svg").size_8().text_color(theme.muted_foreground))
      .child(Label::new("Choose a folder…").font_weight(FontWeight::SEMIBOLD))
      .child(Label::new("It opens as a project at once.").text_color(theme.muted_foreground).text_size(crate::theme::FONT_SIZE_SM))
      .on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx)));
    v_flex()
      .text_size(crate::theme::FONT_SIZE)
      .gap_4()
      .child(tabs)
      .child(Label::new(self.mode.hint()).text_color(theme.muted_foreground))
      .when(!importing, |form| {
        form.child(field(caption, theme.muted_foreground, Input::new(&self.input))).child(field("Location", theme.muted_foreground, location))
      })
      .when(importing, |form| form.child(choose))
      .when_some(self.error.clone(), |form, error| form.child(Label::new(error).text_color(theme.danger)))
      .child(h_flex().gap_2().justify_end().child(Button::new("cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx))).when(
        !importing,
        |footer| {
          footer.child(
            Button::new("submit")
              .label(if self.busy { "Working…" } else { action })
              .primary()
              .disabled(self.busy)
              .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
          )
        },
      ))
  }
}
