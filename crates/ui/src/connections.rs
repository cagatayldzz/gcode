//! Connected accounts (GitHub, GitLab, Azure DevOps): the helpers every part of the interface shares, and the list in Settings.

use crate::{
  settings::{AccountRemoveRequested, ConnectAccountRequested, SettingsGlobal, SettingsModel},
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{button::Button, h_flex, label::Label, v_flex, ActiveTheme as _, Icon, Sizable as _},
  *,
};
use std::path::{Path, PathBuf};
use vekro_hosting::{Account, Accounts, KeychainStore, Provider};

/// `accounts.json` in the data directory: where the accounts are listed. Their tokens are in the Keychain.
pub(crate) fn accounts_path() -> PathBuf {
  vekro_storage::data_dir().join("accounts.json")
}

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
pub(crate) fn credentials_for_url(accounts: &Accounts, url: &str) -> Option<vekro_git::Credentials> {
  if !url.starts_with("http") {
    return None;
  }
  let (connection, _) = accounts.for_remote(url, &KeychainStore)?;
  let (username, password) = connection.git_login();
  Some(vekro_git::Credentials { username, password })
}

/// The login Git gets for the remote of the repository at `root` (see [`credentials_for_url`]).
pub(crate) fn credentials_for_repository(accounts: &Accounts, root: &Path) -> Option<vekro_git::Credentials> {
  credentials_for_url(accounts, &vekro_git::remote_url(root)?)
}

/// The shared model of the settings, for the parts that only get an `App`.
fn model(cx: &App) -> Entity<SettingsModel> {
  cx.global::<SettingsGlobal>().0.clone()
}

/// The accounts in Settings: one row each with a button to disconnect it, and a button to connect another.
pub(crate) struct AccountsList {
  _observe: Subscription,
}

impl AccountsList {
  pub fn new(cx: &mut Context<Self>) -> Self {
    // The list changes when an account is connected or removed in any window.
    let observe = cx.observe(&model(cx), |_, _, cx| cx.notify());
    Self { _observe: observe }
  }
}

impl Render for AccountsList {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let accounts = model(cx).read(cx).accounts.items.clone();
    let rows = accounts.iter().map(|account| {
      let key = account.key();
      h_flex()
        .id(SharedString::from(format!("account-{key}")))
        .gap_3()
        .px_3()
        .py_2()
        .items_center()
        .rounded_md()
        .bg(palette.surface_alt)
        .child(Icon::empty().path(provider_icon(account.provider)).size_5().text_color(theme.muted_foreground))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .child(Label::new(account.provider.label()).font_weight(FontWeight::SEMIBOLD))
            .child(Label::new(account.label()).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground)),
        )
        .child(Button::new(SharedString::from(format!("account-remove-{key}"))).label("Disconnect").small().outline().on_click(move |_, _, cx| {
          let key = key.clone();
          model(cx).update(cx, |_, cx| cx.emit(AccountRemoveRequested(key)));
        }))
    });
    v_flex()
      .gap_2()
      .children(rows)
      .when(accounts.is_empty(), |list| {
        list.child(Label::new("No account is connected yet.").text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
      })
      .child(
        h_flex().child(
          Button::new("connect-account")
            .label("Connect an account…")
            .icon(Icon::empty().path("icons/plus.svg"))
            .small()
            .outline()
            .on_click(|_, _, cx| model(cx).update(cx, |_, cx| cx.emit(ConnectAccountRequested))),
        ),
      )
  }
}

/// How an account is named in a message.
pub(crate) fn describe(account: &Account) -> String {
  format!("{} ({})", account.provider.label(), account.label())
}
