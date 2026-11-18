//! Vekro's light and dark colors and fonts, switched with the system appearance.
//! The palettes use semantic colors from GitHub Primer (https://github.com/primer/primitives).

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
  /// The window title band.
  pub rail: Hsla,
  pub rail_hover: Hsla,
  /// The hover color of an item on a `rail_hover` background, such as the inside of a category.
  pub rail_item_hover: Hsla,
  pub rail_text: Hsla,
  /// The accent for selection controls and badges.
  pub accent: Hsla,
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
      text: hex(0xf0f6fc),
      text_secondary: hex(0x9198a1),
      surface: hex(0x0d1117),
      surface_alt: hex(0x151b23),
      border: hex(0x252c35),
      control_border: hex(0x3d444d),
      selected: hex(0x172b45),
      hover: hex(0x151b23),
      rail: hex(0x010409),
      rail_hover: hex(0x151b23),
      rail_item_hover: hex(0x212830),
      rail_text: hex(0x9198a1),
      accent: hex(0x1f6feb),
      accent_hover: hex(0x1c62d2),
      on_accent: hex(0xffffff),
      primary_button: hex(0x238636),
      primary_button_hover: hex(0x29903b),
      primary_button_active: hex(0x216e39),
      link: hex(0x4493f8),
      link_hover: hex(0x79b8ff),
      new: hex(0x3fb950),
      deleted: hex(0xf85149),
      modified: hex(0xd29922),
      renamed: hex(0x4493f8),
      conflicted: hex(0xdb6d28),
      diff_added: hex(0x103c1d),
      diff_removed: hex(0x4a1214),
    }
  }

  fn light() -> Self {
    Self {
      background: hex(0xffffff),
      text: hex(0x1f2328),
      text_secondary: hex(0x59636e),
      surface: hex(0xffffff),
      surface_alt: hex(0xf6f8fa),
      border: hex(0xe2e7ec),
      control_border: hex(0x818b98),
      selected: hex(0xddf4ff),
      hover: hex(0xf6f8fa),
      rail: hex(0xf6f8fa),
      rail_hover: hex(0xeaeef2),
      rail_item_hover: hex(0xd1d9e0),
      rail_text: hex(0x59636e),
      accent: hex(0x0969da),
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
      diff_added: hex(0xdafbe1),
      diff_removed: hex(0xffebe9),
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
    theme.mono_font_family = MONO_FONT.into();
    theme.mono_font_size = px(code_font_size as f32);
    // Dialogs and notifications are rounded with `radius_lg`; fields, buttons and menu items with `radius`.
    theme.radius = px(6.);
    theme.radius_lg = px(10.);
    theme.notification.width = px(360.);
    let style = &mut std::sync::Arc::make_mut(&mut theme.highlight_theme).style;
    // The design shows no highlight on the line of the cursor.
    style.editor_background = Some(c.background);
    style.editor_gutter_background = Some(c.background);
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
    colors.title_bar = c.rail;
    colors.title_bar_border = c.border;
    colors.status_bar = c.surface_alt;
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
    colors.tab_active = c.selected;
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
