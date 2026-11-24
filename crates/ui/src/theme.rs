//! The app's light and dark colors and fonts, switched with the system appearance.
//! The palettes follow the GitHub Default editor themes (https://github.com/primer/github-vscode-theme).

use gpui_kit::{
  component::{
    highlighter::{SyntaxColors, ThemeStyle},
    ActiveTheme as _, Theme, ThemeMode,
  },
  *,
};
use ws_storage::{FontChoice, ThemeChoice};

/// The size of the text the interface draws itself. The kit's own parts (icons, dock tabs) are sized in `rem`,
/// which stays at the theme's 16 px.
pub(crate) const FONT_SIZE: Pixels = px(14.);

/// Secondary text, such as a changed file's folder.
pub(crate) const FONT_SIZE_SM: Pixels = px(12.);

/// The colors of one appearance.
#[derive(Clone, Copy)]
pub(crate) struct Colors {
  /// The main area: editor, terminal and the window.
  pub background: Hsla,
  pub text: Hsla,
  pub text_secondary: Hsla,
  /// The sidebar and the title band.
  pub surface: Hsla,
  /// A slightly raised surface: the status bar, empty-state icons.
  pub surface_alt: Hsla,
  pub border: Hsla,
  /// The outline of a field or a checkbox: stronger than `border`, which only separates areas.
  pub control_border: Hsla,
  /// The row that is chosen.
  pub selected: Hsla,
  pub explorer_selected: Hsla,
  pub explorer_selected_text: Hsla,
  pub plist_icon: Hsla,
  pub hover: Hsla,
  /// The window title band.
  pub rail: Hsla,
  pub rail_hover: Hsla,
  pub project_tab_selected: Hsla,
  /// The hover color of an item on a `rail_hover` background, such as the inside of a category.
  pub rail_item_hover: Hsla,
  pub rail_text: Hsla,
  /// The accent for selection controls and badges.
  pub accent: Hsla,
  pub tab_indicator: Hsla,
  pub accent_hover: Hsla,
  /// Text and icons drawn on the accent color.
  pub on_accent: Hsla,
  pub primary_button: Hsla,
  pub primary_button_hover: Hsla,
  pub primary_button_active: Hsla,
  pub link: Hsla,
  pub link_hover: Hsla,
  /// Marks for new, deleted, modified, renamed and conflicted files.
  pub new: Hsla,
  pub deleted: Hsla,
  pub modified: Hsla,
  pub renamed: Hsla,
  pub conflicted: Hsla,
  /// The whole-line backgrounds of added and removed lines in a diff.
  pub diff_added: Hsla,
  pub diff_removed: Hsla,
}

/// The line between the rows of a list: the theme's border, half as strong, so that it separates without drawing the eye. It follows
/// the palette, so it is a light line on the dark theme and a dark one on the light theme.
pub(crate) fn row_divider(palette: &Colors) -> Hsla {
  palette.border.opacity(0.5)
}

fn hex(value: u32) -> Hsla {
  rgb(value).into()
}

/// The colors a project's avatar can take, as `(name, #rrggbb)`.
pub(crate) const PROJECT_COLORS: [(&str, &str); 8] = [
  ("Blue", "#4d8dff"),
  ("Purple", "#8b7bff"),
  ("Pink", "#f066b0"),
  ("Red", "#ee5a5a"),
  ("Orange", "#f0883e"),
  ("Yellow", "#d9a62e"),
  ("Green", "#3fb68b"),
  ("Teal", "#2fb5c9"),
];

/// A stored `#rrggbb` color.
pub(crate) fn project_color(value: &str) -> Option<Hsla> {
  let digits = value.strip_prefix('#')?;
  (digits.len() == 6).then(|| u32::from_str_radix(digits, 16).ok()).flatten().map(hex)
}

impl Colors {
  fn dark() -> Self {
    Self {
      background: hex(0x0d1117),
      text: hex(0xe6edf3),
      text_secondary: hex(0x7d8590),
      surface: hex(0x010409),
      surface_alt: hex(0x151b23),
      border: hex(0x30363d),
      control_border: hex(0x30363d),
      selected: hex(0x172b45),
      explorer_selected: hex(0x6e7681).opacity(0.4),
      explorer_selected_text: hex(0xe6edf3),
      plist_icon: hex(0xf0883e),
      hover: hex(0x6e7681).opacity(0.1),
      rail: hex(0x0d1117),
      rail_hover: hex(0x151b23),
      project_tab_selected: hex(0x2f353d),
      rail_item_hover: hex(0x212830),
      rail_text: hex(0x7d8590),
      accent: hex(0x1f6feb),
      tab_indicator: hex(0xf78166),
      accent_hover: hex(0x1c62d2),
      on_accent: hex(0xffffff),
      primary_button: hex(0x238636),
      primary_button_hover: hex(0x2ea043),
      primary_button_active: hex(0x216e39),
      link: hex(0x2f81f7),
      link_hover: hex(0x79b8ff),
      new: hex(0x3fb950),
      deleted: hex(0xf85149),
      modified: hex(0xd29922),
      renamed: hex(0x4493f8),
      conflicted: hex(0xdb6d28),
      diff_added: hex(0x238636).opacity(0.15),
      diff_removed: hex(0xda3633).opacity(0.15),
    }
  }

  fn light() -> Self {
    let text = hex(0x1f2328);
    let rail_hover = hex(0xeaeef2);
    Self {
      background: hex(0xffffff),
      text,
      text_secondary: hex(0x656d76),
      surface: hex(0xf6f8fa),
      surface_alt: hex(0xf6f8fa),
      border: hex(0xd0d7de),
      control_border: hex(0xd0d7de),
      selected: hex(0xddf4ff),
      explorer_selected: hex(0xafb8c1).opacity(0.2),
      explorer_selected_text: text,
      plist_icon: hex(0xbc4c00),
      hover: hex(0xeaeef2).opacity(0.5),
      rail: hex(0xffffff),
      rail_hover,
      project_tab_selected: hex(0xdce2e8),
      rail_item_hover: hex(0xd1d9e0),
      rail_text: hex(0x656d76),
      accent: hex(0x0969da),
      tab_indicator: hex(0xfd8c73),
      accent_hover: hex(0x0759bd),
      on_accent: hex(0xffffff),
      primary_button: hex(0x1f883d),
      primary_button_hover: hex(0x1a7f37),
      primary_button_active: hex(0x116329),
      link: hex(0x0969da),
      link_hover: hex(0x0550ae),
      new: hex(0x1a7f37),
      deleted: hex(0xd1242f),
      modified: hex(0x9a6700),
      renamed: hex(0x0969da),
      conflicted: hex(0xbc4c00),
      diff_added: hex(0xaceebb).opacity(0.3),
      diff_removed: hex(0xffcecb).opacity(0.3),
    }
  }
}

/// The colors for the appearance that is showing.
pub(crate) fn colors(cx: &App) -> Colors {
  if cx.theme().is_dark() {
    Colors::dark()
  } else {
    Colors::light()
  }
}

fn syntax_color(color: Hsla) -> Option<ThemeStyle> {
  syntax_json(serde_json::json!({ "color": color }))
}

fn syntax_json(value: serde_json::Value) -> Option<ThemeStyle> {
  serde_json::from_value(value).ok()
}

/// Shared syntax roles used by the editor and minimap across languages.
fn github_syntax(syntax: &mut SyntaxColors, c: &Colors, dark: bool) {
  let text = syntax_color(c.text);
  let blue = syntax_color(hex(if dark { 0x79c0ff } else { 0x0550ae }));
  let string = syntax_color(hex(if dark { 0xa5d6ff } else { 0x0a3069 }));
  syntax.property = blue;
  syntax.attribute = blue;
  syntax.operator = blue;
  syntax.text_literal = blue;
  syntax.text_code_span = blue;
  syntax.link_uri = string;
  syntax.link_text = text;
  syntax.string_escape = string;
  syntax.string_regex = string;
  syntax.string_special = string;
  syntax.string_special_symbol = string;
  syntax.preproc = syntax.keyword;
  // Markdown: headings and rules bold in the heading blue, list markers orange, the cells' pipes gold, emphasis in the text color.
  let heading = hex(if dark { 0x1f6feb } else { 0x0550ae });
  syntax.title = syntax_json(serde_json::json!({ "color": heading, "font_weight": 700 }));
  syntax.punctuation_list_marker = syntax.constructor;
  syntax.variant = syntax_color(hex(if dark { 0xf2cc60 } else { 0x9a6700 }));
  syntax.emphasis = syntax_json(serde_json::json!({ "color": c.text, "font_style": "italic" }));
  syntax.emphasis_strong = syntax_json(serde_json::json!({ "color": c.text, "font_weight": 700 }));
  for role in [
    &mut syntax.type_,
    &mut syntax.variable,
    &mut syntax.variable_special,
    &mut syntax.embedded,
    &mut syntax.enum_,
    &mut syntax.primary,
    &mut syntax.punctuation,
    &mut syntax.punctuation_bracket,
    &mut syntax.punctuation_delimiter,
  ] {
    *role = text;
  }
}

/// Applies the chosen appearance (the system's, unless the user picked one) with the app's colors and fonts.
pub(crate) fn apply_theme(choice: ThemeChoice, font: FontChoice, code_font_size: u32, window: &mut Window, cx: &mut App) {
  match choice {
    ThemeChoice::System => Theme::sync_system_appearance(Some(window), cx),
    ThemeChoice::Light => Theme::change(ThemeMode::Light, Some(window), cx),
    ThemeChoice::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
  }
  let c = colors(cx);
  let dark = cx.theme().is_dark();
  Theme::update(cx, |theme| {
    theme.font_family = font.sans_family().into();
    theme.mono_font_family = font.mono_family().into();
    theme.mono_font_size = px(code_font_size as f32);
    // Dialogs and notifications are rounded with `radius_lg`; fields, buttons and menu items with `radius`.
    theme.radius = px(6.);
    theme.radius_lg = px(10.);
    // A focused control only tints its border; the Kit's extra ring around it is off.
    theme.focus_ring = false;
    theme.notification.width = px(360.);
    let style = &mut std::sync::Arc::make_mut(&mut theme.highlight_theme).style;
    style.editor_background = Some(c.background);
    style.editor_gutter_background = Some(c.background);
    style.editor_active_line = Some(c.hover);
    style.editor_line_number = Some(hex(if dark { 0x6e7681 } else { 0x8c959f }));
    style.editor_active_line_number = Some(c.text);
    let (label, string, literal, keyword, type_name, function, tag) = if dark {
      (hex(0x7ee787), hex(0xa5d6ff), hex(0x79c0ff), hex(0xff7b72), hex(0xffa657), hex(0xd2a8ff), hex(0x7ee787))
    } else {
      (hex(0x0550ae), hex(0x0a3069), hex(0x0550ae), hex(0xcf222e), hex(0x953800), hex(0x8250df), hex(0x116329))
    };
    let syntax = &mut style.syntax;
    // JSON and YAML keys have their own syntax role.
    syntax.label = syntax_color(label);
    // The name of a class that is built or listed, and the names of the line-based files (see `minimap::outline`).
    syntax.constructor = syntax_color(type_name);
    syntax.comment = syntax_color(hex(if dark { 0x8b949e } else { 0x6e7781 }));
    syntax.comment_doc = syntax.comment;
    syntax.string = syntax_color(string);
    syntax.keyword = syntax_color(keyword);
    syntax.function = syntax_color(function);
    syntax.tag = syntax_color(tag);
    let literal = syntax_color(literal);
    syntax.boolean = literal;
    syntax.constant = literal;
    syntax.number = literal;
    github_syntax(syntax, &c, dark);

    let colors = &mut theme.colors;
    colors.background = c.background;
    colors.foreground = c.text;
    colors.sidebar = c.surface;
    colors.sidebar_foreground = c.text;
    colors.sidebar_border = c.border;
    colors.sidebar_accent = c.selected;
    colors.sidebar_accent_foreground = c.text;
    colors.sidebar_primary = c.accent;
    colors.sidebar_primary_foreground = c.on_accent;
    colors.border = c.border;
    colors.input = c.control_border;
    colors.title_bar = c.rail;
    colors.title_bar_border = c.border;
    colors.status_bar = c.rail;
    colors.status_bar_border = c.border;
    colors.group_box = c.surface;
    colors.group_box_foreground = c.text;
    colors.tab = c.surface;
    colors.tab_bar = c.surface;
    colors.tab_bar_segmented = c.surface_alt;
    colors.tab_foreground = c.text_secondary;
    colors.tab_active_foreground = c.text;
    // A switch that is off is a gray track.
    colors.switch = c.rail_item_hover;
    colors.tab_active = c.background;
    colors.popover = c.surface;
    colors.popover_foreground = c.text;
    colors.muted = c.surface_alt;
    colors.muted_foreground = c.text_secondary;
    colors.secondary = c.surface_alt;
    colors.secondary_foreground = c.text;
    colors.accent = c.rail_hover;
    colors.accent_foreground = c.text;
    colors.primary = c.accent;
    colors.primary_hover = c.accent_hover;
    colors.primary_active = c.accent;
    colors.primary_foreground = c.on_accent;
    colors.button_primary = c.primary_button;
    colors.button_primary_hover = c.primary_button_hover;
    colors.button_primary_active = c.primary_button_active;
    colors.button_primary_foreground = c.on_accent;
    colors.button_secondary = c.surface_alt;
    colors.button_secondary_hover = c.rail_item_hover;
    colors.button_secondary_active = c.selected;
    colors.button_secondary_foreground = c.text;
    colors.ring = c.link;
    colors.caret = c.text;
    colors.link = c.link;
    colors.link_hover = c.link_hover;
    colors.link_active = c.link_hover;
    colors.list = c.surface;
    colors.list_even = c.surface;
    colors.list_hover = c.hover;
    colors.list_active = c.selected;
    colors.list_head = c.surface_alt;
    colors.list_active_border = c.link;
    colors.table = c.background;
    colors.table_even = c.surface;
    colors.table_head = c.surface_alt;
    colors.table_head_foreground = c.text_secondary;
    colors.table_hover = c.hover;
    colors.table_active = c.selected;
    colors.table_active_border = c.link;
    colors.table_row_border = c.border;
    colors.scrollbar_thumb = c.text_secondary.opacity(0.6);
    colors.scrollbar_thumb_hover = c.text_secondary;
    colors.success = c.new;
    colors.danger = c.deleted;
    colors.warning = c.modified;
    colors.info = c.renamed;
    colors.selection = c.accent.opacity(0.35);
  });
  window.refresh();
}
