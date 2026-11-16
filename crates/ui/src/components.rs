//! Interface pieces used in more than one place.

use gpui_kit::{
  component::{button::ButtonCustomVariant, ActiveTheme as _},
  *,
};

/// The blue fill of a chosen avatar, tab or option. The kit's own primary style inverts the text color,
/// which is black on white in a dark theme.
pub(crate) fn selected_style(cx: &App) -> ButtonCustomVariant {
  let primary = cx.theme().primary;
  ButtonCustomVariant::new(cx).color(primary).foreground(white()).hover(primary).active(primary)
}
