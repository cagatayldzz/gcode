use std::{
  io::{Read, Write},
  net::TcpListener,
  sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
  },
  thread,
  time::{SystemTime, UNIX_EPOCH},
};
use ws_hosting::{current_name, default_branch, pull_requests, rename_repository, Account, Connection, Provider, RepoRef};

type Reply = (u16, Vec<(&'static str, String)>, String);

/// A server that answers each request with `route(method, target, request headers)` and counts them.
fn serve(route: impl Fn(&str, &str, &str) -> Reply + Send + 'static) -> (String, Arc<AtomicUsize>) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let count = Arc::new(AtomicUsize::new(0));
  let seen = count.clone();
  thread::spawn(move || {
    for stream in listener.incoming() {
      let Ok(mut stream) = stream else { break };
      let mut data = Vec::new();
      let mut buffer = [0u8; 4096];
      loop {
        let read = stream.read(&mut buffer).unwrap_or(0);
        data.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&data).to_string();
        if let Some((head, rest)) = text.split_once("\r\n\r\n") {
          let length = head
            .lines()
            .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|value| value.trim().parse::<usize>().unwrap_or(0)))
            .unwrap_or(0);
          if rest.len() >= length {
            break;
          }
        }
        if read == 0 {
          break;
        }
      }
      let text = String::from_utf8_lossy(&data).to_string();
      let head = text.split_once("\r\n\r\n").map_or(text.as_str(), |(head, _)| head);
      let mut first = head.lines().next().unwrap_or("").split(' ');
      let (method, target) = (first.next().unwrap_or("").to_string(), first.next().unwrap_or("").to_string());
      seen.fetch_add(1, Ordering::SeqCst);
      let (status, extra, answer) = route(&method, &target, &head.to_ascii_lowercase());
      let extra: String = extra.iter().map(|(name, value)| format!("{name}: {value}\r\n")).collect();
      let response = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{answer}",
        answer.len()
      );
      let _ = stream.write_all(response.as_bytes());
    }
  });
  (base, count)
}

fn github(base: &str) -> Connection {
  Connection {
    account: Account { provider: Provider::GitHub, host: base.into(), organization: None, username: "me".into(), issue: None },
    token: "pat".into(),
  }
}

fn repository() -> RepoRef {
  RepoRef { provider: Provider::GitHub, namespace: "octo".into(), name: "app".into(), project: None }
}

fn now() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[test]
fn the_same_read_is_not_asked_twice() {
  let (base, count) = serve(|_, _, _| (200, Vec::new(), r#"{"name":"app","default_branch":"main"}"#.into()));
  let connection = github(&base);
  assert_eq!(current_name(&connection, &repository()).unwrap(), "app");
  // Another question about the same URL is answered from what was read.
  assert_eq!(default_branch(&connection, &repository()).unwrap(), "main");
  assert_eq!(current_name(&connection, &repository()).unwrap(), "app");
  assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn identical_reads_at_the_same_time_share_one_request() {
  let (base, count) = serve(|_, _, _| {
    thread::sleep(std::time::Duration::from_millis(150));
    (200, Vec::new(), r#"{"name":"app"}"#.into())
  });
  let connection = github(&base);
  let workers: Vec<_> = (0..4)
    .map(|_| {
      let connection = connection.clone();
      thread::spawn(move || current_name(&connection, &repository()).unwrap())
    })
    .collect();
  for worker in workers {
    assert_eq!(worker.join().unwrap(), "app");
  }
  assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn a_refresh_asks_again_with_the_etag_and_keeps_the_answer_when_nothing_changed() {
  let (base, count) = serve(|_, _, headers| {
    if headers.contains("if-none-match: \"v1\"") {
      (304, vec![("ETag", "\"v1\"".into())], String::new())
    } else {
      (
        200,
        vec![("ETag", "\"v1\"".into())],
        r#"[{"number":3,"title":"One","user":{"login":"me"},"html_url":"u","head":{"ref":"a"},"base":{"ref":"main"}}]"#.into(),
      )
    }
  });
  let connection = github(&base);
  assert_eq!(pull_requests(&connection, &repository()).unwrap()[0].number, 3);
  assert_eq!(pull_requests(&connection, &repository()).unwrap().len(), 1);
  assert_eq!(count.load(Ordering::SeqCst), 1);

  connection.expire();
  let again = pull_requests(&connection, &repository()).unwrap();
  assert_eq!((again.len(), again[0].title.as_str()), (1, "One"));
  assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn a_change_makes_what_was_read_stale() {
  let (base, count) = serve(|method, _, _| match method {
    "PATCH" => (200, Vec::new(), r#"{"name":"renamed"}"#.into()),
    _ => (200, Vec::new(), r#"{"name":"app"}"#.into()),
  });
  let connection = github(&base);
  assert_eq!(current_name(&connection, &repository()).unwrap(), "app");
  assert_eq!(rename_repository(&connection, &repository(), "renamed").unwrap(), "renamed");
  assert_eq!(current_name(&connection, &repository()).unwrap(), "app");
  assert_eq!(count.load(Ordering::SeqCst), 3);
}

#[test]
fn a_service_that_used_up_its_limit_is_left_alone_until_it_resets() {
  let reset = (now() + 600).to_string();
  let (base, count) = serve(move |_, _, _| {
    (403, vec![("X-RateLimit-Remaining", "0".into()), ("X-RateLimit-Reset", reset.clone())], r#"{"message":"API rate limit exceeded"}"#.into())
  });
  let connection = github(&base);
  let error = current_name(&connection, &repository()).unwrap_err();
  assert!(error.contains("limiting requests") && error.contains("UTC"), "{error}");
  assert!(!error.contains("lacks a permission"), "{error}");
  assert!(!ws_hosting::is_permission_problem(&error));

  // Nothing more is sent while it asked for time.
  let again = pull_requests(&connection, &repository()).unwrap_err();
  assert!(again.contains("limiting requests"), "{again}");
  assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn a_refused_token_is_not_taken_for_a_rate_limit() {
  let (base, count) = serve(|_, _, _| (403, vec![("X-RateLimit-Remaining", "4999".into())], r#"{"message":"Resource not accessible"}"#.into()));
  let connection = github(&base);
  assert!(current_name(&connection, &repository()).unwrap_err().contains("lacks a permission"));
  assert!(default_branch(&connection, &repository()).unwrap_err().contains("lacks a permission"));
  assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn a_limited_service_still_gives_the_last_answer_it_had() {
  let (base, count) = serve(|_, target, _| {
    if target.contains("/repos/octo/app") && SEEN.fetch_add(1, Ordering::SeqCst) == 0 {
      (200, Vec::new(), r#"{"name":"app"}"#.into())
    } else {
      (429, vec![("Retry-After", "120".into())], "{}".into())
    }
  });
  static SEEN: AtomicUsize = AtomicUsize::new(0);
  let connection = github(&base);
  assert_eq!(current_name(&connection, &repository()).unwrap(), "app");
  connection.expire();
  assert_eq!(current_name(&connection, &repository()).unwrap(), "app");
  assert_eq!(count.load(Ordering::SeqCst), 2);
}
