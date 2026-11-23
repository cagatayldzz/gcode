use std::{
  io::{Read, Write},
  net::TcpListener,
  sync::mpsc,
  thread,
};
use ws_opencommit::{generate, Config};

/// Answers each connection with the next `(status, body)` and reports the request text it received.
fn serve(answers: Vec<(u16, String)>) -> (String, mpsc::Receiver<String>) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let url = format!("http://{}/v1", listener.local_addr().unwrap());
  let (sender, receiver) = mpsc::channel();
  thread::spawn(move || {
    for (status, body) in answers {
      let (mut stream, _) = listener.accept().unwrap();
      let mut request = Vec::new();
      let mut buffer = [0u8; 4096];
      loop {
        let read = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&request).to_string();
        if let Some((head, rest)) = text.split_once("\r\n\r\n") {
          let length = head
            .lines()
            .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|value| value.trim().parse::<usize>().unwrap()))
            .unwrap_or(0);
          if rest.len() >= length {
            break;
          }
        }
        if read == 0 {
          break;
        }
      }
      let _ = sender.send(String::from_utf8_lossy(&request).to_string());
      let response =
        format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
      stream.write_all(response.as_bytes()).unwrap();
    }
  });
  (url, receiver)
}

fn answer(content: &str) -> (u16, String) {
  (200, serde_json::json!({"choices": [{"message": {"content": content}}]}).to_string())
}

fn config(url: &str) -> Config {
  Config { api_key: "sk-test".into(), api_url: url.into(), ..Config::default() }
}

const DIFF: &str = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n+hello\n";

#[test]
fn the_diff_is_sent_with_the_key_and_the_answer_becomes_the_message() {
  let (url, requests) = serve(vec![answer("feat: greet the user")]);
  let message = generate(&config(&url), "Be brief.", DIFF).unwrap();
  assert_eq!(message.summary, "feat: greet the user");
  let request = requests.recv().unwrap();
  assert!(request.starts_with("POST /v1/chat/completions"));
  assert!(request.to_ascii_lowercase().contains("authorization: bearer sk-test"));
  assert!(request.contains("gpt-4o-mini"));
  assert!(request.contains("+hello"));
  assert!(request.contains("Be brief."));
}

#[test]
fn a_rejected_key_says_so() {
  let (url, _) = serve(vec![(401, r#"{"error":{"message":"bad key"}}"#.into())]);
  let error = generate(&config(&url), "", DIFF).unwrap_err();
  assert!(error.contains("API key was rejected"), "{error}");
}

#[test]
fn other_failures_carry_the_service_message() {
  let (url, _) = serve(vec![(500, r#"{"error":{"message":"overloaded"}}"#.into())]);
  let error = generate(&config(&url), "", DIFF).unwrap_err();
  assert!(error.contains("HTTP 500") && error.contains("overloaded"), "{error}");
}

#[test]
fn an_answer_without_text_is_an_error() {
  let (url, _) = serve(vec![answer("   ")]);
  assert!(generate(&config(&url), "", DIFF).is_err());
}

#[test]
fn a_large_diff_is_summarized_in_parts_and_merged() {
  let file = |name: &str| format!("diff --git a/{name} b/{name}\n+{}\n", "x".repeat(4000));
  let diff = format!("{}{}", file("a"), file("b"));
  let (url, requests) = serve(vec![answer("feat: part one"), answer("fix: part two"), answer("feat: both parts")]);
  let config = Config { max_input_tokens: 2_200, ..config(&url) };
  let message = generate(&config, "", &diff).unwrap();
  assert_eq!(message.summary, "feat: both parts");
  let first = requests.recv().unwrap();
  let second = requests.recv().unwrap();
  let merge = requests.recv().unwrap();
  assert!(first.contains("a/a") && !first.contains("a/b"));
  assert!(second.contains("a/b") && !second.contains("a/a b/a"));
  assert!(merge.contains("feat: part one") && merge.contains("fix: part two"));
}

#[test]
fn no_changes_and_no_key_fail_before_any_request() {
  assert!(generate(&Config { api_key: "k".into(), ..Config::default() }, "", "  ").unwrap_err().contains("no changes"));
  assert!(generate(&Config::default(), "", DIFF).unwrap_err().contains("API key"));
}
