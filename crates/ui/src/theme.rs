//! Vekro's light and dark colors and fonts, switched with the system appearance.
//!
//! The dark palette follows the project's design image (deep blue-black surfaces, one blue accent, the blue of
//! the logo); the light one is a cool slate counterpart with the same accent.

use gpui_kit::{
  component::{ActiveTheme as _, Theme, ThemeMode},
  *,
};
use vekro_storage::ThemeChoice;

/// The size of the text the interface draws itself. The kit's own parts (icons, dock tabs) are sized in `rem`,
/// which stays at the theme's 16 px.
pub(crate) const FONT_SIZE: Pixels = px(14.);

/// Secondary text, such as a changed file's folder.
pub(crate) const FONT_SIZE_SM: Pixels = px(12.);

/// The code font size of the editor and the diff.
const MONO_FONT_SIZE: Pixels = px(13.);

/// The interface font (bundled, see `init`).
const SANS_FONT: &str = "Geist";

/// The code font, used by the editor and the diff.
const MONO_FONT: &str = "Geist Mono";

/// The colors of one appearance.
#[derive(Clone, Copy)]
pub(crate) struct Colors {
  /// The main area: editor, terminal and the window.
  pub background: Hsla,
  pub text: Hsla,
  pub text_secondary: Hsla,
  /// The sidebar, the project rail and the title band.
  pub surface: Hsla,
  /// A slightly raised surface: the status bar, empty-state icons.
  pub surface_alt: Hsla,
  pub border: Hsla,
  /// The row that is chosen.
  pub selected: Hsla,
  pub hover: Hsla,
  /// The project rail on the far left: the colors of github.com's header and page border, in both appearances.
  pub rail: Hsla,
  pub rail_hover: Hsla,
  /// The hover color of an item on a `rail_hover` background, such as the inside of a category.
  pub rail_item_hover: Hsla,
  pub rail_text: Hsla,
  /// The accent: the blue of the logo.
  pub accent: Hsla,
  pub accent_hover: Hsla,
  pub link: Hsla,
  /// Marks for new, deleted, modified, renamed and conflicted files.
  pub new: Hsla,
  pub deleted: Hsla,
  pub modified: Hsla,
  pub renamed: Hsla,
  pub conflicted: Hsla,
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
      background: hex(0x0f1319),
      text: hex(0xe7eaf0),
      text_secondary: hex(0x8d96a5),
      surface: hex(0x131820),
      surface_alt: hex(0x181e28),
      border: hex(0x222a36),
      selected: hex(0x1f2836),
      hover: hex(0x1a212c),
      rail: hex(0x010409),
      rail_hover: hex(0x161b22),
      rail_item_hover: hex(0x242d3a),
      rail_text: hex(0x848d97),
      accent: hex(0x4d8dff),
      accent_hover: hex(0x6aa0ff),
      link: hex(0x6aa0ff),
      new: hex(0x3fb980),
      deleted: hex(0xf0646b),
      modified: hex(0xe2b13c),
      renamed: hex(0x4d8dff),
      conflicted: hex(0xf59a4a),
    }
  }

  fn light() -> Self {
    Self {
      background: hex(0xffffff),
      text: hex(0x1b2230),
      text_secondary: hex(0x667085),
      surface: hex(0xf4f6f9),
      surface_alt: hex(0xebeef3),
      border: hex(0xe1e5ec),
      selected: hex(0xe3e9f4),
      hover: hex(0xebeff6),
      rail: hex(0xf6f8fa),
      rail_hover: hex(0xeaeef2),
      rail_item_hover: hex(0xdce2ea),
      rail_text: hex(0x59636e),
      accent: hex(0x2f6bff),
      accent_hover: hex(0x4a7fff),
      link: hex(0x2f6bff),
      new: hex(0x1f9d63),
      deleted: hex(0xd8424a),
      modified: hex(0xb8860b),
      renamed: hex(0x2f6bff),
      conflicted: hex(0xd9702a),
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

/// Applies the chosen appearance (the system's, unless the user picked one) with Vekro's colors and fonts.
pub(crate) fn apply_theme(choice: ThemeChoice, window: &mut Window, cx: &mut App) {
  match choice {
    ThemeChoice::System => Theme::sync_system_appearance(Some(window), cx),
    ThemeChoice::Light => Theme::change(ThemeMode::Light, Some(window), cx),
    ThemeChoice::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
  }
  let c = colors(cx);
  Theme::update(cx, |theme| {
    theme.font_family = SANS_FONT.into();
    theme.font_size = px(16.);
    theme.mono_font_family = MONO_FONT.into();
    theme.mono_font_size = MONO_FONT_SIZE;
    let style = &mut std::sync::Arc::make_mut(&mut theme.highlight_theme).style;
    style.editor_active_line = Some(c.selected);
    style.editor_active_line_number = Some(c.text);
    let colors = &mut theme.colors;
    colors.background = c.background;
    colors.foreground = c.text;
    colors.sidebar = c.surface;
    colors.sidebar_foreground = c.text;
    colors.sidebar_border = c.border;
    colors.sidebar_accent = c.selected;
    colors.sidebar_accent_foreground = c.text;
    colors.sidebar_primary = c.accent;
    colors.sidebar_primary_foreground = white();
    colors.border = c.border;
    colors.input = c.border;
    colors.popover = c.surface;
    colors.popover_foreground = c.text;
    colors.muted = c.surface_alt;
    colors.muted_foreground = c.text_secondary;
    colors.secondary = c.surface_alt;
    colors.secondary_foreground = c.text;
    colors.accent = c.hover;
    colors.accent_foreground = c.text;
    colors.primary = c.accent;
    colors.primary_hover = c.accent_hover;
    colors.primary_active = c.accent;
    colors.primary_foreground = white();
    colors.ring = c.accent;
    colors.caret = c.text;
    colors.link = c.link;
    colors.link_hover = c.accent_hover;
    colors.link_active = c.accent;
    colors.list = c.surface;
    colors.list_even = c.surface;
    colors.list_hover = c.hover;
    colors.list_active = c.selected;
    colors.list_head = c.surface_alt;
    colors.success = c.new;
    colors.danger = c.deleted;
    colors.warning = c.modified;
    colors.info = c.renamed;
    colors.selection = c.accent.opacity(0.35);
  });
  window.refresh();
}
