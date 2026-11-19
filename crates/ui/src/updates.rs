//! Vekro's own updates: the state every window shares, and the panel in Settings that shows it.

use crate::{
  settings::{SettingsGlobal, SettingsModel},
  theme::{colors, FONT_SIZE_SM},
};
use futures::{channel::mpsc, StreamExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    scroll::ScrollableElement as _,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Sizable as _,
  },
  *,
};
use std::time::SystemTime;
use vekro_updater::{Release, Stage, Staged};

/// The version of the running application.
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Development builds never check for or install published releases.
pub(crate) const UPDATES_ENABLED: bool = !cfg!(debug_assertions) && cfg!(target_os = "macos");

/// How tall the release notes get before they scroll.
const NOTES_MAX_HEIGHT: Pixels = px(160.);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppUpdate {
  Idle,
  Checking,
  UpToDate,
  Available(Release),
  /// Preparing the update; the release is kept so a failure can offer a retry.
  Working(Release, Stage),
  /// Downloaded and verified; restarting installs it.
  Ready(Release),
  Failed(String),
}

impl AppUpdate {
  fn is_working(&self) -> bool {
    matches!(self, Self::Checking | Self::Working(..))
  }
}

/// The update state of the shared settings model.
pub(crate) struct UpdateState {
  pub update: AppUpdate,
  pub checked: Option<SystemTime>,
  /// The release a window already announced, so each is announced once.
  pub announced: Option<String>,
  /// Set by the title bar's Update button: restart as soon as the download is ready.
  pub restart_when_ready: bool,
  last_release: Option<Release>,
  staged: Option<Staged>,
}

impl UpdateState {
  pub fn new() -> Self {
    Self { update: AppUpdate::Idle, checked: None, announced: None, restart_when_ready: false, last_release: None, staged: None }
  }
}

enum Message<T> {
  Stage(Stage),
  Done(T),
}

impl SettingsModel {
  /// Looks for a newer Vekro release.
  pub(crate) fn check_update(&mut self, cx: &mut Context<Self>) {
    if !UPDATES_ENABLED || self.updates.update.is_working() || matches!(self.updates.update, AppUpdate::Ready(_)) {
      return;
    }
    self.updates.update = AppUpdate::Checking;
    cx.notify();
    Self::run_update_task(
      cx,
      |_| vekro_updater::check(APP_VERSION),
      |_, _, _| {},
      |model, result, _| {
        model.updates.checked = Some(SystemTime::now());
        model.updates.update = match result {
          Ok(Some(release)) => AppUpdate::Available(release),
          Ok(None) => AppUpdate::UpToDate,
          Err(error) => AppUpdate::Failed(error.to_string()),
        };
      },
    );
  }

  /// Downloads and verifies the available release.
  pub(crate) fn download_update(&mut self, cx: &mut Context<Self>) {
    if !UPDATES_ENABLED {
      return;
    }
    let release = match &self.updates.update {
      AppUpdate::Available(release) => release.clone(),
      AppUpdate::Failed(_) => match &self.updates.last_release {
        Some(release) => release.clone(),
        None => return,
      },
      _ => return,
    };
    let current = match vekro_updater::current_app_bundle() {
      Ok(path) => path,
      Err(error) => {
        self.updates.update = AppUpdate::Failed(error.to_string());
        cx.notify();
        return;
      }
    };
    self.updates.last_release = Some(release.clone());
    self.updates.update = AppUpdate::Working(release.clone(), Stage::Downloading);
    cx.notify();
    let staged_release = release.clone();
    Self::run_update_task(
      cx,
      move |stage| vekro_updater::prepare(&release, &current, stage),
      move |model, stage, _| model.updates.update = AppUpdate::Working(staged_release.clone(), stage),
      |model, result, _| match result {
        Ok(staged) => {
          model.updates.staged = Some(staged);
          model.updates.update = model.updates.last_release.clone().map(AppUpdate::Ready).unwrap_or(AppUpdate::Idle);
        }
        Err(error) => model.updates.update = AppUpdate::Failed(error.to_string()),
      },
    );
  }

  /// Quits and lets the installer swap in the verified update.
  pub(crate) fn restart_to_update(&mut self, cx: &mut Context<Self>) {
    if !UPDATES_ENABLED {
      return;
    }
    let Some(staged) = self.updates.staged.take() else { return };
    match vekro_updater::current_app_bundle().and_then(|current| vekro_updater::install(&staged, &current)) {
      Ok(()) => cx.quit(),
      Err(error) => {
        vekro_updater::discard(&staged);
        self.updates.update = AppUpdate::Failed(error.to_string());
        cx.notify();
      }
    }
  }

  /// Runs `work` on a background thread. `on_stage` receives progress reports on the foreground, then `done` the result.
  fn run_update_task<T: Send + 'static>(
    cx: &mut Context<Self>,
    work: impl FnOnce(&dyn Fn(Stage)) -> T + Send + 'static,
    on_stage: impl Fn(&mut Self, Stage, &mut Context<Self>) + 'static,
    done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
  ) {
    let (sender, mut receiver) = mpsc::unbounded();
    std::thread::spawn(move || {
      let stages = sender.clone();
      let result = work(&|stage| {
        let _ = stages.unbounded_send(Message::Stage(stage));
      });
      let _ = sender.unbounded_send(Message::Done(result));
    });
    cx.spawn(async move |this, cx| {
      let mut done = Some(done);
      while let Some(message) = receiver.next().await {
        let alive = this.update(cx, |model, cx| {
          match message {
            Message::Stage(stage) => on_stage(model, stage, cx),
            Message::Done(result) => {
              if let Some(done) = done.take() {
                done(model, result, cx);
              }
            }
          }
          cx.notify();
        });
        if alive.is_err() {
          break;
        }
      }
    })
    .detach();
  }
}

fn model(cx: &App) -> Entity<SettingsModel> {
  cx.global::<SettingsGlobal>().0.clone()
}

/// The update status in Settings, with its actions.
pub(crate) struct UpdatePanel {
  _observe: Subscription,
}

impl UpdatePanel {
  pub fn new(cx: &mut Context<Self>) -> Self {
    Self { _observe: cx.observe(&model(cx), |_, _, cx| cx.notify()) }
  }
}

/// `2026-11-17 10:53`, for the time of the last check.
fn local_time(time: SystemTime) -> String {
  let seconds = time.elapsed().map_or(0, |elapsed| elapsed.as_secs());
  match seconds {
    0..=59 => "just now".to_string(),
    60..=3599 => format!("{} minutes ago", seconds / 60),
    _ => format!("{} hours ago", seconds / 3600),
  }
}

impl Render for UpdatePanel {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let shared = model(cx);
    let (update, checked) = {
      let state = &shared.read(cx).updates;
      (state.update.clone(), state.checked)
    };
    let caption = |text: String| Label::new(text).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground);
    let action = |id: &'static str, label: &'static str, primary: bool, run: fn(&mut SettingsModel, &mut Context<SettingsModel>)| {
      let shared = shared.clone();
      let button = Button::new(id).label(label).small().on_click(move |_, _, cx| shared.update(cx, run));
      if primary {
        button.primary()
      } else {
        button.outline()
      }
    };
    let row = || h_flex().gap_3().items_center().flex_wrap();
    if !UPDATES_ENABLED {
      return caption("Updates are available in release builds, not in development builds.".to_string()).into_any_element();
    }
    match update {
      AppUpdate::Idle => row()
        .child(caption(format!("Version {APP_VERSION}. Not checked yet.")))
        .child(action("check-update", "Check for updates", false, SettingsModel::check_update))
        .into_any_element(),
      AppUpdate::Checking => row().child(Spinner::new().small()).child(Label::new("Checking for updates…")).into_any_element(),
      AppUpdate::UpToDate => row()
        .child(Label::new("Vekro is up to date").font_weight(FontWeight::SEMIBOLD))
        .child(caption(match checked {
          Some(time) => format!("Version {APP_VERSION}, checked {}.", local_time(time)),
          None => format!("Version {APP_VERSION}."),
        }))
        .child(action("check-update", "Check again", false, SettingsModel::check_update))
        .into_any_element(),
      AppUpdate::Available(release) => {
        let notes = release.plain_notes();
        let url = release.page_url.clone();
        v_flex()
          .gap_2()
          .child(
            row()
              .child(Label::new(format!("Vekro {} is available", release.version)).font_weight(FontWeight::SEMIBOLD))
              .child(caption(format!("You have {APP_VERSION}.")))
              .when(!url.is_empty(), |line| {
                line.child(Button::new("release-page").ghost().small().label("View on GitHub").on_click(move |_, _, cx| cx.open_url(&url)))
              })
              .child(action("download-update", "Download & Install", true, SettingsModel::download_update)),
          )
          .when(!notes.is_empty(), |column| {
            column.child(
              v_flex()
                .id("release-notes")
                .max_h(NOTES_MAX_HEIGHT)
                .overflow_y_scrollbar()
                .p_2()
                .rounded_md()
                .bg(palette.surface_alt)
                .child(Label::new(notes).text_size(FONT_SIZE_SM)),
            )
          })
          .into_any_element()
      }
      AppUpdate::Working(release, stage) => row()
        .child(Spinner::new().small())
        .child(Label::new(match stage {
          Stage::Downloading => format!("Downloading Vekro {}…", release.version),
          Stage::Verifying => "Verifying the download…".to_string(),
        }))
        .into_any_element(),
      AppUpdate::Ready(release) => row()
        .child(Label::new(format!("Vekro {} is ready", release.version)).font_weight(FontWeight::SEMIBOLD))
        .child(caption("Verified: checksum, code signature and developer. Restarting installs it.".to_string()))
        .child(action("restart-update", "Restart to Update", true, SettingsModel::restart_to_update))
        .into_any_element(),
      AppUpdate::Failed(message) => v_flex()
        .gap_2()
        .child(Label::new(message).text_color(theme.danger))
        .child(row().child(action("check-update", "Try again", false, SettingsModel::check_update)))
        .into_any_element(),
    }
  }
}
