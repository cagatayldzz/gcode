//! The dialog that opens a pull request (a merge request on GitLab) for the current branch.

use crate::theme::{FONT_SIZE, FONT_SIZE_SM};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputState, TextareaState},
    label::Label,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, WindowExt as _,
  },
  *,
};
use ws_hosting::{Accounts, NewPullRequest, PullRequest};

/// What the form tells the window: the pull request was opened, or the service refused the token.
pub(crate) enum PullRequestEvent {
  Opened(PullRequest),
  /// The token is not allowed to open pull requests (or was rejected): the account has a problem to show.
  Refused,
}

impl EventEmitter<PullRequestEvent> for PullRequestForm {}

pub(crate) struct PullRequestForm {
  remote_url: String,
  accounts: Accounts,
  source: String,
  /// The branch has commits the remote does not have yet: the pull request would not show them.
  unpushed: usize,
  published: bool,
  title: Entity<InputState>,
  description: Entity<TextareaState>,
  target: Entity<InputState>,
  draft: bool,
  busy: bool,
  error: Option<SharedString>,
}

/// What the form needs to know about the repository and the branch.
pub(crate) struct PullRequestTarget {
  pub remote_url: String,
  pub source: String,
  pub unpushed: usize,
  pub published: bool,
  pub suggested_title: String,
}

impl PullRequestForm {
  /// The address of the remote the pull request is for.
  pub fn remote_url(&self) -> &str {
    &self.remote_url
  }

  pub fn new(accounts: Accounts, target: PullRequestTarget, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let title = cx.new(|cx| InputState::new(window, cx).placeholder("Title (required)").default_value(target.suggested_title.clone()));
    let description = cx.new(|cx| TextareaState::new(window, cx).placeholder("Description"));
    let base = cx.new(|cx| InputState::new(window, cx).placeholder("Loading the default branch…"));
    let this = Self {
      remote_url: target.remote_url.clone(),
      accounts: accounts.clone(),
      source: target.source,
      unpushed: target.unpushed,
      published: target.published,
      title,
      description,
      target: base,
      draft: false,
      busy: false,
      error: None,
    };
    // The branch the repository's pull requests go into by default fills the target field.
    let url = target.remote_url;
    let work = cx.background_executor().spawn(async move {
      let (connection, repository) = accounts.for_remote(&url, &accounts.keychain()).ok_or("No connected account serves this repository.")?;
      ws_hosting::default_branch(&connection, &repository)
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        if let Ok(branch) = result {
          if this.target.read(cx).value().is_empty() {
            this.target.update(cx, |input, cx| input.set_value(branch, window, cx));
          }
        }
        this.target.update(cx, |input, cx| input.set_placeholder("main", window, cx));
      });
    })
    .detach();
    this
  }

  fn create(&mut self, cx: &mut Context<Self>) {
    if self.busy {
      return;
    }
    let title = self.title.read(cx).value().trim().to_string();
    let target = self.target.read(cx).value().trim().to_string();
    if title.is_empty() || target.is_empty() {
      self.error = Some("Enter a title and the branch the changes should go into.".into());
      cx.notify();
      return;
    }
    if target == self.source {
      self.error = Some("The branch cannot go into itself. Choose another target branch.".into());
      cx.notify();
      return;
    }
    let new =
      NewPullRequest { title, description: self.description.read(cx).value().to_string(), source: self.source.clone(), target, draft: self.draft };
    let (accounts, url) = (self.accounts.clone(), self.remote_url.clone());
    self.busy = true;
    self.error = None;
    cx.notify();
    let work = cx.background_executor().spawn(async move {
      let (connection, repository) = accounts
        .for_remote(&url, &accounts.keychain())
        .ok_or("The account for this repository is not connected any more. Connect it again in Settings → Accounts.")?;
      ws_hosting::create_pull_request(&connection, &repository, &new)
    });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.busy = false;
        match result {
          Ok(request) => cx.emit(PullRequestEvent::Opened(request)),
          Err(message) => {
            if ws_hosting::is_permission_problem(&message) {
              cx.emit(PullRequestEvent::Refused);
            }
            this.error = Some(message.into());
          }
        }
        cx.notify();
      });
    })
    .detach();
  }
}

impl Render for PullRequestForm {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let warning = if !self.published {
      Some("This branch is not on the remote yet. Publish it first (the button in the Git toolbar), then open the pull request.".to_string())
    } else if self.unpushed > 0 {
      Some(format!("{} commit(s) of this branch are not pushed yet, so the pull request will not show them.", self.unpushed))
    } else {
      None
    };
    let field = |caption: &'static str, content: AnyElement| {
      v_flex().gap_1p5().child(Label::new(caption).text_color(muted).text_size(FONT_SIZE_SM)).child(content)
    };
    v_flex()
      .text_size(FONT_SIZE)
      .gap_4()
      .child(Label::new(format!("From {} into the branch below.", self.source)).text_color(muted))
      .when_some(warning, |form, warning| form.child(Label::new(warning).text_size(FONT_SIZE_SM).text_color(cx.theme().warning)))
      .child(field("Title", crate::components::soft_input(&self.title, cx).into_any_element()))
      .child(field("Description", crate::components::soft_textarea(&self.description, cx).h(px(110.)).into_any_element()))
      .child(field("Target branch", crate::components::soft_input(&self.target, cx).into_any_element()))
      .child(h_flex().gap_2().items_center().child(Label::new("Open as a draft").flex_1()).child(
        Switch::new("pull-request-draft").checked(self.draft).on_change(cx.listener(|this, checked: &bool, _, cx| {
          this.draft = *checked;
          cx.notify();
        })),
      ))
      .when_some(self.error.clone(), |form, error| form.child(Label::new(error).text_color(cx.theme().danger)))
      .child(
        h_flex()
          .gap_2()
          .justify_end()
          .child(Button::new("pull-request-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
          .child(
            Button::new("pull-request-create")
              .label(if self.busy { "Opening…" } else { "Create pull request" })
              .primary()
              .loading(self.busy)
              .disabled(self.busy || !self.published)
              .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
          ),
      )
  }
}
