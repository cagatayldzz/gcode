use std::path::{Path, PathBuf};

/// The `file://` address of an absolute path.
pub fn file_uri(path: &Path) -> String {
  let text = path.to_string_lossy();
  let mut uri = String::with_capacity(text.len() + 8);
  uri.push_str("file://");
  for byte in text.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => uri.push(byte as char),
      other => uri.push_str(&format!("%{other:02X}")),
    }
  }
  uri
}

/// The path of a `file://` address, or `None` for any other address.
pub fn file_path(uri: &str) -> Option<PathBuf> {
  let rest = uri.strip_prefix("file://")?;
  let bytes = rest.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut index = 0;
  while index < bytes.len() {
    if bytes[index] == b'%' && index + 2 < bytes.len() {
      let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
      out.push(u8::from_str_radix(hex, 16).ok()?);
      index += 3;
    } else {
      out.push(bytes[index]);
      index += 1;
    }
  }
  Some(PathBuf::from(String::from_utf8(out).ok()?))
}
