//! The dialog that connects an account: the service, where it lives and a personal access token. The token is checked with the
//! service first and kept in the macOS Keychain; only the account (service, host, user name) goes to `accounts.json`.

use crate::{
  components::selected_style,
  theme::{FONT_SIZE, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
  },
  *,
};
use vekro_hosting::{Account, Connection, KeychainStore, Provider, TokenStore as _};

/// Sent when the account was verified and its token is in the Keychain, with what is wrong or unknown about the token's permissions.
pub(crate) struct AccountConnected(pub Account, pub Vec<String>, pub Option<String>);

impl EventEmitter<AccountConnected> for ConnectAccount {}

pub(crate) struct ConnectAccount {
  provider: Provider,
  host: Entity<InputState>,
  organization: Entity<InputState>,
  token: Entity<InputState>,
  busy: bool,
  error: Option<SharedString>,
  _subscription: Subscription,
}

impl ConnectAccount {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let host = cx.new(|cx| InputState::new(window, cx).placeholder(Provider::GitHub.default_host()));
    let organization = cx.new(|cx| InputState::new(window, cx).placeholder("your-organization"));
    let token = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Paste the token"));
    let subscription = cx.subscribe_in(&token, window, |this, _, event: &InputEvent, _, cx| {
      if let InputEvent::PressEnter { .. } = event {
        this.connect(cx);
      }
    });
    Self { provider: Provider::GitHub, host, organization, token, busy: false, error: None, _subscription: subscription }
  }

  fn choose(&mut self, provider: Provider, window: &mut Window, cx: &mut Context<Self>) {
    if self.busy || self.provider == provider {
      return;
    }
    self.provider = provider;
    self.error = None;
    self.host.update(cx, |input, cx| {
      input.set_value("", window, cx);
      input.set_placeholder(provider.default_host(), window, cx);
    });
    cx.notify();
  }

  fn connect(&mut self, cx: &mut Context<Self>) {
    if self.busy {
      return;
    }
    let provider = self.provider;
    let host = self.host.read(cx).value().trim().to_string();
    let organization = self.organization.read(cx).value().trim().to_string();
    let token = self.token.read(cx).value().trim().to_string();
    if token.is_empty() {
      self.error = Some("Paste a personal access token.".into());
      cx.notify();
      return;
    }
    if provider == Provider::AzureDevOps && organization.is_empty() {
      self.error = Some("Enter the Azure DevOps organization.".into());
      cx.notify();
      return;
    }
    let account = Account {
      provider,
      host: if host.is_empty() || provider == Provider::AzureDevOps { provider.default_host().to_string() } else { host },
      organization: (provider == Provider::AzureDevOps).then_some(organization),
      username: String::new(),
      issue: None,
    };
    self.busy = true;
    self.error = None;
    cx.notify();
    let work = cx.background_executor().spawn(async move {
      let username = vekro_hosting::verify(&Connection { account: account.clone(), token: token.clone() })?;
      let verified = Account { username, ..account };
      KeychainStore.set(&verified.key(), &token)?;
      let access = vekro_hosting::check_access(&Connection { account: verified.clone(), token });
      Ok::<_, String>((verified, access.warnings(provider), access.unchecked(provider)))
    });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.busy = false;
        match result {
          Ok((account, warnings, unchecked)) => cx.emit(AccountConnected(account, warnings, unchecked)),
          Err(message) => this.error = Some(message.into()),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// How to create the token: where, which permissions, and what else matters.
  fn guide(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let guide = self.provider.token_guide();
    v_flex()
      .gap_1p5()
      .p_3()
      .rounded_md()
      .border_1()
      .border_color(theme.border)
      .child(Label::new("Create a token with these permissions").font_weight(FontWeight::SEMIBOLD))
      .child(Label::new(guide.place).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
      .children(guide.permissions.iter().map(|(name, why)| {
        h_flex().gap_2().items_start().child(Icon::empty().path("icons/check.svg").size_4().flex_none().mt_0p5().text_color(theme.success)).child(
          v_flex()
            .child(Label::new(*name).font_weight(FontWeight::MEDIUM))
            .child(Label::new(*why).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground)),
        )
      }))
      .child(Label::new(guide.note).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
  }

  fn provider_button(&self, provider: Provider, cx: &mut Context<Self>) -> impl IntoElement {
    let chosen = self.provider == provider;
    Button::new(provider.id())
      .small()
      .when(chosen, |button| button.custom(selected_style(cx)))
      .when(!chosen, |button| button.outline())
      .label(provider.label())
      .on_click(cx.listener(move |this, _, window, cx| this.choose(provider, window, cx)))
  }
}

impl Render for ConnectAccount {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let azure = self.provider == Provider::AzureDevOps;
    let field = |caption: &'static str, content: AnyElement| {
      v_flex().gap_1p5().child(Label::new(caption).text_color(muted).text_size(FONT_SIZE_SM)).child(content)
    };
    v_flex()
      .text_size(FONT_SIZE)
      .gap_4()
      .child(h_flex().gap_2().children(Provider::ALL.map(|provider| self.provider_button(provider, cx))))
      .child(self.guide(cx))
      .when(azure, |form| form.child(field("Organization", Input::new(&self.organization).into_any_element())))
      .when(!azure, |form| {
        form.child(field("Host (leave empty for the service itself, or enter your own server)", Input::new(&self.host).into_any_element()))
      })
      .child(field("Personal access token", Input::new(&self.token).mask_toggle().into_any_element()))
      .child(
        Label::new("The token is checked with the service and kept in the macOS Keychain. Git uses it over HTTPS for this host.")
          .text_size(FONT_SIZE_SM)
          .text_color(muted),
      )
      .when_some(self.error.clone(), |form, error| form.child(Label::new(error).text_color(cx.theme().danger)))
      .child(
        h_flex().gap_2().justify_end().child(Button::new("connect-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx))).child(
          Button::new("connect-submit")
            .label(if self.busy { "Checking…" } else { "Connect" })
            .primary()
            .loading(self.busy)
            .disabled(self.busy)
            .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
        ),
      )
  }
}
