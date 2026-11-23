//! Syntax sample: Rust (module documentation).

use std::{
  collections::{BTreeMap, HashMap},
  fmt::{self, Display},
  sync::Arc,
};

/// A documented constant.
pub const LIMIT: usize = 16;
static GREETING: &str = "hello\tworld\n";
const HEX: u32 = 0xFF_u32;

#[derive(Debug, Clone, PartialEq)]
pub enum Role {
  Admin,
  Guest { name: String },
  Id(u32),
}

#[derive(Debug, Clone)]
pub struct User<'a> {
  pub id: u32,
  pub name: &'a str,
  roles: Vec<Role>,
}

pub trait Describe {
  type Output: Display;
  fn describe(&self) -> Self::Output;
}

impl<'a> Describe for User<'a> {
  type Output = String;

  fn describe(&self) -> String {
    format!("{} ({})", self.name, self.id)
  }
}

impl fmt::Display for Role {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Role::Admin => write!(f, "admin"),
      Role::Guest { name } => write!(f, "guest {name}"),
      Role::Id(id) => write!(f, "#{id}"),
    }
  }
}

macro_rules! square {
  ($x:expr) => {
    $x * $x
  };
}

/* A block comment
   over two lines. */
pub fn find<'a>(users: &'a [User<'a>], id: u32) -> Option<&'a User<'a>> {
  users.iter().find(|user| user.id == id)
}

async fn load(path: &str) -> Result<String, Box<dyn std::error::Error>> {
  let text = std::fs::read_to_string(path)?;
  Ok(text)
}

fn main() {
  let mut map: HashMap<String, i32> = HashMap::new();
  let shared = Arc::new(BTreeMap::<u8, char>::new());
  map.insert("one".to_string(), 1);
  let raw = r#"raw "string""#;
  let ch = 'x';
  let float = 3.14e-2_f64;
  let total = (1..=10).filter(|n| n % 2 == 0).map(|n| square!(n)).sum::<i32>();
  match map.get("one") {
    Some(value) if *value > 0 => println!("{value} {total}"),
    Some(_) | None => eprintln!("missing {raw} {ch} {float}"),
  }
  if let Some(user) = find(&[], 1) {
    println!("{}", user.describe());
  } else if shared.is_empty() {
    loop {
      break;
    }
  }
  unsafe {
    let _ = LIMIT as u8;
  }
}
