use image::{DynamicImage, Rgba, RgbaImage};
use ws_visual_check::{pixel, window_point};

#[test]
fn reads_pixels_and_rejects_points_outside_the_image() {
  let mut image = RgbaImage::new(4, 3);
  image.put_pixel(3, 2, Rgba([10, 20, 30, 40]));
  let image = DynamicImage::ImageRgba8(image);
  assert_eq!(pixel(&image, 3, 2), Some(Rgba([10, 20, 30, 40])));
  assert_eq!(pixel(&image, 4, 2), None);
  assert_eq!(pixel(&image, 3, 3), None);
}

#[test]
fn maps_pixel_centers_to_logical_window_coordinates() {
  assert_eq!(window_point((200, 100), (100., 50.), (0, 0)), Some((0.25, 0.25)));
  assert_eq!(window_point((200, 100), (100., 50.), (199, 99)), Some((99.75, 49.75)));
  assert_eq!(window_point((200, 100), (100., 50.), (200, 0)), None);
  assert_eq!(window_point((0, 100), (100., 50.), (0, 0)), None);
  assert_eq!(window_point((200, 100), (f64::NAN, 50.), (0, 0)), None);
}
