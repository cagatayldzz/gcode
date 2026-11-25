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
    h_flex,
    input::{InputEvent, InputState},
    label::Label,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
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
  let Some(extension) = extensions.0.iter().find(|extension| extension.manifest.id == ws_extensions::AI_AGENT) else { return Vec::new() };
  AgentKind::ALL.into_iter().filter(|kind| extension.manifest.agents.iter().any(|id| id == kind.id())).collect()
}

/// Sent when an extension was installed or removed.
pub(crate) struct ExtensionsChanged;

/// The Extensions view: what is installed, and what the marketplace offers.
pub(crate) struct ExtensionsPanel {
  search: Entity<InputState>,
  /// The extensions the marketplace lists; read when the view is first shown, and again on request.
  available: Load<Vec<Available>>,
  loaded: bool,
  /// The extensions being installed or removed right now, by id.
  busy: HashSet<String>,
  list_task: Option<Task<()>>,
  _search_subscription: Subscription,
}

impl EventEmitter<ExtensionsChanged> for ExtensionsPanel {}

impl ExtensionsPanel {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search extensions in the marketplace"));
    let subscription = cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify());
    Self { search, available: Load::Loading, loaded: false, busy: HashSet::new(), list_task: None, _search_subscription: subscription }
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
  fn install(&mut self, available: Available, window: &mut Window, cx: &mut Context<Self>) {
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

  fn uninstall(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
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

  fn item(&self, id: &str, name: &str, description: &str, meta: String, icon: &str, action: AnyElement, cx: &App) -> impl IntoElement {
    let palette = colors(cx);
    h_flex()
      .id(SharedString::from(format!("extension-{id}")))
      .gap_3()
      .px_3()
      .py_2()
      .items_start()
      .child(
        div()
          .flex_none()
          .size_10()
          .rounded_lg()
          .border_1()
          .border_color(palette.border)
          .bg(palette.surface_alt)
          .flex()
          .items_center()
          .justify_center()
          .child(Icon::empty().path(SharedString::from(icon.to_string())).size_5().text_color(palette.text)),
      )
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_0p5()
          .child(Label::new(name.to_string()).font_weight(FontWeight::SEMIBOLD).truncate())
          .child(Label::new(description.to_string()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
          .child(Label::new(meta).text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
          .child(div().pt_1().child(action)),
      )
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
    let matches = |name: &str, description: &str, publisher: &str| {
      query.is_empty() || [name, description, publisher].iter().any(|text| text.to_lowercase().contains(&query))
    };
    let installed = installed(cx);
    let listed: &[Available] = match &self.available {
      Load::Ready(listed) => listed,
      _ => &[],
    };
    let button = |id: String, label: &'static str, primary: bool, busy: bool| {
      let button = Button::new(SharedString::from(format!("{label}-{id}"))).xsmall().label(label).disabled(busy);
      if primary {
        button.primary()
      } else {
        button.outline()
      }
    };
    let busy_mark = || {
      h_flex()
        .gap_2()
        .items_center()
        .child(Spinner::new().small())
        .child(Label::new("Working…").text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
    };

    // What is installed, with the button that brings it to the version the marketplace has.
    let installed_rows: Vec<AnyElement> = installed
      .iter()
      .filter(|extension| {
        let manifest = &extension.manifest;
        matches(&manifest.name, &manifest.description, &manifest.publisher)
      })
      .map(|extension| {
        let manifest = &extension.manifest;
        let id = manifest.id.clone();
        let busy = self.busy.contains(&id);
        let newer = listed.iter().find(|available| available.id == id && available.version != manifest.version).cloned();
        let icon = listed.iter().find(|available| available.id == id).map_or("icons/package.svg", |available| available.icon.as_str());
        let action = if busy {
          busy_mark().into_any_element()
        } else {
          let remove_id = id.clone();
          h_flex()
            .gap_2()
            .children(newer.map(|available| {
              button(id.clone(), "Update", true, false).on_click(cx.listener(move |this, _, window, cx| this.install(available.clone(), window, cx)))
            }))
            .child(
              button(id.clone(), "Uninstall", false, false)
                .on_click(cx.listener(move |this, _, window, cx| this.uninstall(remove_id.clone(), window, cx))),
            )
            .into_any_element()
        };
        let meta = [manifest.publisher.as_str(), &format!("v{}", manifest.version)]
          .iter()
          .filter(|part| part.len() > 1)
          .copied()
          .collect::<Vec<_>>()
          .join(" · ");
        self.item(&id, &manifest.name, &manifest.description, meta, icon, action, cx).into_any_element()
      })
      .collect();

    // What the marketplace has that is not installed yet.
    let marketplace_rows: Vec<AnyElement> = listed
      .iter()
      .filter(|available| !installed.iter().any(|extension| extension.manifest.id == available.id))
      .filter(|available| matches(&available.name, &available.description, &available.publisher))
      .map(|available| {
        let busy = self.busy.contains(&available.id);
        let action = if busy {
          busy_mark().into_any_element()
        } else {
          let install = available.clone();
          button(available.id.clone(), "Install", true, false)
            .on_click(cx.listener(move |this, _, window, cx| this.install(install.clone(), window, cx)))
            .into_any_element()
        };
        let meta = [available.publisher.as_str(), &format!("v{}", available.version)]
          .iter()
          .filter(|part| part.len() > 1)
          .copied()
          .collect::<Vec<_>>()
          .join(" · ");
        self.item(&available.id, &available.name, &available.description, meta, &available.icon, action, cx).into_any_element()
      })
      .collect();

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
      .child(div().px_3().pb_1().child(soft_input(&self.search, cx).bg(palette.background)))
      .child(
        v_flex()
          .id("extensions-list")
          .flex_1()
          .min_h_0()
          .overflow_y_scroll()
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
      )
  }
}
