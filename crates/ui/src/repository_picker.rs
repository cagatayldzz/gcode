//! The list of repositories of a connected account, to pick one to clone.

use crate::{
  components::selected_style,
  connections::provider_icon,
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    scroll::ScrollableElement as _,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
  },
  *,
};
use vekro_hosting::{Account, KeychainStore, Repository, TokenStore as _};

/// The tallest the list gets before it scrolls.
const LIST_MAX_HEIGHT: Pixels = px(220.);

/// Sent with the HTTPS address of the repository the user chose.
pub(crate) struct RepositoryChosen(pub String);

impl EventEmitter<RepositoryChosen> for RepositoryPicker {}

enum Listing {
  Loading,
  Loaded(Vec<Repository>),
  Failed(SharedString),
}

pub(crate) struct RepositoryPicker {
  accounts: Vec<Account>,
  selected: usize,
  listing: Listing,
  filter: Entity<InputState>,
  /// Which load the shown list belongs to, so that a slow answer for another account is ignored.
  epoch: u64,
  _subscription: Subscription,
}

impl RepositoryPicker {
  pub fn new(accounts: Vec<Account>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter repositories"));
    let subscription = cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify());
    let mut this = Self { accounts, selected: 0, listing: Listing::Loading, filter, epoch: 0, _subscription: subscription };
    this.load(cx);
    this
  }

  fn load(&mut self, cx: &mut Context<Self>) {
    let Some(account) = self.accounts.get(self.selected).cloned() else { return };
    self.listing = Listing::Loading;
    self.epoch += 1;
    let epoch = self.epoch;
    cx.notify();
    let work = cx.background_executor().spawn(async move {
      let token = KeychainStore
        .get(&account.key())
        .ok_or_else(|| "The token is not in the Keychain any more. Disconnect the account in Settings and connect it again.".to_string())?;
      vekro_hosting::repositories(&vekro_hosting::Connection { account, token })
    });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch == epoch {
          this.listing = match result {
            Ok(found) => Listing::Loaded(found),
            Err(message) => Listing::Failed(message.into()),
          };
          cx.notify();
        }
      });
    })
    .detach();
  }
}

impl Render for RepositoryPicker {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let accounts = self.accounts.iter().enumerate().map(|(index, account)| {
      let chosen = index == self.selected;
      Button::new(SharedString::from(format!("picker-account-{}", account.key())))
        .small()
        .when(chosen, |button| button.custom(selected_style(cx)))
        .when(!chosen, |button| button.outline())
        .icon(Icon::empty().path(provider_icon(account.provider)))
        .label(account.label())
        .on_click(cx.listener(move |this, _, _, cx| {
          if this.selected != index {
            this.selected = index;
            this.load(cx);
          }
        }))
    });
    let query = self.filter.read(cx).value().to_lowercase();
    let body = match &self.listing {
      Listing::Loading => h_flex()
        .gap_2()
        .p_2()
        .items_center()
        .child(Spinner::new().small())
        .child(Label::new("Loading repositories…").text_color(theme.muted_foreground))
        .into_any_element(),
      Listing::Failed(message) => Label::new(message.clone()).text_color(theme.danger).p_2().into_any_element(),
      Listing::Loaded(found) => {
        let rows: Vec<AnyElement> = found
          .iter()
          .filter(|repository| query.is_empty() || repository.name.to_lowercase().contains(&query))
          .map(|repository| {
            let url = repository.clone_url.clone();
            h_flex()
              .id(SharedString::from(format!("repository-{}", repository.clone_url)))
              .gap_2()
              .px_2()
              .py_1()
              .rounded_md()
              .items_center()
              .cursor_pointer()
              .hover(|row| row.bg(palette.hover))
              .child(
                Icon::empty()
                  .path(if repository.private { "icons/lock.svg" } else { "icons/globe.svg" })
                  .size_4()
                  .flex_none()
                  .text_color(theme.muted_foreground),
              )
              .child(
                v_flex().flex_1().min_w_0().child(Label::new(repository.name.clone()).overflow_hidden().whitespace_nowrap().text_ellipsis()).when(
                  !repository.description.is_empty(),
                  |column| {
                    column.child(
                      Label::new(repository.description.clone())
                        .text_size(FONT_SIZE_SM)
                        .text_color(theme.muted_foreground)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis(),
                    )
                  },
                ),
              )
              .on_click(cx.listener(move |_, _, _, cx| cx.emit(RepositoryChosen(url.clone()))))
              .into_any_element()
          })
          .collect();
        if rows.is_empty() {
          Label::new(if found.is_empty() { "This account has no repositories." } else { "No repository matches." })
            .text_color(theme.muted_foreground)
            .p_2()
            .into_any_element()
        } else {
          v_flex().id("repository-list").max_h(LIST_MAX_HEIGHT).overflow_y_scrollbar().gap_0p5().children(rows).into_any_element()
        }
      }
    };
    v_flex()
      .gap_2()
      .p_2()
      .rounded_md()
      .border_1()
      .border_color(theme.input)
      .when(self.accounts.len() > 1, |column| column.child(h_flex().gap_2().flex_wrap().children(accounts)))
      .child(Input::new(&self.filter))
      .child(body)
  }
}
