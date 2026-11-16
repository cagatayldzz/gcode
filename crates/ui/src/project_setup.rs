//! The "Add project" dialog: create a new project, clone a repository or import an existing folder.

use crate::components::selected_style;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    v_flex, ActiveTheme, Disableable as _, Sizable as _,
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

  fn tab(&self, id: &'static str, label: &'static str, mode: Mode, cx: &mut Context<Self>) -> impl IntoElement {
    Button::new(id)
      .label(label)
      .small()
      .when(self.mode == mode, |button| button.custom(selected_style(cx)))
      .when(self.mode != mode, |button| button.ghost())
      .on_click(cx.listener(move |this, _, window, cx| this.set_mode(mode, window, cx)))
  }
}

impl Render for ProjectSetup {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let (hint, action) = match self.mode {
      Mode::New => ("Creates a new folder with an empty Git repository.", "Create project"),
      Mode::Clone => ("Downloads a repository with your Git login (SSH key or credential helper).", "Clone"),
      Mode::Import => ("Adds a folder that already exists on this Mac.", "Choose folder…"),
    };
    let importing = self.mode == Mode::Import;
    v_flex()
      .gap_3()
      .child(
        h_flex().gap_1().child(self.tab("mode-new", "New", Mode::New, cx)).child(self.tab("mode-clone", "Clone", Mode::Clone, cx)).child(self.tab(
          "mode-import",
          "Import",
          Mode::Import,
          cx,
        )),
      )
      .child(Label::new(hint).text_sm().text_color(theme.muted_foreground))
      .when(!importing, |form| {
        form.child(Input::new(&self.input)).child(
          h_flex()
            .gap_2()
            .items_center()
            .child(Label::new("In").text_sm().text_color(theme.muted_foreground))
            .child(Label::new(self.location.display().to_string()).text_sm().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
            .child(Button::new("choose-location").label("Change…").small().on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx)))),
        )
      })
      .when_some(self.error.clone(), |form, error| form.child(Label::new(error).text_sm().text_color(theme.danger)))
      .child(h_flex().justify_end().child(
        Button::new("submit").label(if self.busy { "Working…" } else { action }).primary().disabled(self.busy).on_click(cx.listener(
          |this, _, _, cx| {
            if this.mode == Mode::Import {
              this.choose_folder(cx);
            } else {
              this.submit(cx);
            }
          },
        )),
      ))
  }
}
