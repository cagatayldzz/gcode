//! Profiles: the list shared by the whole application, opening a window for a profile, and the Profiles page of Settings.

use crate::{
  theme::{colors, FONT_SIZE_SM},
  workspace::{Workspace, TOOLBAR_HEIGHT},
  SwitchProfile,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    v_flex, ActiveTheme, Icon, Sizable as _, TitleBar,
  },
  *,
};
use std::{collections::HashMap, path::PathBuf};
use ws_storage::{Profile, ProfileList, WindowMode, WindowPlacement};

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
  let previous_profiles = list.profiles.clone();
  let result = change(list);
  if let Err(error) = list.save(&ProfileList::default_path()) {
    eprintln!("Could not save the profiles: {error}");
  }
  if list.profiles != previous_profiles {
    crate::recent::refresh_dock_menu(cx);
  }
  result
}

/// The profile of a window, if it is an app window that can be read right now (not the one being updated).
fn window_profile(handle: AnyWindowHandle, cx: &App) -> Option<Profile> {
  // The window's root is the Kit's `Root`, which holds the workspace as its view.
  let root = handle.downcast::<gpui_kit::base::Root>()?;
  let root = root.read(cx).ok()?;
  root.view().clone().downcast::<Workspace>().ok().map(|workspace| workspace.read(cx).profile().clone())
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

/// The window that shows the profile, if one is open and can be read right now.
fn profile_window(id: &str, cx: &App) -> Option<AnyWindowHandle> {
  cx.windows().into_iter().find(|handle| window_profile(*handle, cx).is_some_and(|shown| shown.id == id))
}

/// Shows `profile`: brings its window forward if it has one, else opens one. A profile never has two windows.
pub fn open_window(profile: Profile, cx: &mut App) {
  if let Some(handle) = profile_window(&profile.id, cx) {
    let _ = handle.update(cx, |_, window, _| window.activate_window());
    return;
  }
  let options = WindowOptions {
    window_bounds: Some(initial_bounds(cx)),
    window_min_size: Some(size(px(800.), px(500.))),
    titlebar: Some(TitlebarOptions { traffic_light_position: Some(point(px(18.), TOOLBAR_HEIGHT / 2. - px(6.))), ..TitleBar::title_bar_options() }),
    ..TitleBar::window_options()
  };
  update_profiles(cx, |list| list.set_last(&profile.id));
  let (handle, workspace) = gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| Workspace::new(profile.clone(), window, cx)))
    .expect("Failed to open the window");
  // The keyboard starts in the window, so its shortcuts work before anything is clicked.
  let _ = handle.update(cx, |_, window, cx| {
    // The close button minimizes the window, so the terminals and agents keep running; ⌘Q quits the app.
    window.on_window_should_close(cx, |window, _| {
      window.minimize_window();
      false
    });
    workspace.update(cx, |workspace, cx| workspace.warm_index(cx));
    let (profile, active) = workspace.read_with(cx, |workspace, _| (workspace.profile().id.clone(), workspace.project_list().active.clone()));
    if let Some(active) = active {
      crate::recent::record_project(&profile, &active, cx);
    }
    let focus = workspace.read(cx).focus_handle().clone();
    focus.focus(window, cx);
  });
}

/// Opens a recent file or project from the Dock menu: the window of the profile comes forward (or opens), and shows the project.
pub(crate) fn open_recent(profile: &str, path: PathBuf, file: bool, cx: &mut App) {
  switch_profile(profile, cx);
  let Some(handle) = profile_window(profile, cx) else { return };
  let Some(root) = handle.downcast::<gpui_kit::base::Root>() else { return };
  let _ = root.update(cx, |root, window, cx| {
    if let Ok(workspace) = root.view().clone().downcast::<Workspace>() {
      workspace.update(cx, |workspace, cx| workspace.open_recent(path, file, window, cx));
    }
  });
}

/// Opens a window with the profile that was used last (at startup, and when the Dock icon is clicked with no window open).
pub fn open_last_window(cx: &mut App) {
  let id = profile_list(cx).last_used().id;
  switch_profile(&id, cx);
}

/// Brings forward a window that shows the profile, or opens one.
fn switch_profile(id: &str, cx: &mut App) {
  let Some(profile) = profile_list(cx).get(id).cloned() else { return };
  open_window(profile, cx);
  cx.activate(true);
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
  crate::recent::refresh_dock_menu(cx);
  cx.on_action(|action: &SwitchProfile, cx| {
    let id = action.id.clone();
    cx.defer(move |cx| switch_profile(&id, cx));
  });
}

/// How many projects and connected accounts a profile has.
#[derive(Clone, Copy, Default)]
struct ProfileStats {
  projects: usize,
  accounts: usize,
}

/// The Profiles page of Settings: every profile as a card with what it holds, whether it is open, and its actions.
pub(crate) struct ProfileManager {
  workspace: Entity<Workspace>,
  /// What each profile holds, read off the interface thread; shown as soon as it arrives.
  stats: HashMap<String, ProfileStats>,
  /// What the stats were read for: the profiles and this window's project count, so that a change reads them again.
  seen: (Vec<Profile>, usize),
  _stats_task: Task<()>,
  _observer: Subscription,
}

impl ProfileManager {
  pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
    let observer = cx.observe(&workspace, |this, workspace, cx| {
      let seen = (profile_list(cx).profiles, workspace.read(cx).project_list().projects.len());
      if seen != this.seen {
        this.seen = seen;
        this.reload_stats(cx);
      }
      cx.notify();
    });
    // The page is made while the workspace is being updated (Settings opens from it), so the workspace cannot be read here: the project count
    // is filled in by the first change the workspace reports.
    let seen = (profile_list(cx).profiles, 0);
    let mut manager = Self { workspace, stats: HashMap::new(), seen, _stats_task: Task::ready(()), _observer: observer };
    manager.reload_stats(cx);
    manager
  }

  /// Reads the projects and accounts of every profile in the background.
  fn reload_stats(&mut self, cx: &mut Context<Self>) {
    let profiles = profile_list(cx).profiles;
    let work = cx.background_executor().spawn(async move {
      profiles
        .iter()
        .map(|profile| {
          let projects = ws_storage::ProjectList::load(&profile.projects_path()).projects.len();
          let accounts = ws_hosting::Accounts::load_in(&profile.accounts_path(), profile.keychain_namespace()).items.len();
          (profile.id.clone(), ProfileStats { projects, accounts })
        })
        .collect::<HashMap<_, _>>()
    });
    self._stats_task = cx.spawn(async move |this, cx| {
      let stats = work.await;
      let _ = this.update(cx, |this, cx| {
        this.stats = stats;
        cx.notify();
      });
    });
  }
}

fn count(number: usize, noun: &str) -> String {
  format!("{number} {noun}{}", if number == 1 { "" } else { "s" })
}

/// A small label after a profile's name: `tint` is its color.
fn chip(text: &'static str, tint: Hsla) -> Div {
  h_flex().px_1p5().rounded_md().bg(tint.opacity(0.14)).text_size(FONT_SIZE_SM).font_weight(FontWeight::MEDIUM).text_color(tint).child(text)
}

impl Render for ProfileManager {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let list = profile_list(cx);
    let current = self.workspace.read(cx).profile().id.clone();
    let open_elsewhere = other_open_profiles(cx);
    let cards = list.profiles.iter().map(|profile| {
      let id = profile.id.clone();
      let is_current = id == current;
      let is_open = is_current || open_elsewhere.contains(&id);
      let (open, rename, delete) = (id.clone(), id.clone(), id.clone());
      let initial: SharedString = profile.name.trim().chars().take(1).flat_map(char::to_uppercase).collect::<String>().into();
      let summary = match self.stats.get(&id) {
        Some(stats) => format!("{} · {}", count(stats.projects, "project"), count(stats.accounts, "connected account")),
        None => String::new(),
      };
      h_flex()
        .id(SharedString::from(format!("profile-{id}")))
        .gap_3()
        .px_3()
        .py_2p5()
        .items_center()
        .rounded_lg()
        .border_1()
        .border_color(if is_current { theme.primary } else { palette.border })
        .bg(palette.surface_alt)
        .child(Avatar::new().name(initial).small().size(PROFILE_CARD_AVATAR))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(
              h_flex()
                .gap_2()
                .items_center()
                .child(Label::new(profile.name.clone()).font_weight(FontWeight::SEMIBOLD).overflow_hidden().whitespace_nowrap().text_ellipsis())
                .when(is_current, |row| row.child(chip("This window", theme.primary)))
                .when(is_open && !is_current, |row| row.child(chip("Open", palette.new)))
                .when(profile.is_default(), |row| row.child(chip("Default", theme.muted_foreground))),
            )
            .child(Label::new(summary).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground)),
        )
        .when(!is_current, |row| {
          row.child(
            Button::new(SharedString::from(format!("profile-open-{id}")))
              .label(if is_open { "Switch to" } else { "Open" })
              .small()
              .outline()
              .on_click(cx.listener(move |_, _, _, cx| {
                let id = open.clone();
                cx.defer(move |cx| show_profile(id, cx));
              })),
          )
        })
        .child(
          Button::new(SharedString::from(format!("profile-rename-{id}")))
            .ghost()
            .small()
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
              .small()
              .icon(Icon::empty().path("icons/trash.svg"))
              .tooltip("Delete this profile and its data")
              .on_click(cx.listener(move |this, _, window, cx| {
                this.workspace.update(cx, |workspace, cx| workspace.confirm_delete_profile(delete.clone(), window, cx));
              })),
          )
        })
    });
    v_flex().gap_2().children(cards).child(
      h_flex().pt_1().child(
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

/// The avatar of a profile card: the Kit's small avatar (24 px) stretched, as in the activity bar, to sit beside two lines of text.
const PROFILE_CARD_AVATAR: Pixels = px(32.);
