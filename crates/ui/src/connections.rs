//! Connected accounts (GitHub, GitLab, Azure DevOps): the helpers every part of the interface shares, and the list in Settings.

use crate::{
  settings::SettingsModel,
  theme::{colors, FONT_SIZE_SM},
  workspace::Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
  },
  *,
};
use std::{collections::HashSet, path::Path};
use ws_hosting::{Account, Accounts, Provider};

/// The icon of a hosting service.
pub(crate) fn provider_icon(provider: Provider) -> &'static str {
  match provider {
    Provider::GitHub => "icons/github.svg",
    Provider::GitLab => "icons/server.svg",
    Provider::AzureDevOps => "icons/cloud.svg",
  }
}

/// The login Git gets for an HTTPS address: the connected account that serves its host. `None` for an SSH address (the user's
/// own keys apply) or when no account serves the host. Reads the Keychain, so call it off the interface thread.
pub(crate) fn credentials_for_url(accounts: &Accounts, url: &str) -> Option<ws_git::Credentials> {
  if !url.starts_with("http") {
    return None;
  }
  let (connection, _) = accounts.for_remote(url, &accounts.keychain())?;
  let (username, password) = connection.git_login();
  Some(ws_git::Credentials { username, password })
}

/// The login Git gets for the remote of the repository at `root` (see [`credentials_for_url`]).
pub(crate) fn credentials_for_repository(accounts: &Accounts, root: &Path) -> Option<ws_git::Credentials> {
  credentials_for_url(accounts, &ws_git::remote_url(root)?)
}

/// The accounts in Settings: one row each with a button to disconnect it, and a button to connect another.
pub(crate) struct AccountsList {
  model: Entity<SettingsModel>,
  /// The window that does what the buttons ask; the dialogs open in the window the button is in.
  workspace: Entity<Workspace>,
  /// Keys of the accounts whose permissions are shown under their row.
  expanded: HashSet<String>,
  _observe: Subscription,
}

impl AccountsList {
  pub fn new(model: Entity<SettingsModel>, workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
    // The list changes when an account is connected or removed in any window of the profile.
    let observe = cx.observe(&model, |_, _, cx| cx.notify());
    Self { model, workspace, expanded: HashSet::new(), _observe: observe }
  }
}

impl Render for AccountsList {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let (accounts, rechecking) = {
      let shared = self.model.read(cx);
      (shared.accounts.items.clone(), shared.rechecking.clone())
    };
    let rows = accounts.iter().map(|account| {
      let key = account.key();
      let checking = rechecking.contains(&key);
      let open = self.expanded.contains(&key);
      let row = h_flex()
        .id(SharedString::from(format!("account-{key}")))
        .gap_3()
        .px_3()
        .py_2()
        .items_center()
        .rounded_md()
        .bg(palette.surface_alt)
        .child(
          Button::new(SharedString::from(format!("account-permissions-{key}")))
            .small()
            .ghost()
            .icon(Icon::empty().path(if open { "icons/chevron-down.svg" } else { "icons/chevron-right.svg" }))
            .tooltip(if open { "Hide permissions" } else { "Show permissions" })
            .on_click({
              let key = key.clone();
              cx.listener(move |this, _, _, cx| {
                if !this.expanded.remove(&key) {
                  this.expanded.insert(key.clone());
                }
                cx.notify();
              })
            }),
        )
        .child(Icon::empty().path(provider_icon(account.provider)).size_5().text_color(theme.muted_foreground))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .child(Label::new(account.provider.label()).font_weight(FontWeight::SEMIBOLD))
            .child(Label::new(account.label()).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground)),
        )
        .child(Button::new(SharedString::from(format!("account-reconnect-{key}"))).label("Reconnect").small().outline().on_click({
          let key = key.clone();
          let model = self.model.clone();
          let workspace = self.workspace.clone();
          move |_, window, cx| {
            let existing = model.read(cx).accounts.items.iter().find(|account| account.key() == key).cloned();
            workspace.update(cx, |workspace, cx| workspace.open_connect_account(existing, window, cx));
          }
        }))
        .child(Button::new(SharedString::from(format!("account-remove-{key}"))).label("Disconnect").small().outline().on_click({
          let key = key.clone();
          let workspace = self.workspace.clone();
          move |_, window, cx| {
            let key = key.clone();
            workspace.update(cx, |workspace, cx| workspace.remove_account(key, window, cx));
          }
        }));
      // While a permission is missing, an alert stays under the account until a check finds it fixed.
      let alert = account.issue.clone().map(|issue| {
        let key = key.clone();
        let workspace = self.workspace.clone();
        let tokens_url = account.tokens_url();
        let guide = account.provider.token_guide();
        let needed = guide.permissions.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ");
        v_flex()
          .gap_2()
          .p_3()
          .rounded_md()
          .border_1()
          .border_color(theme.warning)
          .bg(theme.warning.opacity(0.08))
          .child(
            h_flex()
              .gap_2()
              .items_center()
              .child(Icon::empty().path("icons/triangle-alert.svg").size_4().text_color(theme.warning))
              .child(Label::new("This account needs attention").font_weight(FontWeight::SEMIBOLD)),
          )
          .child(Label::new(issue).text_size(FONT_SIZE_SM))
          .child(
            Label::new(format!("Create a new token with {needed}, disconnect this account and connect it again, or fix the permissions of the current token. Then check again."))
              .text_size(FONT_SIZE_SM)
              .text_color(theme.muted_foreground),
          )
          .child(
            h_flex().gap_2().items_center().child(
              Button::new(SharedString::from(format!("account-recheck-{key}")))
                .label(if checking { "Checking…" } else { "Check again" })
                .small()
                .primary()
                .loading(checking)
                .disabled(checking)
                .on_click(move |_, window, cx| {
                  let key = key.clone();
                  workspace.update(cx, |workspace, cx| workspace.recheck_account(key, window, cx));
                }),
            ).child(
              Button::new(SharedString::from(format!("account-tokens-{}", account.key())))
                .small()
                .outline()
                .icon(Icon::empty().path("icons/external-link.svg"))
                .tooltip("Open the token page in the browser")
                .on_click(move |_, _, cx| cx.open_url(&tokens_url)),
            ),
          )
      });
      // What the app uses the token for, and whether the last check found it lacking.
      let permissions = open.then(|| {
        let guide = account.provider.token_guide();
        let status = match &account.issue {
          Some(_) => ("icons/triangle-alert.svg", theme.warning, "A permission is missing"),
          None => ("icons/check.svg", theme.success, "No missing permission found"),
        };
        v_flex()
          .gap_1p5()
          .px_3()
          .py_2()
          .ml_8()
          .rounded_md()
          .border_1()
          .border_color(palette.border)
          .child(
            h_flex()
              .gap_2()
              .items_center()
              .child(Icon::empty().path(status.0).size_4().text_color(status.1))
              .child(Label::new(status.2).text_size(FONT_SIZE_SM).font_weight(FontWeight::SEMIBOLD)),
          )
          .children(guide.permissions.iter().map(|(name, usage)| {
            v_flex()
              .child(Label::new(*name).text_size(FONT_SIZE_SM).font_weight(FontWeight::MEDIUM))
              .child(Label::new(*usage).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
          }))
          .child(Label::new(guide.note).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
      });
      v_flex().gap_1().child(row).children(permissions).children(alert)
    });
    v_flex()
      .gap_2()
      .child(h_flex().child(
        Button::new("connect-account").label("Connect an account…").icon(Icon::empty().path("icons/plus.svg")).small().outline().on_click({
          let workspace = self.workspace.clone();
          move |_, window, cx| workspace.update(cx, |workspace, cx| workspace.open_connect_account(None, window, cx))
        }),
      ))
      .children(rows)
      .when(accounts.is_empty(), |list| {
        list.child(Label::new("No account is connected yet.").text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
      })
  }
}

/// How an account is named in a message.
pub(crate) fn describe(account: &Account) -> String {
  format!("{} ({})", account.provider.label(), account.label())
}
