//! A deterministic stdio server for the language client integration tests.

use serde_json::{json, Value};
use std::{
  io::{self, BufRead, Read, Write},
  thread,
  time::Duration,
};

fn receive(input: &mut impl BufRead) -> Option<Value> {
  let mut length = None;
  loop {
    let mut line = String::new();
    if input.read_line(&mut line).unwrap() == 0 {
      return None;
    }
    if line.trim().is_empty() {
      let mut body = vec![0; length.expect("Content-Length header")];
      input.read_exact(&mut body).unwrap();
      return Some(serde_json::from_slice(&body).unwrap());
    }
    if let Some((name, value)) = line.split_once(':') {
      if name.eq_ignore_ascii_case("Content-Length") {
        length = Some(value.trim().parse().unwrap());
      }
    }
  }
}

fn send(output: &mut impl Write, mut message: Value) {
  message["jsonrpc"] = json!("2.0");
  let body = serde_json::to_vec(&message).unwrap();
  write!(output, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
  output.write_all(&body).unwrap();
  output.flush().unwrap();
}

fn main() {
  let scenario = std::env::args().nth(1).expect("scenario");
  let mut input = io::stdin().lock();
  let mut output = io::stdout().lock();
  let init = receive(&mut input).unwrap();
  assert_eq!(init["method"], "initialize");
  let invalid_frame = match scenario.as_str() {
    "oversized" => "Content-Length: 1000000000\r\n\r\n",
    "truncated" => "Content-Length: 200\r\n\r\n{}",
    "malformed" => "Content-Length: 3\r\n\r\n{{{",
    _ => "",
  };
  if !invalid_frame.is_empty() {
    output.write_all(invalid_frame.as_bytes()).unwrap();
    output.flush().unwrap();
    if scenario != "truncated" {
      io::copy(&mut input, &mut io::sink()).unwrap();
    }
    return;
  }

  // The client can write snapshots while initialization is pending.
  thread::sleep(Duration::from_millis(100));
  send(&mut output, json!({"id": init["id"], "result": {"capabilities": {"textDocumentSync": 2, "documentFormattingProvider": true}}}));
  assert_eq!(receive(&mut input).unwrap()["method"], "initialized");
  let opened = receive(&mut input).unwrap();
  assert_eq!(opened["method"], "textDocument/didOpen");
  let document = &opened["params"]["textDocument"];
  assert_eq!(document["text"], "🦀 workspace");
  let version = document["version"].as_i64().unwrap();
  let diagnostics = |version, items| json!({"method": "textDocument/publishDiagnostics", "params": {"uri": document["uri"], "version": version, "diagnostics": items}});
  send(&mut output, diagnostics(version - 1, json!([])));
  send(
    &mut output,
    diagnostics(
      version,
      json!([{"severity": 1, "message": "Current diagnostic", "range": {"start": {"line": 0, "character": 2}, "end": {"line": 0, "character": 4}}}]),
    ),
  );
  if scenario == "exit" {
    let _ = input.read(&mut [0]);
    return;
  }

  while let Some(message) = receive(&mut input) {
    let result = match message["method"].as_str() {
      Some("textDocument/didChange") => {
        let change = &message["params"]["contentChanges"][0];
        assert_eq!(change["range"]["end"], json!({"line": 0, "character": 12}));
        assert_eq!(change["text"], "🦀 new text");
        continue;
      }
      Some("textDocument/hover") => json!({"contents": {"kind": "markdown", "value": "**string**"}}),
      Some("textDocument/completion") => json!({"isIncomplete": false, "items": [{"label": "workspace"}]}),
      Some("textDocument/formatting") => {
        json!([{"range": {"start": {"line": 0, "character": 2}, "end": {"line": 0, "character": 12}}, "newText": " workspace formatted"}])
      }
      Some("test/error") => {
        send(&mut output, json!({"id": message["id"], "error": {"code": -32603, "message": "Formatter refused"}}));
        continue;
      }
      _ => continue,
    };
    send(&mut output, json!({"id": message["id"], "result": result}));
  }
}
