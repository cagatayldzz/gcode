//! Profiles: the list shared by the whole application, opening a window for a profile, and the Profiles page of Settings.

use crate::{
  theme::{colors, FONT_SIZE_SM},
  workspace::{Workspace, TOOLBAR_HEIGHT},
  NewWindow, SwitchProfile,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    v_flex, ActiveTheme, Icon, Sizable as _, TitleBar,
  },
  *,
};
use vekro_storage::{Profile, ProfileList, WindowMode, WindowPlacement};

/// The profiles, loaded when first needed. Every change is written to `profiles.json` at once.
struct ProfilesGlobal(ProfileList);

impl Global for ProfilesGlobal {}

fn ensure_loaded(cx: &mut App) {
  if !cx.has_global::<ProfilesGlobal>() {
    cx.set_global(ProfilesGlobal(ProfileList::load(&ProfileList::default_path())));
  }
}

/// The profiles as they are now.
pub(crate) fn profile_list(cx: &mut App) -> ProfileList {
  ensure_loaded(cx);
  cx.global::<ProfilesGlobal>().0.clone()
}

/// Changes the list and saves it; a failed save is logged and the change stays for this run.
pub(crate) fn update_profiles<T>(cx: &mut App, change: impl FnOnce(&mut ProfileList) -> T) -> T {
  ensure_loaded(cx);
  let list = &mut cx.global_mut::<ProfilesGlobal>().0;
  let result = change(list);
  if let Err(error) = list.save(&ProfileList::default_path()) {
    eprintln!("Could not save the profiles: {error}");
  }
  result
}

/// The profile of a window, if it is a Vekro window that can be read right now (not the one being updated).
fn window_profile(handle: AnyWindowHandle, cx: &App) -> Option<Profile> {
  let workspace = handle.downcast::<Workspace>()?;
  workspace.read(cx).ok().map(|workspace| workspace.profile().clone())
}

/// The window as it was when it was last closed, if it still fits on a screen that is connected; else a centered default.
fn initial_bounds(cx: &App) -> WindowBounds {
  let saved = WindowPlacement::load(&WindowPlacement::default_path()).and_then(|placement| {
    let bounds = Bounds::new(point(px(placement.x), px(placement.y)), size(px(placement.width), px(placement.height)));
    // Its center must be on a screen, so that a window saved on a display that is gone does not open out of sight.
    let visible = cx.displays().iter().any(|display| display.bounds().contains(&bounds.center()));
    visible.then_some(match placement.mode {
      WindowMode::Windowed => WindowBounds::Windowed(bounds),
      WindowMode::Maximized => WindowBounds::Maximized(bounds),
      WindowMode::Fullscreen => WindowBounds::Fullscreen(bounds),
    })
  });
  saved.unwrap_or_else(|| WindowBounds::Windowed(Bounds::centered(None, size(px(1280.), px(800.)), cx)))
}

/// Opens a window that shows `profile`.
pub fn open_window(profile: Profile, cx: &mut App) {
  let options = WindowOptions {
    window_bounds: Some(initial_bounds(cx)),
    window_min_size: Some(size(px(800.), px(500.))),
    titlebar: Some(TitlebarOptions { traffic_light_position: Some(point(px(18.), TOOLBAR_HEIGHT / 2. - px(6.))), ..TitleBar::title_bar_options() }),
    ..TitleBar::window_options()
  };
  update_profiles(cx, |list| list.set_last(&profile.id));
  gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| Workspace::new(profile.clone(), window, cx)))
    .expect("Failed to open the Vekro window");
}

/// Opens a window with the profile that was used last (at startup, and when the Dock icon is clicked with no window open).
pub fn open_last_window(cx: &mut App) {
  let profile = profile_list(cx).last_used();
  open_window(profile, cx);
}

/// File → New Window: a window with the profile of the window in front, or else the one used last.
fn new_window(cx: &mut App) {
  let current = cx.active_window().and_then(|handle| window_profile(handle, cx));
  match current {
    Some(profile) => open_window(profile, cx),
    None => open_last_window(cx),
  }
}

/// Brings forward a window that shows the profile, or opens one.
fn switch_profile(id: &str, cx: &mut App) {
  let Some(profile) = profile_list(cx).get(id).cloned() else { return };
  let existing = cx.windows().into_iter().find(|handle| window_profile(*handle, cx).is_some_and(|shown| shown.id == profile.id));
  match existing {
    Some(handle) => {
      let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
    None => open_window(profile, cx),
  }
}

/// Opens the window of a profile that was just created or chosen. Windows read each other, so call it outside the update of
/// the window that asks (`cx.defer`).
pub(crate) fn show_profile(id: String, cx: &mut App) {
  switch_profile(&id, cx);
}

/// The ids of the profiles that have a window open, other than the window being updated.
pub(crate) fn other_open_profiles(cx: &App) -> Vec<String> {
  cx.windows().into_iter().filter_map(|handle| window_profile(handle, cx)).map(|profile| profile.id).collect()
}

/// Registers the application-wide actions of profiles and windows.
pub(crate) fn init(cx: &mut App) {
  ensure_loaded(cx);
  // The menu bar until the first window sets its own.
  let list = profile_list(cx);
  cx.set_menus(crate::menus(None, &list.profiles, &list.last));
  cx.on_action(|_: &NewWindow, cx| new_window(cx));
  cx.on_action(|action: &SwitchProfile, cx| switch_profile(&action.id, cx));
}

/// The Profiles page of Settings: every profile with its actions.
pub(crate) struct ProfileManager {
  workspace: Entity<Workspace>,
  _observer: Subscription,
}

impl ProfileManager {
  pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
    let observer = cx.observe(&workspace, |_, _, cx| cx.notify());
    Self { workspace, _observer: observer }
  }
}

impl Render for ProfileManager {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let list = profile_list(cx);
    let current = self.workspace.read(cx).profile().id.clone();
    let rows = list.profiles.iter().map(|profile| {
      let id = profile.id.clone();
      let is_current = id == current;
      let (open, rename, delete) = (id.clone(), id.clone(), id.clone());
      h_flex()
        .id(SharedString::from(format!("profile-{id}")))
        .gap_3()
        .px_3()
        .py_2()
        .items_center()
        .rounded_md()
        .bg(palette.surface_alt)
        .child(Icon::empty().path("icons/user.svg").size_5().text_color(theme.muted_foreground))
        .child(
          v_flex().flex_1().min_w_0().child(Label::new(profile.name.clone()).font_weight(FontWeight::SEMIBOLD)).child(
            Label::new(if is_current {
              "Used by this window"
            } else if profile.is_default() {
              "Default"
            } else {
              ""
            })
            .text_size(FONT_SIZE_SM)
            .text_color(theme.muted_foreground),
          ),
        )
        .when(!is_current, |row| {
          row.child(Button::new(SharedString::from(format!("profile-open-{id}"))).label("Open").small().outline().on_click(cx.listener(
            move |_, _, _, cx| {
              let id = open.clone();
              cx.defer(move |cx| show_profile(id, cx));
            },
          )))
        })
        .child(
          Button::new(SharedString::from(format!("profile-rename-{id}")))
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/pencil.svg"))
            .tooltip("Rename")
            .on_click(cx.listener(move |this, _, window, cx| {
              this.workspace.update(cx, |workspace, cx| workspace.rename_profile(rename.clone(), window, cx));
            })),
        )
        .when(!profile.is_default(), |row| {
          row.child(
            Button::new(SharedString::from(format!("profile-delete-{id}")))
              .ghost()
              .xsmall()
              .icon(Icon::empty().path("icons/trash.svg"))
              .tooltip("Delete this profile and its data")
              .on_click(cx.listener(move |this, _, window, cx| {
                this.workspace.update(cx, |workspace, cx| workspace.confirm_delete_profile(delete.clone(), window, cx));
              })),
          )
        })
    });
    v_flex().gap_2().children(rows).child(
      h_flex().child(
        Button::new("profile-add")
          .label("Add profile…")
          .icon(Icon::empty().path("icons/plus.svg"))
          .small()
          .outline()
          .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::AddProfile), cx)),
      ),
    )
  }
}
