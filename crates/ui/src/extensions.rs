//! Extensions: optional features that are not installed until the user installs them from the Extensions view of the sidebar. What is
//! installed is read from the data folder and shared by every window; the AI agents are the first feature that needs an extension.

use crate::{
  agents::AgentKind,
  commit_view::Load,
  components::soft_input,
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    empty::{Empty, EmptyHeader, EmptyTitle},
    h_flex,
    input::{InputEvent, InputState},
    label::Label,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
  },
  *,
};
use std::collections::HashSet;
use ws_extensions::{Available, Installed};

/// The extensions that are installed, for every window: installing or removing one in a window shows in all of them.
struct InstalledExtensions(Vec<Installed>);

impl Global for InstalledExtensions {}

/// Reads what is installed, once, when the app starts.
pub(crate) fn init(cx: &mut App) {
  cx.set_global(InstalledExtensions(ws_extensions::installed()));
}

fn installed(cx: &App) -> Vec<Installed> {
  cx.try_global::<InstalledExtensions>().map(|extensions| extensions.0.clone()).unwrap_or_default()
}

/// The agents that the AI Agent extension switches on; none while it is not installed.
pub(crate) fn enabled_agents(cx: &App) -> Vec<AgentKind> {
  let Some(extensions) = cx.try_global::<InstalledExtensions>() else { return Vec::new() };
  let Some(extension) = extensions.0.iter().find(|extension| extension.enabled && extension.manifest.id == ws_extensions::AI_AGENT) else {
    return Vec::new();
  };
  AgentKind::ALL.into_iter().filter(|kind| extension.manifest.agents.iter().any(|id| id == kind.id())).collect()
}

/// Whether the extension `id` is installed and switched on.
pub(crate) fn is_enabled(id: &str, cx: &App) -> bool {
  cx.try_global::<InstalledExtensions>()
    .is_some_and(|extensions| extensions.0.iter().any(|extension| extension.enabled && extension.manifest.id == id))
}

/// Sent when an extension was installed or removed.
pub(crate) struct ExtensionsChanged;

/// Sent when another extension was chosen in the list, or its page was closed.
pub(crate) struct ExtensionSelected;

/// An extension as the list and its page show it: what is installed, joined with what the marketplace says about it.
#[derive(Clone)]
pub(crate) struct Entry {
  pub id: String,
  pub name: String,
  pub description: String,
  pub publisher: String,
  pub version: String,
  pub icon: String,
  /// Only a sketch in the marketplace: it cannot be installed yet.
  pub draft: bool,
  /// `Some(enabled)` while the extension is installed.
  pub installed: Option<bool>,
  /// The marketplace's package, when its version is higher than the installed one.
  pub update: Option<Available>,
  /// The agents the extension switches on, by their names.
  pub agents: Vec<String>,
}

/// The Extensions view: what is installed, and what the marketplace offers.
pub(crate) struct ExtensionsPanel {
  search: Entity<InputState>,
  /// The extensions the marketplace lists; read when the view is first shown, and again on request.
  available: Load<Vec<Available>>,
  loaded: bool,
  /// The extensions being installed or removed right now, by id.
  busy: HashSet<String>,
  list_task: Option<Task<()>>,
  /// The scroll position of the list, which has a scrollbar strip of its own.
  list_scroll: ScrollHandle,
  /// The extension whose page shows in the main area.
  selected: Option<String>,
  _search_subscription: Subscription,
}

impl EventEmitter<ExtensionsChanged> for ExtensionsPanel {}
impl EventEmitter<ExtensionSelected> for ExtensionsPanel {}

impl ExtensionsPanel {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search extensions in the marketplace"));
    let subscription = cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify());
    Self {
      search,
      available: Load::Loading,
      loaded: false,
      busy: HashSet::new(),
      list_task: None,
      list_scroll: ScrollHandle::new(),
      selected: None,
      _search_subscription: subscription,
    }
  }

  /// Reads the marketplace the first time the view is shown.
  pub fn ensure_loaded(&mut self, cx: &mut Context<Self>) {
    if !self.loaded {
      self.reload(cx);
    }
  }

  /// Reads the marketplace again.
  pub fn reload(&mut self, cx: &mut Context<Self>) {
    self.loaded = true;
    self.available = Load::Loading;
    let work = cx.background_executor().spawn(async { ws_extensions::fetch_index() });
    self.list_task = Some(cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.list_task = None;
        this.available = match result {
          Ok(listed) => Load::Ready(listed),
          Err(error) => Load::Failed(error.to_string().into()),
        };
        cx.notify();
      });
    }));
    cx.notify();
  }

  /// Installs `available`, or brings the installed one to this version.
  pub(crate) fn install(&mut self, available: Available, window: &mut Window, cx: &mut Context<Self>) {
    if !self.busy.insert(available.id.clone()) {
      return;
    }
    cx.notify();
    let id = available.id.clone();
    let work = cx.background_executor().spawn(async move { ws_extensions::install(&available) });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.busy.remove(&id);
        this.finished(result.map(|_| ()).map_err(|error| error.to_string()), window, cx);
      });
    })
    .detach();
  }

  /// Switches an installed extension on or off, for every window.
  pub(crate) fn set_enabled(&mut self, id: &str, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
    let result = ws_extensions::set_enabled(id, enabled).map_err(|error| error.to_string());
    self.finished(result, window, cx);
  }

  pub(crate) fn uninstall(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    if !self.busy.insert(id.clone()) {
      return;
    }
    cx.notify();
    let removed = id.clone();
    let work = cx.background_executor().spawn(async move { ws_extensions::uninstall(&removed) });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.busy.remove(&id);
        this.finished(result.map_err(|error| error.to_string()), window, cx);
      });
    })
    .detach();
  }

  /// What follows an install or a removal: the failure is shown, and what is installed is read again for every window.
  fn finished(&mut self, result: Result<(), String>, window: &mut Window, cx: &mut Context<Self>) {
    if let Err(message) = result {
      window.push_notification(crate::failure(message), cx);
    }
    cx.set_global(InstalledExtensions(ws_extensions::installed()));
    cx.emit(ExtensionsChanged);
    cx.refresh_windows();
    cx.notify();
  }

  /// What the marketplace has listed so far.
  fn listed(&self) -> &[Available] {
    match &self.available {
      Load::Ready(listed) => listed,
      _ => &[],
    }
  }

  /// The installed extensions and the ones only the marketplace has.
  fn entries(&self, cx: &App) -> (Vec<Entry>, Vec<Entry>) {
    let listed = self.listed();
    let installed = installed(cx);
    let agent_names = |ids: &[String]| -> Vec<String> {
      ids.iter().map(|id| AgentKind::ALL.iter().find(|kind| kind.id() == id).map_or_else(|| id.clone(), |kind| kind.label().to_string())).collect()
    };
    let have: Vec<Entry> = installed
      .iter()
      .map(|extension| {
        let manifest = &extension.manifest;
        let known = listed.iter().find(|available| available.id == manifest.id);
        Entry {
          id: manifest.id.clone(),
          name: manifest.name.clone(),
          description: manifest.description.clone(),
          publisher: manifest.publisher.clone(),
          version: manifest.version.clone(),
          icon: known.map_or("icons/package.svg", |available| available.icon.as_str()).to_string(),
          draft: false,
          installed: Some(extension.enabled),
          update: known.filter(|available| ws_extensions::is_newer(&available.version, &manifest.version)).cloned(),
          agents: agent_names(&manifest.agents),
        }
      })
      .collect();
    let more: Vec<Entry> = listed
      .iter()
      .filter(|available| !installed.iter().any(|extension| extension.manifest.id == available.id))
      .map(|available| Entry {
        id: available.id.clone(),
        name: available.name.clone(),
        description: available.description.clone(),
        publisher: available.publisher.clone(),
        version: available.version.clone(),
        icon: available.icon.clone(),
        draft: available.draft,
        installed: None,
        update: (!available.draft).then(|| available.clone()),
        agents: Vec::new(),
      })
      .collect();
    (have, more)
  }

  /// The chosen extension, as it is now; `None` when nothing is chosen or it is gone (removed, or not in the marketplace).
  pub(crate) fn selected_entry(&self, cx: &App) -> Option<Entry> {
    let id = self.selected.as_deref()?;
    let (have, more) = self.entries(cx);
    have.into_iter().chain(more).find(|entry| entry.id == id)
  }

  pub(crate) fn has_selection(&self) -> bool {
    self.selected.is_some()
  }

  pub(crate) fn is_busy(&self, id: &str) -> bool {
    self.busy.contains(id)
  }

  /// Shows the page of an extension in the main area, or closes it with `None`.
  pub(crate) fn select(&mut self, id: Option<String>, cx: &mut Context<Self>) {
    if self.selected != id {
      self.selected = id;
      cx.emit(ExtensionSelected);
      cx.notify();
    }
  }

  /// One compact row: the icon, the name, a line of what it is, and who publishes it. Choosing it shows its page.
  fn row(&self, entry: &Entry, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let selected = self.selected.as_deref() == Some(entry.id.as_str());
    let id = entry.id.clone();
    let tint = |text: &'static str, color: Hsla| {
      div().flex_none().px_1p5().rounded_md().bg(color.opacity(0.14)).text_size(FONT_SIZE_SM).text_color(color).child(text)
    };
    let mark: Option<AnyElement> = if self.busy.contains(&entry.id) {
      Some(Spinner::new().small().into_any_element())
    } else if entry.draft {
      Some(tint("Soon", palette.text_secondary).into_any_element())
    } else if entry.installed == Some(false) {
      Some(tint("Disabled", palette.text_secondary).into_any_element())
    } else if entry.installed.is_some() && entry.update.is_some() {
      Some(tint("Update", cx.theme().primary).into_any_element())
    } else {
      None
    };
    let meta =
      [entry.publisher.as_str(), &format!("v{}", entry.version)].iter().filter(|part| part.len() > 1).copied().collect::<Vec<_>>().join(" · ");
    h_flex()
      .id(SharedString::from(format!("extension-{}", entry.id)))
      .gap_3()
      .px_3()
      .py_2()
      .items_center()
      .border_b_1()
      .border_color(crate::theme::row_divider(&palette))
      .cursor_pointer()
      .when(selected, |row| row.bg(palette.selected))
      .hover(|row| row.bg(if selected { palette.selected_hover } else { palette.hover }))
      .child(
        div()
          .flex_none()
          .size_8()
          .rounded_md()
          .border_1()
          .border_color(palette.border)
          .bg(palette.surface_alt)
          .flex()
          .items_center()
          .justify_center()
          .child(Icon::empty().path(SharedString::from(entry.icon.clone())).size_4().text_color(palette.text)),
      )
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .child(
            h_flex()
              .gap_2()
              .items_center()
              .child(Label::new(entry.name.clone()).font_weight(FontWeight::SEMIBOLD).flex_1().min_w_0().truncate())
              .children(mark),
          )
          .child(Label::new(entry.description.clone()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary).truncate())
          .child(Label::new(meta).text_size(FONT_SIZE_SM).text_color(palette.text_secondary).truncate()),
      )
      .on_click(cx.listener(move |this, _, _, cx| this.select(Some(id.clone()), cx)))
  }

  fn heading(&self, title: &'static str, count: usize, cx: &App) -> impl IntoElement {
    let palette = colors(cx);
    h_flex()
      .gap_2()
      .px_3()
      .pt_3()
      .pb_1()
      .items_center()
      .child(Label::new(title).font_weight(FontWeight::SEMIBOLD))
      .child(Label::new(count.to_string()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
  }
}

impl Render for ExtensionsPanel {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let query = self.search.read(cx).value().trim().to_lowercase();
    let matches =
      |entry: &Entry| query.is_empty() || [&entry.name, &entry.description, &entry.publisher].iter().any(|text| text.to_lowercase().contains(&query));
    let (have, more) = self.entries(cx);
    let installed_rows: Vec<AnyElement> = have.iter().filter(|entry| matches(entry)).map(|entry| self.row(entry, cx).into_any_element()).collect();
    let marketplace_rows: Vec<AnyElement> = more.iter().filter(|entry| matches(entry)).map(|entry| self.row(entry, cx).into_any_element()).collect();

    let marketplace_state: Option<AnyElement> = match &self.available {
      Load::Loading => Some(
        h_flex()
          .gap_2()
          .px_3()
          .py_2()
          .items_center()
          .child(Spinner::new().small())
          .child(Label::new("Loading the marketplace…").text_color(theme.muted_foreground))
          .into_any_element(),
      ),
      Load::Failed(message) => Some(Label::new(message.clone()).px_3().py_2().text_color(theme.danger).into_any_element()),
      Load::Ready(_) if marketplace_rows.is_empty() => Some(
        Label::new(if query.is_empty() { "Nothing more to install." } else { "No matching extensions." })
          .px_3()
          .py_2()
          .text_color(theme.muted_foreground)
          .into_any_element(),
      ),
      Load::Ready(_) => None,
    };
    let overflows = self.list_scroll.max_offset().y > px(0.);
    v_flex()
      .size_full()
      .child(
        h_flex().px_3().pt_3().pb_2().items_center().justify_between().child(Label::new("Extensions").font_weight(FontWeight::SEMIBOLD)).child(
          Button::new("extensions-refresh")
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/refresh-cw.svg").size_4())
            .tooltip("Refresh")
            .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
        ),
      )
      .child(div().flex_none().px_3().pb_1().border_b_1().border_color(palette.border).child(soft_input(&self.search, cx).bg(palette.background)))
      // The scrollbar has a strip of its own beside the list, as in History, shown only while the rows are taller than the list.
      .child(
        h_flex()
          .flex_1()
          .min_h_0()
          .bg(palette.background)
          .child(
            div().flex_1().min_w_0().h_full().child(
              v_flex()
                .id("extensions-list")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.list_scroll)
                .child(self.heading("Installed", installed_rows.len(), cx))
                .when(installed_rows.is_empty(), |list| {
                  list.child(
                    Label::new(if query.is_empty() { "No extensions installed." } else { "No matching extensions." })
                      .px_3()
                      .py_2()
                      .text_color(theme.muted_foreground),
                  )
                })
                .children(installed_rows)
                .child(self.heading("Marketplace", marketplace_rows.len(), cx))
                .children(marketplace_state)
                .children(marketplace_rows),
            ),
          )
          .when(overflows, |row| row.child(crate::components::scrollbar_strip(&self.list_scroll, "extensions-scrollbar", cx))),
      )
  }
}

/// The width of the column of facts beside an extension's page.
const PAGE_FACTS_WIDTH: Pixels = px(260.);

/// The page of the chosen extension, in the main area: who it is from, what it does, and the buttons to install, update, switch it on or
/// off, or remove it.
pub(crate) struct ExtensionPage {
  panel: Entity<ExtensionsPanel>,
  _observer: Subscription,
}

impl ExtensionPage {
  pub fn new(panel: Entity<ExtensionsPanel>, cx: &mut Context<Self>) -> Self {
    let observer = cx.observe(&panel, |_, _, cx| cx.notify());
    Self { panel, _observer: observer }
  }

  /// A heading of a block of the page.
  fn heading(text: &'static str) -> Label {
    Label::new(text).text_lg().font_weight(FontWeight::SEMIBOLD)
  }

  /// A row of the facts column: a name and its value, striped like a table.
  fn fact(name: &'static str, value: String, striped: bool, cx: &App) -> impl IntoElement {
    let palette = colors(cx);
    h_flex()
      .gap_3()
      .px_2()
      .py_1()
      .when(striped, |row| row.bg(palette.surface))
      .child(Label::new(name).w_20().flex_none().text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
      .child(Label::new(value).flex_1().min_w_0().text_size(FONT_SIZE_SM))
  }
}

impl Render for ExtensionPage {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let Some(entry) = self.panel.read(cx).selected_entry(cx) else {
      return Empty::new()
        .size_full()
        .child(EmptyHeader::new().title(EmptyTitle::new().child("This extension is not available.")))
        .into_any_element();
    };
    let busy = self.panel.read(cx).is_busy(&entry.id);
    let button = |label: &'static str, primary: bool| {
      let button = Button::new(SharedString::from(format!("page-{label}-{}", entry.id))).small().label(label);
      if primary {
        button.primary()
      } else {
        button.outline()
      }
    };
    let id = entry.id.clone();
    let actions: AnyElement = if busy {
      h_flex()
        .gap_2()
        .items_center()
        .child(Spinner::new().small())
        .child(Label::new("Working…").text_color(palette.text_secondary))
        .into_any_element()
    } else if entry.draft {
      Label::new("Coming soon").text_color(palette.text_secondary).into_any_element()
    } else if let Some(enabled) = entry.installed {
      let (enable_id, remove_id) = (id.clone(), id.clone());
      h_flex()
        .gap_2()
        .children(entry.update.clone().map(|available| {
          button("Update", true).on_click(cx.listener(move |this, _, window, cx| {
            this.panel.update(cx, |panel, cx| panel.install(available.clone(), window, cx));
          }))
        }))
        .child(button(if enabled { "Disable" } else { "Enable" }, !enabled && entry.update.is_none()).on_click(cx.listener(
          move |this, _, window, cx| {
            this.panel.update(cx, |panel, cx| panel.set_enabled(&enable_id, !enabled, window, cx));
          },
        )))
        .child(button("Uninstall", false).on_click(cx.listener(move |this, _, window, cx| {
          this.panel.update(cx, |panel, cx| panel.uninstall(remove_id.clone(), window, cx));
        })))
        .into_any_element()
    } else {
      match entry.update.clone() {
        Some(available) => button("Install", true)
          .on_click(cx.listener(move |this, _, window, cx| this.panel.update(cx, |panel, cx| panel.install(available.clone(), window, cx))))
          .into_any_element(),
        None => Label::new("Not available").text_color(palette.text_secondary).into_any_element(),
      }
    };
    let status = match entry.installed {
      Some(true) if entry.update.is_some() => "Installed, an update is available",
      Some(true) => "Installed",
      Some(false) => "Installed, disabled",
      None if entry.draft => "In development",
      None => "Not installed",
    };
    let facts = [
      ("Identifier", entry.id.clone()),
      ("Version", format!("v{}", entry.version)),
      ("Publisher", entry.publisher.clone()),
      ("Status", status.to_string()),
    ];
    v_flex()
      .size_full()
      .bg(palette.background)
      .child(
        h_flex()
          .gap_4()
          .p_6()
          .items_start()
          .border_b_1()
          .border_color(palette.border)
          .child(
            div()
              .flex_none()
              .size_20()
              .rounded_xl()
              .border_1()
              .border_color(palette.border)
              .bg(palette.surface_alt)
              .flex()
              .items_center()
              .justify_center()
              .child(Icon::empty().path(SharedString::from(entry.icon.clone())).size_10().text_color(palette.text)),
          )
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_1p5()
              .child(Label::new(entry.name.clone()).text_2xl().font_weight(FontWeight::SEMIBOLD))
              .child(Label::new(format!("{} · v{}", entry.publisher, entry.version)).text_color(palette.text_secondary))
              .child(Label::new(entry.description.clone()))
              .child(div().pt_1().child(actions)),
          )
          .child(
            Button::new("extension-page-close")
              .ghost()
              .small()
              .icon(Icon::empty().path("icons/x.svg"))
              .tooltip("Close")
              .on_click(cx.listener(|this, _, _, cx| this.panel.update(cx, |panel, cx| panel.select(None, cx)))),
          ),
      )
      .child(
        h_flex()
          .id("extension-page-body")
          .flex_1()
          .min_h_0()
          .overflow_y_scroll()
          .items_start()
          .gap_8()
          .p_6()
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_3()
              .child(Self::heading("About"))
              .child(Label::new(entry.description.clone()))
              .when(!entry.agents.is_empty(), |column| {
                column.child(Self::heading("Contributes")).child(Label::new(format!("Agents: {}", entry.agents.join(", "))))
              }),
          )
          .child(
            v_flex().w(PAGE_FACTS_WIDTH).flex_none().gap_2().child(Self::heading("Installation")).child(
              v_flex()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .overflow_hidden()
                .children(facts.into_iter().enumerate().map(|(index, (name, value))| Self::fact(name, value, index % 2 == 0, cx))),
            ),
          ),
      )
      .into_any_element()
  }
}
