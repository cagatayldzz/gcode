//! Pixel inspection and coordinate conversion for screenshots.

use image::{DynamicImage, GenericImageView, Rgba};

/// Returns the pixel at an image coordinate, or `None` outside the image.
pub fn pixel(image: &DynamicImage, x: u32, y: u32) -> Option<Rgba<u8>> {
  (x < image.width() && y < image.height()).then(|| image.get_pixel(x, y))
}

/// Converts an image pixel to a coordinate relative to a window of the given logical size.
pub fn window_point(image_size: (u32, u32), window_size: (f64, f64), point: (u32, u32)) -> Option<(f64, f64)> {
  let (width, height) = image_size;
  let (window_width, window_height) = window_size;
  if width == 0
    || height == 0
    || point.0 >= width
    || point.1 >= height
    || !window_width.is_finite()
    || !window_height.is_finite()
    || window_width <= 0.
    || window_height <= 0.
  {
    return None;
  }
  Some(((f64::from(point.0) + 0.5) * window_width / f64::from(width), (f64::from(point.1) + 0.5) * window_height / f64::from(height)))
}
