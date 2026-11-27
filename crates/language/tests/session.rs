use futures::{executor::block_on, StreamExt as _};
use serde_json::json;
use std::{path::Path, time::Instant};
use ws_language::{Server, Session};

fn server(root: &Path, scenario: &str) -> Server {
  let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/language_server.py");
  Server { program: "python3".into(), args: vec![fixture.to_string_lossy().into_owned(), scenario.into()], root: root.to_path_buf() }
}

#[test]
fn initialization_sends_the_latest_open_text_before_changes_and_requests() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("index.ts");
  let (session, mut updates) = Session::start(&server(root.path(), "protocol")).unwrap();
  session.sync(&path, "older text");
  session.sync(&path, "🦀 workspace");
  let update = block_on(updates.next()).unwrap();
  assert_eq!(update.path, path);
  assert_eq!(update.problems[0].message, "Current diagnostic");
  assert!(session.matches_document(&path, update.version, "🦀 workspace"));
  assert!(!session.matches_document(&path, update.version, "🦀 new text"));

  session.sync(&path, "🦀 new text");
  let answer = block_on(session.request("textDocument/completion", json!({}))).unwrap().unwrap();
  assert_eq!(answer["items"][0]["label"], "workspace");
  let hover = block_on(session.request("textDocument/hover", json!({}))).unwrap().unwrap();
  assert_eq!(hover["contents"]["value"], "**string**");
}

#[test]
fn requests_time_out_without_stopping_a_responsive_server() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("index.ts");
  let (session, mut updates) = Session::start(&server(root.path(), "protocol")).unwrap();
  session.sync(&path, "🦀 workspace");
  assert!(block_on(updates.next()).is_some());
  let started = Instant::now();

  assert!(block_on(session.request("test/noReply", json!({}))).is_err());
  assert!(started.elapsed() < std::time::Duration::from_secs(8));
  assert!(session.is_running());
  assert!(block_on(session.request("textDocument/hover", json!({}))).is_ok());
}

#[test]
fn malformed_oversized_and_truncated_frames_close_the_session() {
  let root = tempfile::tempdir().unwrap();
  for scenario in ["oversized", "malformed", "truncated"] {
    let (session, mut updates) = Session::start(&server(root.path(), scenario)).unwrap();
    assert!(block_on(updates.next()).is_none(), "{scenario}");
    assert!(!session.is_running(), "{scenario}");
    assert!(block_on(session.request("test/noReply", json!({}))).is_err());
  }
}

#[test]
fn a_server_exit_cancels_pending_requests_and_allows_a_new_session() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("index.ts");
  let (session, mut updates) = Session::start(&server(root.path(), "exit")).unwrap();
  session.sync(&path, "🦀 workspace");
  assert!(block_on(updates.next()).is_some());
  assert!(block_on(session.request("test/noReply", json!({}))).is_err());
  assert!(block_on(updates.next()).is_none());
  assert!(!session.is_running());

  let (new_session, mut updates) = Session::start(&server(root.path(), "protocol")).unwrap();
  new_session.sync(&path, "🦀 workspace");
  assert!(block_on(updates.next()).is_some());
  assert!(block_on(new_session.request("textDocument/hover", json!({}))).is_ok());
}

#[test]
fn formatting_capability_and_remote_errors_are_reported() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("index.ts");
  let (session, mut updates) = Session::start(&server(root.path(), "protocol")).unwrap();
  session.sync(&path, "🦀 workspace");
  assert!(block_on(updates.next()).is_some());
  assert!(session.supports_formatting());

  let reply = block_on(session.request("textDocument/formatting", json!({}))).unwrap().unwrap();
  let edits: Vec<lsp_types::TextEdit> = serde_json::from_value(reply).unwrap();
  assert_eq!(ws_language::apply_text_edits("🦀 workspace", &edits).unwrap(), "🦀 workspace formatted");
  assert_eq!(block_on(session.request("test/error", json!({}))).unwrap().unwrap_err(), "Formatter refused");
}
