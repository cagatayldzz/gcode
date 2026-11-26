//! The Settings view: pages grouped by what they are for in a list on the left (with a search that finds a page by the settings on
//! it), and on the right the chosen page as titled cards of rows. A row is a setting's name and description with its control at the
//! right. It looks like the project settings view and uses the app's own text size.

use crate::{
  components::{display_path, soft_input},
  connections::AccountsList,
  profiles::ProfileManager,
  project_manager::ProjectManager,
  settings::{activity_layout, activity_row, choose_projects_folder, SettingsModel, Store},
  theme::{colors, Colors, FONT_SIZE, FONT_SIZE_SM},
  updates::UpdatePanel,
  workspace::Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonCustomVariant, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    label::Label,
    scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode},
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _,
  },
  *,
};
use std::path::PathBuf;
use ws_storage::{FontChoice, Preferences, ThemeChoice, DEFAULT_CODE_FONT_SIZE, DEFAULT_EXPLORER_FONT_SIZE, FONT_SIZES};

/// The pages of Settings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
  General,
  Editor,
  ActivityBar,
  Projects,
  Profiles,
  Accounts,
  About,
}

/// The headings of the list on the left and the pages under each.
const SECTIONS: [(&str, &[Page]); 4] = [
  ("Application", &[Page::General, Page::Editor, Page::ActivityBar]),
  ("Workspace", &[Page::Projects, Page::Profiles]),
  ("Connections", &[Page::Accounts]),
  ("Product", &[Page::About]),
];

impl Page {
  /// The page a caller names ("Accounts", "Projects", …); General when the name is not known.
  pub(crate) fn named(name: &str) -> Self {
    SECTIONS.iter().flat_map(|(_, pages)| pages.iter()).copied().find(|page| page.label() == name).unwrap_or(Self::General)
  }

  fn label(self) -> &'static str {
    match self {
      Self::General => "General",
      Self::Editor => "Editor",
      Self::ActivityBar => "Activity bar",
      Self::Projects => "Projects",
      Self::Profiles => "Profiles",
      Self::Accounts => "Accounts",
      Self::About => "About",
    }
  }

  fn icon(self) -> &'static str {
    match self {
      Self::General => "icons/sliders-horizontal.svg",
      Self::Editor => "icons/code.svg",
      Self::ActivityBar => "icons/panel-left.svg",
      Self::Projects => "icons/folder-git-2.svg",
      Self::Profiles => "icons/users.svg",
      Self::Accounts => "icons/key-round.svg",
      Self::About => "icons/info.svg",
    }
  }

  fn description(self) -> String {
    match self {
      Self::General => "How the app looks.".into(),
      Self::Editor => "The size of text in the editor, in a file's changes and in Explorer.".into(),
      Self::ActivityBar => "Order the buttons of the bar at the left edge and choose which ones show. A hidden button's shortcut still works.".into(),
      Self::Projects => {
        "Where new and cloned projects are created, and the projects of the menu in the title bar: rename, recolor, reorder, group or remove them. Right-click a project in that menu to move it to a category."
          .into()
      }
      Self::Profiles => "Each profile keeps its own projects, settings and connected accounts, like a browser profile. Choose one from the Profiles menu to open its window.".into(),
      Self::Accounts => format!(
        "A connected account lets {} list your repositories to clone, list and open pull requests, and gives Git the token for that host over HTTPS. Tokens are kept in the macOS Keychain.",
        ws_product::NAME
      ),
      Self::About => format!("The version of {}, its updates, where it keeps its files and where to find the project.", ws_product::NAME),
    }
  }

  /// Words that find the page in the search besides its name.
  fn keywords(self) -> &'static str {
    match self {
      Self::General => "appearance theme light dark system color mode",
      Self::Editor => "font size text code explorer files preview",
      Self::ActivityBar => "buttons order show hide sidebar rail shortcut",
      Self::Projects => "folder directory clone new create add all repositories list rename color icon category group reorder remove menu title bar",
      Self::Profiles => "profile window accounts separate",
      Self::Accounts => "github gitlab azure devops token keychain connect pull request permission",
      Self::About => "version update release download restart launch check repository github link issues source data folder location",
    }
  }

  fn matches(self, query: &str) -> bool {
    query.is_empty() || format!("{} {}", self.label(), self.keywords()).to_lowercase().contains(query)
  }
}

/// The Settings view's content.
pub(crate) struct SettingsWindow {
  page: Page,
  store: Store,
  /// The window of the application that does what some settings ask for (adding projects).
  workspace: Entity<Workspace>,
  fallback_dir: PathBuf,
  projects: Entity<ProjectManager>,
  profiles: Entity<ProfileManager>,
  accounts: Entity<AccountsList>,
  updates: Entity<UpdatePanel>,
  search: Entity<InputState>,
  scroll: ScrollHandle,
  _subscriptions: Vec<Subscription>,
}

impl SettingsWindow {
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    model: Entity<SettingsModel>,
    workspace: Entity<Workspace>,
    fallback_dir: PathBuf,
    projects: Entity<ProjectManager>,
    profiles: Entity<ProfileManager>,
    accounts: Entity<AccountsList>,
    updates: Entity<UpdatePanel>,
    page: &str,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search settings"));
    let subscriptions = vec![
      // A change made here or in another window of the profile redraws the controls.
      cx.observe(&model, |_, _, cx| cx.notify()),
      cx.subscribe_in(&search, window, |this, _, event: &InputEvent, _, cx| match event {
        InputEvent::Change => cx.notify(),
        // Enter opens the first page that matches.
        InputEvent::PressEnter { .. } => {
          let query = this.query(cx);
          if let Some(page) = SECTIONS.iter().flat_map(|(_, pages)| pages.iter()).copied().find(|page| page.matches(&query)) {
            this.page = page;
          }
          cx.notify();
        }
        _ => {}
      }),
    ];
    Self {
      page: Page::named(page),
      store: Store(model),
      workspace,
      fallback_dir,
      projects,
      profiles,
      accounts,
      updates,
      search,
      scroll: ScrollHandle::new(),
      _subscriptions: subscriptions,
    }
  }

  /// Shows the page a caller names, when the window is already open.
  pub(crate) fn show_page(&mut self, name: &str, cx: &mut Context<Self>) {
    self.page = Page::named(name);
    cx.notify();
  }

  fn query(&self, cx: &App) -> String {
    self.search.read(cx).value().trim().to_lowercase()
  }

  /// One entry of the list on the left.
  fn nav_item(&self, page: Page, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let selected = self.page == page;
    let tint = if selected { palette.explorer_selected_text } else { cx.theme().muted_foreground };
    h_flex()
      .id(SharedString::from(format!("settings-page-{}", page.label())))
      .gap_2()
      .mx_2()
      .px_2()
      .py_1p5()
      .items_center()
      .rounded_md()
      .cursor_pointer()
      .when(selected, |row| row.bg(palette.explorer_selected))
      .when(!selected, |row| row.hover(|row| row.bg(palette.hover)))
      .child(Icon::empty().path(page.icon()).size_4().text_color(tint))
      .child(Label::new(page.label()).text_color(if selected { palette.explorer_selected_text } else { palette.text }))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.page = page;
        cx.notify();
      }))
  }

  fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let query = self.query(cx);
    let mut any = false;
    let mut list = v_flex().gap_0p5().pb_3();
    for (heading, pages) in SECTIONS {
      let shown: Vec<Page> = pages.iter().copied().filter(|page| page.matches(&query)).collect();
      if shown.is_empty() {
        continue;
      }
      any = true;
      list = list
        .child(
          div()
            .px_4()
            .pt_3()
            .pb_1()
            .text_size(FONT_SIZE_SM)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(palette.text_secondary)
            .child(heading.to_uppercase()),
        )
        .children(shown.into_iter().map(|page| self.nav_item(page, cx)));
    }
    v_flex()
      .w_56()
      .flex_none()
      .h_full()
      .bg(cx.theme().sidebar)
      .border_r_1()
      .border_color(palette.border)
      .child(v_flex().gap_3().p_4().child(soft_input(&self.search, cx).prefix(Icon::empty().path("icons/search.svg").size_4())))
      .child(
        div()
          .id("settings-pages")
          .flex_1()
          .min_h_0()
          .overflow_y_scroll()
          .child(list)
          .when(!any, |column| column.child(div().px_4().py_2().text_color(palette.text_secondary).child("No settings match."))),
      )
  }

  /// The theme: three options side by side.
  fn theme_control(&self, cx: &App) -> impl IntoElement {
    let current = self.store.0.read(cx).preferences.theme;
    let palette = colors(cx);
    h_flex().gap_1().children([("System", ThemeChoice::System), ("Light", ThemeChoice::Light), ("Dark", ThemeChoice::Dark)].map(|(label, choice)| {
      let store = self.store.clone();
      Button::new(SharedString::from(format!("theme-{label}")))
        .label(label)
        .small()
        .when(current == choice, |button| {
          // The kit paints a custom button's own background at a fifth of its strength, so the fill is set apart.
          button
            .custom(
              ButtonCustomVariant::new(cx)
                .color(palette.accent)
                .foreground(palette.on_accent)
                .hover(palette.accent_hover)
                .active(palette.accent_hover),
            )
            .bg(palette.accent)
        })
        .when(current != choice, |button| button.outline())
        .on_click(move |_, _, cx| store.update(cx, |preferences| preferences.theme = choice))
    }))
  }

  /// The choice of fonts for the interface and the code.
  fn font_control(&self, cx: &App) -> impl IntoElement {
    let current = self.store.0.read(cx).preferences.font;
    let palette = colors(cx);
    h_flex().gap_1().children([("GitHub", FontChoice::GitHub), ("Geist", FontChoice::Geist), ("Inter", FontChoice::Inter)].map(|(label, choice)| {
      let store = self.store.clone();
      Button::new(SharedString::from(format!("font-{label}")))
        .label(label)
        .small()
        .when(current == choice, |button| {
          // The kit paints a custom button's own background at a fifth of its strength, so the fill is set apart.
          button
            .custom(
              ButtonCustomVariant::new(cx)
                .color(palette.accent)
                .foreground(palette.on_accent)
                .hover(palette.accent_hover)
                .active(palette.accent_hover),
            )
            .bg(palette.accent)
        })
        .when(current != choice, |button| button.outline())
        .on_click(move |_, _, cx| store.update(cx, |preferences| preferences.font = choice))
    }))
  }

  /// A size in pixels with buttons to make it smaller and larger, and one to bring back the default.
  fn size_control(&self, id: &'static str, default: u32, size: fn(&mut Preferences) -> &mut u32, cx: &App) -> impl IntoElement {
    let value = *size(&mut self.store.preferences(cx));
    let (min, max) = (*FONT_SIZES.start(), *FONT_SIZES.end());
    let step = |delta: i32, enabled: bool, icon: &'static str| {
      let store = self.store.clone();
      Button::new(SharedString::from(format!("{id}-{icon}")))
        .outline()
        .small()
        .icon(Icon::empty().path(format!("icons/{icon}.svg")))
        .disabled(!enabled)
        .on_click(move |_, _, cx| {
          store.update(cx, |preferences| {
            let target = size(preferences);
            *target = target.saturating_add_signed(delta).clamp(min, max);
          })
        })
    };
    let reset = self.store.clone();
    h_flex()
      .gap_1()
      .items_center()
      .child(step(-1, value > min, "minus"))
      .child(div().w_12().text_center().child(format!("{value} px")))
      .child(step(1, value < max, "plus"))
      .when(value != default, |row| {
        row.child(
          Button::new(SharedString::from(format!("{id}-reset")))
            .ghost()
            .small()
            .label("Reset")
            .on_click(move |_, _, cx| reset.update(cx, |preferences| *size(preferences) = default)),
        )
      })
  }

  fn general(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    card(&palette, vec![row("Theme", "System follows the macOS appearance.", self.theme_control(cx), &palette)])
  }

  fn editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let code = self.store.0.read(cx).preferences.code_font_size;
    let theme = cx.theme().clone();
    let preview = div()
      .px_4()
      .py_3()
      .rounded_md()
      .border_1()
      .border_color(palette.border)
      .bg(palette.surface)
      .font_family(theme.mono_font_family.clone())
      .text_size(px(code as f32))
      .text_color(palette.text)
      .child("fn main() { println!(\"Hello, world\"); }");
    v_flex()
      .gap_4()
      .child(card(
        &palette,
        vec![
          row(
            "Font",
            "GitHub: the system font and Menlo. Geist: Geist and Geist Mono. Inter: Inter, with Geist Mono for code.",
            self.font_control(cx),
            &palette,
          ),
          row(
            "Code font size",
            "The size of code in the editor and in a file's changes.",
            self.size_control("code-size", DEFAULT_CODE_FONT_SIZE, |preferences| &mut preferences.code_font_size, cx),
            &palette,
          ),
          row(
            "Explorer font size",
            "The size of file and folder names in Explorer.",
            self.size_control("explorer-size", DEFAULT_EXPLORER_FONT_SIZE, |preferences| &mut preferences.explorer_font_size, cx),
            &palette,
          ),
        ],
      ))
      .child(preview)
  }

  fn activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let layout = activity_layout(&self.store.0, cx);
    let count = layout.len();
    let rows: Vec<Div> = layout
      .into_iter()
      .enumerate()
      .map(|(index, (id, shown))| div().px_4().py_1p5().child(activity_row(&self.store, index, count, id, shown, cx)))
      .collect();
    let store = self.store.clone();
    v_flex().gap_3().child(card(&palette, rows)).child(
      h_flex().child(
        Button::new("activity-reset")
          .label("Reset to default")
          .small()
          .outline()
          .on_click(move |_, _, cx| store.update(cx, |preferences| preferences.activity_bar = Default::default())),
      ),
    )
  }

  fn projects_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let chosen = self.store.0.read(cx).preferences.projects_dir.clone();
    let path = chosen.clone().unwrap_or_else(|| self.fallback_dir.clone());
    let (choose, reset, workspace) = (self.store.clone(), self.store.clone(), self.workspace.clone());
    let folder = h_flex()
      .gap_2()
      .items_center()
      .child(div().max_w_64().truncate().text_color(palette.text_secondary).child(display_path(&path)))
      .child(
        Button::new("choose-projects-folder").label("Change…").small().outline().on_click(move |_, _, cx| choose_projects_folder(choose.clone(), cx)),
      )
      .when(chosen.is_some(), |row| {
        row.child(
          Button::new("reset-projects-folder")
            .label("Reset")
            .small()
            .ghost()
            .on_click(move |_, _, cx| reset.update(cx, |preferences| preferences.projects_dir = None)),
        )
      });
    card(
      &palette,
      vec![
        row("Projects folder", "New and cloned projects are created here, and the Add project dialog starts here.", folder, &palette),
        row(
          "Add every project in the folder",
          "Adds every Git repository directly inside the projects folder to the project menu.",
          Button::new("add-all-projects")
            .label("Add all")
            .small()
            .outline()
            .on_click(move |_, window, cx| workspace.update(cx, |workspace, cx| workspace.add_projects_from_folder(window, cx))),
          &palette,
        ),
      ],
    )
  }

  fn about(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let value = |text: String| div().max_w_80().truncate().text_color(palette.text_secondary).child(text);
    let checking = self.store.0.read(cx).preferences.check_updates;
    let store = self.store.clone();
    let info = card(
      &palette,
      vec![
        plain_row("Version", value(env!("CARGO_PKG_VERSION").into()), &palette),
        plain_row("Data folder", value(display_path(&ws_storage::data_dir())), &palette),
      ],
    );
    let launch_check = card(
      &palette,
      vec![row(
        "Check for updates on launch",
        format!(
          "{} asks GitHub (api.github.com) for the latest release of its repository. Nothing about you or your projects is sent. An update is only downloaded when you choose to.",
          ws_product::NAME
        ),
        Switch::new("check-updates").checked(checking).on_click(move |value, _, cx| store.update(cx, |preferences| preferences.check_updates = *value)),
        &palette,
      )],
    );
    // The links come from the repository the releases are published from.
    let links = ws_updater::repository_slug().map(|slug| {
      let link = |id: &'static str, title: &'static str, address: String| {
        let open = address.clone();
        plain_row(
          title,
          Button::new(id)
            .label(address.trim_start_matches("https://").to_string())
            .small()
            .ghost()
            .icon(Icon::empty().path("icons/external-link.svg"))
            .on_click(move |_, _, cx| cx.open_url(&open)),
          &palette,
        )
      };
      let base = format!("https://github.com/{slug}");
      card(
        &palette,
        vec![
          link("link-repository", "Repository", base.clone()),
          link("link-releases", "Releases", format!("{base}/releases")),
          link("link-issues", "Report an issue", format!("{base}/issues")),
        ],
      )
    });
    v_flex().gap_4().child(self.updates.clone()).child(launch_check).child(info).children(links)
  }
}

/// A setting: its name and what it does at the left, its control at the right.
fn row(title: &'static str, description: impl Into<SharedString>, control: impl IntoElement, palette: &Colors) -> Div {
  h_flex()
    .gap_6()
    .px_4()
    .py_3()
    .items_center()
    .justify_between()
    .child(
      v_flex()
        .gap_0p5()
        .flex_1()
        .min_w_0()
        .child(Label::new(title).font_weight(FontWeight::MEDIUM))
        .child(Label::new(description.into()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary)),
    )
    .child(div().flex_none().child(control))
}

/// A row that only states a value.
fn plain_row(title: &'static str, value: impl IntoElement, palette: &Colors) -> Div {
  h_flex().gap_6().px_4().py_3().items_center().justify_between().child(Label::new(title).text_color(palette.text_secondary)).child(value)
}

/// Rows in one bordered box, separated by lines.
fn card(palette: &Colors, rows: Vec<Div>) -> Div {
  let last = rows.len().saturating_sub(1);
  v_flex()
    .w_full()
    .rounded_md()
    .border_1()
    .border_color(palette.border)
    .overflow_hidden()
    .children(rows.into_iter().enumerate().map(|(index, row)| row.when(index < last, |row| row.border_b_1().border_color(palette.border))))
}

impl Render for SettingsWindow {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let content = match self.page {
      Page::General => self.general(cx).into_any_element(),
      Page::Editor => self.editor(cx).into_any_element(),
      Page::ActivityBar => self.activity_bar(cx).into_any_element(),
      Page::Projects => v_flex().gap_4().child(self.projects_page(cx)).child(self.projects.clone()).into_any_element(),
      Page::Profiles => self.profiles.clone().into_any_element(),
      Page::Accounts => self.accounts.clone().into_any_element(),
      Page::About => self.about(cx).into_any_element(),
    };
    let page = self.page;
    // The title scrolls with the page, and the scrollbar runs the whole height of the view.
    let header = v_flex()
      .gap_1()
      .pt_6()
      .pb_4()
      .child(div().text_lg().font_semibold().child(page.label()))
      .child(Label::new(page.description()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary));
    let scrolling = div()
      .relative()
      .flex_1()
      .min_h_0()
      .child(
        div()
          .id("settings-body")
          .size_full()
          .overflow_y_scroll()
          .track_scroll(&self.scroll)
          .child(v_flex().w_full().px_8().pb_6().child(header).child(content)),
      )
      .child(div().absolute().inset_0().child(
        Scrollbar::new(&self.scroll).id("settings-scrollbar").axis(ScrollbarAxis::Vertical).mode(ScrollbarMode::Always).viewport_from_layout(),
      ));
    let body = v_flex().flex_1().min_w_0().h_full().child(scrolling);
    h_flex().size_full().bg(palette.background).text_size(FONT_SIZE).child(self.sidebar(cx)).child(body)
  }
}
