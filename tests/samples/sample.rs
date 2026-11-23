// Syntax sample: Rust.
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct User<'a> {
  pub id: u32,
  pub name: &'a str,
}

const LIMIT: usize = 16;

pub fn find<'a>(users: &'a [User<'a>], id: u32) -> Option<&'a User<'a>> {
  users.iter().find(|user| user.id == id)
}

fn main() {
  let mut map: HashMap<String, i32> = HashMap::new();
  map.insert("one".to_string(), 1);
  match map.get("one") {
    Some(value) if *value > 0 => println!("{value}"),
    _ => eprintln!("missing"),
  }
}
