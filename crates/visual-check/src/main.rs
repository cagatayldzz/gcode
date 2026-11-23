use std::{env, error::Error, io};
use ws_visual_check::{pixel, window_point};

fn main() {
  if let Err(error) = run() {
    eprintln!("{error}");
    std::process::exit(1);
  }
}

fn run() -> Result<(), Box<dyn Error>> {
  let args: Vec<_> = env::args().skip(1).collect();
  if args.len() != 1 && args.len() != 3 && args.len() != 5 {
    return Err(io::Error::other("Usage: ws-visual-check IMAGE.png [X Y [WINDOW_WIDTH WINDOW_HEIGHT]]").into());
  }
  let image = image::open(&args[0])?;
  println!("image: {} x {} pixels", image.width(), image.height());
  if args.len() >= 3 {
    let x = args[1].parse()?;
    let y = args[2].parse()?;
    let rgba = pixel(&image, x, y).ok_or_else(|| io::Error::other("Pixel is outside the image"))?.0;
    println!("pixel ({x}, {y}): rgba({}, {}, {}, {})", rgba[0], rgba[1], rgba[2], rgba[3]);
    if args.len() == 5 {
      let size = (args[3].parse()?, args[4].parse()?);
      let (x, y) = window_point((image.width(), image.height()), size, (x, y)).ok_or_else(|| io::Error::other("Invalid window size"))?;
      println!("window point: ({x:.2}, {y:.2})");
    }
  }
  Ok(())
}
