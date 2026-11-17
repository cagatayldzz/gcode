//! The OpenCommit settings dialog: how commit messages are generated. The draft is applied when Save is pressed.

use crate::{
  components::display_path,
  theme::{colors, FONT_SIZE, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    label::Label,
    scroll::ScrollableElement as _,
    select::{Select, SelectState},
    switch::Switch,
    v_flex, ActiveTheme as _, IndexPath, WindowExt as _,
  },
  *,
};
use vekro_opencommit::{config_path, Config, DEFAULT_INSTRUCTIONS, DEFAULT_MAX_INPUT_TOKENS, LANGUAGES, MODELS, TOKEN_LIMITS};

/// The tallest the form gets before it scrolls, so the buttons stay in view.
const BODY_MAX_HEIGHT: Pixels = px(520.);

/// The settings the user saved: the OpenCommit file's values and the text of the commit instructions (`None` is the built-in one).
pub(crate) struct CommitSettingsSaved {
  pub config: Config,
  pub instructions: Option<String>,
}

type Choice = Entity<SelectState<Vec<SharedString>>>;

pub(crate) struct CommitSettings {
  config: Config,
  api_key: Entity<InputState>,
  api_url: Entity<InputState>,
  models: Vec<String>,
  model: Choice,
  tokens: Choice,
  language: Choice,
  instructions: Entity<TextareaState>,
}

impl EventEmitter<CommitSettingsSaved> for CommitSettings {}

fn token_label(tokens: u32) -> String {
  let mut digits = tokens.to_string();
  if digits.len() > 3 {
    digits.insert(digits.len() - 3, ',');
  }
  match tokens == DEFAULT_MAX_INPUT_TOKENS {
    true => format!("{digits} tokens (Recommended)"),
    false => format!("{digits} tokens"),
  }
}

fn choice(items: Vec<String>, selected: usize, window: &mut Window, cx: &mut Context<CommitSettings>) -> Choice {
  let items: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
  cx.new(|cx| SelectState::new(items, Some(IndexPath::default().row(selected)), window, cx))
}

impl CommitSettings {
  pub fn new(config: Config, instructions: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let mut models: Vec<String> = MODELS.iter().map(|model| model.to_string()).collect();
    if !models.contains(&config.model) {
      models.insert(0, config.model.clone());
    }
    let mut limits: Vec<u32> = TOKEN_LIMITS.to_vec();
    if !limits.contains(&config.max_input_tokens) {
      limits.push(config.max_input_tokens);
      limits.sort_unstable();
    }
    let mut languages: Vec<(String, String)> = LANGUAGES.iter().map(|(code, name)| (code.to_string(), format!("{name} ({code})"))).collect();
    if !languages.iter().any(|(code, _)| *code == config.language) {
      languages.insert(0, (config.language.clone(), config.language.clone()));
    }
    let model_index = models.iter().position(|model| *model == config.model).unwrap_or(0);
    let token_index = limits.iter().position(|limit| *limit == config.max_input_tokens).unwrap_or(0);
    let language_index = languages.iter().position(|(code, _)| *code == config.language).unwrap_or(0);
    let model = choice(models.clone(), model_index, window, cx);
    let tokens = choice(limits.iter().map(|limit| token_label(*limit)).collect(), token_index, window, cx);
    let language = choice(languages.into_iter().map(|(_, label)| label).collect(), language_index, window, cx);
    let api_key = {
      let value = config.api_key.clone();
      cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("sk-…").default_value(value))
    };
    let api_url = {
      let value = config.api_url.clone();
      cx.new(|cx| InputState::new(window, cx).placeholder("https://api.openai.com/v1").default_value(value))
    };
    let text = instructions.unwrap_or_else(|| DEFAULT_INSTRUCTIONS.to_string());
    let instructions = cx.new(|cx| TextareaState::new(window, cx).placeholder("Optional instructions for the model").default_value(text));
    Self { config, api_key, api_url, models, model, tokens, language, instructions }
  }

  fn save(&mut self, cx: &mut Context<Self>) {
    let mut config = self.config.clone();
    if let Some(model) = self.model.read(cx).selected_index(cx).and_then(|index| self.models.get(index.row)) {
      config.model = model.clone();
    }
    let mut limits: Vec<u32> = TOKEN_LIMITS.to_vec();
    if !limits.contains(&config.max_input_tokens) {
      limits.push(config.max_input_tokens);
      limits.sort_unstable();
    }
    if let Some(limit) = self.tokens.read(cx).selected_index(cx).and_then(|index| limits.get(index.row)) {
      config.max_input_tokens = *limit;
    }
    let mut codes: Vec<String> = LANGUAGES.iter().map(|(code, _)| code.to_string()).collect();
    if !codes.contains(&config.language) {
      codes.insert(0, config.language.clone());
    }
    if let Some(code) = self.language.read(cx).selected_index(cx).and_then(|index| codes.get(index.row)) {
      config.language = code.clone();
    }
    config.api_key = self.api_key.read(cx).value().trim().to_string();
    config.api_url = self.api_url.read(cx).value().trim().to_string();
    let text = self.instructions.read(cx).value().trim().to_string();
    let instructions = (text != DEFAULT_INSTRUCTIONS).then_some(text);
    cx.emit(CommitSettingsSaved { config, instructions });
  }

  /// A titled group of rows with a line between them.
  fn section(title: &'static str, rows: Vec<AnyElement>, help: Option<SharedString>, cx: &App) -> impl IntoElement {
    let palette = colors(cx);
    let theme = cx.theme();
    v_flex()
      .gap_1p5()
      .child(Label::new(title).font_weight(FontWeight::SEMIBOLD))
      .child(
        v_flex().rounded_lg().border_1().border_color(palette.border).children(
          rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| div().px_3p5().py_2p5().when(index > 0, |row| row.border_t_1().border_color(palette.border)).child(row)),
        ),
      )
      .when_some(help, |section, help| section.child(Label::new(help).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground)))
  }

  fn row(label: &'static str, control: impl IntoElement) -> AnyElement {
    h_flex().gap_3().items_center().justify_between().child(Label::new(label)).child(control).into_any_element()
  }

  fn toggle(&self, id: &'static str, label: &'static str, checked: bool, change: fn(&mut Config, bool), cx: &mut Context<Self>) -> AnyElement {
    Self::row(
      label,
      Switch::new(id).checked(checked).on_change(cx.listener(move |this, checked: &bool, _, cx| {
        change(&mut this.config, *checked);
        cx.notify();
      })),
    )
  }
}

impl Render for CommitSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let generation = Self::section(
      "Generation",
      vec![
        Self::row("API key", div().w_64().child(Input::new(&self.api_key).mask_toggle())),
        Self::row("API URL", div().w_64().child(Input::new(&self.api_url))),
        Self::row("Model", div().w_64().child(Select::new(&self.model).menu_width(px(256.)))),
        Self::row("Largest diff", div().w_64().child(Select::new(&self.tokens).menu_width(px(256.)))),
        Self::row("Language", div().w_64().child(Select::new(&self.language).menu_width(px(256.)))),
        Self::row("Commit format", Label::new("Conventional Commits").text_color(muted)),
      ],
      Some("A larger diff is split per file and the parts are summarized, then merged into one message. A higher limit sends more per request at a higher token cost. Leave the API URL empty for OpenAI; any OpenAI-compatible service works.".into()),
      cx,
    );
    let message = Self::section(
      "Message",
      vec![
        self.toggle("one-line", "One-line commit", self.config.one_line, |config, value| config.one_line = value, cx),
        self.toggle("description", "Add a description", self.config.description, |config, value| config.description = value, cx),
        self.toggle("why", "Explain why the change was made", self.config.why, |config, value| config.why = value, cx),
        self.toggle("emoji", "Use GitMoji", self.config.emoji, |config, value| config.emoji = value, cx),
        self.toggle("omit-scope", "Omit the scope", self.config.omit_scope, |config, value| config.omit_scope = value, cx),
      ],
      None,
      cx,
    );
    let instructions = v_flex()
      .gap_1p5()
      .child(Label::new("Commit instructions").font_weight(FontWeight::SEMIBOLD))
      .child(Textarea::new(&self.instructions).h_20())
      .child(
        Label::new("Sent to the model with every generated message, on top of the rules above. Leave empty for none.")
          .text_size(FONT_SIZE_SM)
          .text_color(muted),
      );
    let git = Self::section(
      "Git",
      vec![self.toggle("git-push", "Push after commit", self.config.git_push, |config, value| config.git_push = value, cx)],
      Some(
        format!(
          "Saved to {}, shared with the oco command. Other settings in that file are kept. Without a push target, the branch is published to origin.",
          display_path(&config_path())
        )
        .into(),
      ),
      cx,
    );
    v_flex()
      .text_size(FONT_SIZE)
      .gap_4()
      .child(
        v_flex()
          .id("commit-settings-body")
          .max_h(BODY_MAX_HEIGHT)
          .gap_4()
          .pr_3()
          .overflow_y_scrollbar()
          .child(generation)
          .child(message)
          .child(instructions)
          .child(git),
      )
      .child(
        h_flex()
          .gap_2()
          .justify_between()
          .child(Button::new("commit-settings-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
          .child(Button::new("commit-settings-save").label("Save").primary().on_click(cx.listener(|this, _, _, cx| this.save(cx)))),
      )
  }
}
