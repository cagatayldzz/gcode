//! Vekro's light and dark colors and fonts, switched with the system appearance.
//!
//! Both palettes follow Google Chrome: a gray (light) or near-black (dark) frame around the title band,
//! white or dark-gray pages, and one blue accent.

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
  /// The sidebar and the title band.
  pub surface: Hsla,
  /// A slightly raised surface: the status bar, empty-state icons.
  pub surface_alt: Hsla,
  pub border: Hsla,
  /// The outline of a field or a checkbox: stronger than `border`, which only separates areas.
  pub control_border: Hsla,
  /// The row that is chosen.
  pub selected: Hsla,
  pub hover: Hsla,
  /// The title band: Chrome's window frame.
  pub rail: Hsla,
  pub rail_hover: Hsla,
  /// The hover color of an item on a `rail_hover` background, such as the inside of a category.
  pub rail_item_hover: Hsla,
  pub rail_text: Hsla,
  /// The accent: Chrome's blue.
  pub accent: Hsla,
  pub accent_hover: Hsla,
  /// Text and icons drawn on the accent color.
  pub on_accent: Hsla,
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
  /// Chrome's dark theme: a dark frame, lighter toolbar grays and the light blue of its buttons.
  fn dark() -> Self {
    Self {
      background: hex(0x202124),
      text: hex(0xe8eaed),
      text_secondary: hex(0x9aa0a6),
      surface: hex(0x292a2d),
      surface_alt: hex(0x35363a),
      border: hex(0x3c4043),
      control_border: hex(0x5f6368),
      selected: hex(0x394457),
      hover: hex(0x35363a),
      rail: hex(0x171717),
      rail_hover: hex(0x2d2e31),
      rail_item_hover: hex(0x3c4043),
      rail_text: hex(0x9aa0a6),
      accent: hex(0x8ab4f8),
      accent_hover: hex(0xaecbfa),
      on_accent: hex(0x062e6f),
      link: hex(0x8ab4f8),
      new: hex(0x81c995),
      deleted: hex(0xf28b82),
      modified: hex(0xfdd663),
      renamed: hex(0x8ab4f8),
      conflicted: hex(0xfcad70),
    }
  }

  /// Chrome's light theme: a gray frame, white pages and toolbars, Google blue.
  fn light() -> Self {
    Self {
      background: hex(0xffffff),
      text: hex(0x202124),
      text_secondary: hex(0x5f6368),
      surface: hex(0xf8f9fa),
      surface_alt: hex(0xf1f3f4),
      border: hex(0xdadce0),
      control_border: hex(0x80868b),
      selected: hex(0xe8f0fe),
      hover: hex(0xf1f3f4),
      rail: hex(0xdee1e6),
      rail_hover: hex(0xd0d4da),
      rail_item_hover: hex(0xc4c8ce),
      rail_text: hex(0x3c4043),
      accent: hex(0x1a73e8),
      accent_hover: hex(0x1b66c9),
      on_accent: hex(0xffffff),
      link: hex(0x1a73e8),
      new: hex(0x188038),
      deleted: hex(0xd93025),
      modified: hex(0xb06000),
      renamed: hex(0x1a73e8),
      conflicted: hex(0xe8710a),
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
pub(crate) fn apply_theme(choice: ThemeChoice, code_font_size: u32, window: &mut Window, cx: &mut App) {
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
    theme.mono_font_size = px(code_font_size as f32);
    // Dialogs and notifications are rounded with `radius_lg`; fields, buttons and menu items with `radius`.
    theme.radius = px(6.);
    theme.radius_lg = px(12.);
    theme.notification.width = px(360.);
    let style = &mut std::sync::Arc::make_mut(&mut theme.highlight_theme).style;
    // The design shows no highlight on the line of the cursor.
    style.editor_active_line = Some(c.background);
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
    colors.sidebar_primary_foreground = c.on_accent;
    colors.border = c.border;
    colors.input = c.control_border;
    colors.tab_bar_segmented = c.surface_alt;
    // A switch that is off is a gray track.
    colors.switch = c.rail_item_hover;
    colors.tab_active = c.selected;
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
    colors.primary_foreground = c.on_accent;
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
