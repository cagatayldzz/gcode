use std::{
  io::{Read, Write},
  net::TcpListener,
  sync::mpsc,
  thread,
};
use vekro_hosting::{
  check_access, create_pull_request, default_branch, pull_requests, repositories, verify, Account, Connection, NewPullRequest, Provider, RepoRef,
  TokenAccess,
};

struct Request {
  method: String,
  target: String,
  headers: String,
  body: String,
}

/// Answers every request with what `route` says and reports each request it saw.
fn serve(route: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> (String, mpsc::Receiver<Request>) {
  serve_with_headers(move |method, target| {
    let (status, body) = route(method, target);
    (status, Vec::new(), body)
  })
}

/// Like [`serve`], with response headers.
fn serve_with_headers(
  route: impl Fn(&str, &str) -> (u16, Vec<(&'static str, &'static str)>, String) + Send + 'static,
) -> (String, mpsc::Receiver<Request>) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let (sender, receiver) = mpsc::channel();
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
      let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
      let mut first = head.lines().next().unwrap_or("").split(' ');
      let (method, target) = (first.next().unwrap_or("").to_string(), first.next().unwrap_or("").to_string());
      let (status, extra, answer) = route(&method, &target);
      let _ = sender.send(Request { method, target, headers: head.to_ascii_lowercase(), body: body.to_string() });
      let extra: String = extra.iter().map(|(name, value)| format!("{name}: {value}\r\n")).collect();
      let response = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{answer}",
        answer.len()
      );
      let _ = stream.write_all(response.as_bytes());
    }
  });
  (base, receiver)
}

fn connection(provider: Provider, base: &str, organization: Option<&str>) -> Connection {
  Connection {
    account: Account { provider, host: base.into(), organization: organization.map(str::to_string), username: "me".into(), issue: None },
    token: "pat".into(),
  }
}

fn github_repo() -> RepoRef {
  RepoRef { provider: Provider::GitHub, namespace: "octo".into(), name: "app".into(), project: None }
}

#[test]
fn github_verifies_lists_and_opens_pull_requests() {
  let (base, requests) = serve(|method, target| {
    match (method, target) {
    ("GET", "/api/v3/user") => (200, r#"{"login":"octocat"}"#.into()),
    ("GET", t) if t.starts_with("/api/v3/user/repos") => (200, r#"[{"full_name":"octo/app","clone_url":"https://h/octo/app.git","private":true,"description":"An app"}]"#.into()),
    ("GET", t) if t.starts_with("/api/v3/repos/octo/app/pulls") => (
      200,
      r#"[{"number":7,"title":"Fix it","user":{"login":"octocat"},"html_url":"https://h/octo/app/pull/7","draft":true,"head":{"ref":"fix"},"base":{"ref":"main"}}]"#.into(),
    ),
    ("POST", "/api/v3/repos/octo/app/pulls") => (201, r#"{"number":8,"title":"New","user":{"login":"octocat"},"html_url":"https://h/octo/app/pull/8","head":{"ref":"feature"},"base":{"ref":"main"}}"#.into()),
    ("GET", "/api/v3/repos/octo/app") => (200, r#"{"default_branch":"develop"}"#.into()),
    _ => (404, "{}".into()),
  }
  });
  let github = connection(Provider::GitHub, &base, None);
  assert_eq!(verify(&github).unwrap(), "octocat");
  let first = requests.recv().unwrap();
  assert!(first.headers.contains("authorization: bearer pat") && first.headers.contains("user-agent: vekro"));

  let found = repositories(&github).unwrap();
  assert_eq!((found[0].name.as_str(), found[0].clone_url.as_str(), found[0].private), ("octo/app", "https://h/octo/app.git", true));

  let open = pull_requests(&github, &github_repo()).unwrap();
  assert_eq!((open[0].number, open[0].title.as_str(), open[0].author.as_str(), open[0].draft), (7, "Fix it", "octocat", true));
  assert_eq!((open[0].source.as_str(), open[0].target.as_str()), ("fix", "main"));

  assert_eq!(default_branch(&github, &github_repo()).unwrap(), "develop");
  let created = create_pull_request(
    &github,
    &github_repo(),
    &NewPullRequest { title: "New".into(), description: "Body".into(), source: "feature".into(), target: "main".into(), draft: false },
  )
  .unwrap();
  assert_eq!((created.number, created.url.as_str()), (8, "https://h/octo/app/pull/8"));
  let post = std::iter::from_fn(|| requests.try_recv().ok()).find(|request| request.method == "POST").unwrap();
  let body: serde_json::Value = serde_json::from_str(&post.body).unwrap();
  assert_eq!(
    (body["head"].as_str(), body["base"].as_str(), body["title"].as_str(), body["draft"].as_bool()),
    (Some("feature"), Some("main"), Some("New"), Some(false))
  );
}

#[test]
fn gitlab_names_the_project_by_its_encoded_path_and_marks_drafts() {
  let (base, requests) = serve(|method, target| match (method, target) {
    ("GET", "/api/v4/user") => (200, r#"{"username":"gl"}"#.into()),
    ("GET", t) if t.starts_with("/api/v4/projects?") => (
      200,
      r#"[{"path_with_namespace":"grp/sub/app","http_url_to_repo":"https://h/grp/sub/app.git","visibility":"private","description":null}]"#.into(),
    ),
    ("POST", "/api/v4/projects/grp%2Fsub%2Fapp/merge_requests") => (
      201,
      r#"{"iid":3,"title":"Draft: Work","author":{"username":"gl"},"web_url":"https://h/mr/3","source_branch":"w","target_branch":"main"}"#.into(),
    ),
    ("GET", t) if t.starts_with("/api/v4/projects/grp%2Fsub%2Fapp/merge_requests") => (
      200,
      r#"[{"iid":3,"title":"Work","author":{"username":"gl"},"web_url":"https://h/mr/3","draft":false,"source_branch":"w","target_branch":"main"}]"#
        .into(),
    ),
    _ => (404, "{}".into()),
  });
  let gitlab = connection(Provider::GitLab, &base, None);
  let repository = RepoRef { provider: Provider::GitLab, namespace: "grp/sub".into(), name: "app".into(), project: None };
  assert_eq!(verify(&gitlab).unwrap(), "gl");
  assert!(requests.recv().unwrap().headers.contains("private-token: pat"));
  assert_eq!(repositories(&gitlab).unwrap()[0].name, "grp/sub/app");
  assert_eq!(pull_requests(&gitlab, &repository).unwrap()[0].number, 3);
  let created = create_pull_request(
    &gitlab,
    &repository,
    &NewPullRequest { title: "Work".into(), description: String::new(), source: "w".into(), target: "main".into(), draft: true },
  )
  .unwrap();
  assert_eq!(created.number, 3);
  let post = std::iter::from_fn(|| requests.try_recv().ok()).find(|request| request.method == "POST").unwrap();
  let body: serde_json::Value = serde_json::from_str(&post.body).unwrap();
  assert_eq!((body["title"].as_str(), body["source_branch"].as_str()), (Some("Draft: Work"), Some("w")));
  assert_eq!(post.target, "/api/v4/projects/grp%2Fsub%2Fapp/merge_requests");
}

#[test]
fn azure_devops_signs_in_with_a_basic_header_and_builds_pull_request_links() {
  let (base, requests) = serve(|method, target| {
    match (method, target) {
    ("GET", t) if t.starts_with("/contoso/_apis/projects") => (200, r#"{"count":1,"value":[{"name":"Shop"}]}"#.into()),
    ("GET", t) if t.starts_with("/contoso/_apis/git/repositories?") => (
      200,
      r#"{"value":[{"name":"shop","remoteUrl":"https://contoso@dev.azure.com/contoso/Shop/_git/shop","project":{"name":"Shop","visibility":"private"}}]}"#.into(),
    ),
    ("POST", t) if t.starts_with("/contoso/Shop/_apis/git/repositories/shop/pullrequests") => {
      (201, r#"{"pullRequestId":12,"title":"T","createdBy":{"displayName":"Me"},"sourceRefName":"refs/heads/f","targetRefName":"refs/heads/main"}"#.into())
    }
    ("GET", t) if t.starts_with("/contoso/Shop/_apis/git/repositories/shop?") => (200, r#"{"defaultBranch":"refs/heads/main"}"#.into()),
    _ => (404, "{}".into()),
  }
  });
  let azure = connection(Provider::AzureDevOps, &base, Some("contoso"));
  let repository = RepoRef { provider: Provider::AzureDevOps, namespace: "contoso".into(), name: "shop".into(), project: Some("Shop".into()) };
  assert_eq!(verify(&azure).unwrap(), "contoso");
  assert!(requests.recv().unwrap().headers.contains("authorization: basic onbhda=="), "the token is sent as the password, with no user name");
  let listed = repositories(&azure).unwrap();
  assert_eq!(
    (listed[0].name.as_str(), listed[0].clone_url.as_str(), listed[0].private),
    ("Shop/shop", "https://dev.azure.com/contoso/Shop/_git/shop", true)
  );
  assert_eq!(default_branch(&azure, &repository).unwrap(), "main");
  let created = create_pull_request(
    &azure,
    &repository,
    &NewPullRequest { title: "T".into(), description: String::new(), source: "f".into(), target: "main".into(), draft: false },
  )
  .unwrap();
  assert_eq!((created.number, created.source.as_str(), created.target.as_str()), (12, "f", "main"));
  assert_eq!(created.url, format!("{base}/contoso/Shop/_git/shop/pullrequest/12"));
  let post = std::iter::from_fn(|| requests.try_recv().ok()).find(|request| request.method == "POST").unwrap();
  let body: serde_json::Value = serde_json::from_str(&post.body).unwrap();
  assert_eq!((body["sourceRefName"].as_str(), body["targetRefName"].as_str()), (Some("refs/heads/f"), Some("refs/heads/main")));
}

#[test]
fn a_rejected_token_and_a_missing_organization_say_what_is_wrong() {
  let (base, _) = serve(|_, _| (401, r#"{"message":"Bad credentials"}"#.into()));
  assert!(verify(&connection(Provider::GitHub, &base, None)).unwrap_err().contains("rejected the token"));
  // Azure DevOps answers a bad token with a sign-in page and status 200.
  let (page, _) = serve(|_, _| (200, "<html>Sign in</html>".into()));
  assert!(verify(&connection(Provider::AzureDevOps, &page, Some("contoso"))).unwrap_err().contains("rejected the token"));
  assert!(verify(&connection(Provider::AzureDevOps, &page, None)).unwrap_err().contains("organization"));
  let (forbidden, _) = serve(|_, _| (403, r#"{"message":"Resource not accessible"}"#.into()));
  assert!(verify(&connection(Provider::GitHub, &forbidden, None)).unwrap_err().contains("lacks a permission"));
}

#[test]
fn a_classic_github_token_reports_its_scopes_and_expiry() {
  let (full, _) = serve_with_headers(|_, _| {
    (200, vec![("X-OAuth-Scopes", "repo, read:org"), ("github-authentication-token-expiration", "2027-03-01 00:00:00 UTC")], "{}".into())
  });
  let access = check_access(&connection(Provider::GitHub, &full, None));
  assert_eq!(access, TokenAccess { read: Some(true), write: Some(true), pull_requests: Some(true), expires: Some("2027-03-01 00:00:00 UTC".into()) });
  assert!(access.warnings(Provider::GitHub).is_empty() && access.unchecked(Provider::GitHub).is_none());

  let (narrow, _) = serve_with_headers(|_, _| (200, vec![("X-OAuth-Scopes", "read:user")], "{}".into()));
  let access = check_access(&connection(Provider::GitHub, &narrow, None));
  assert_eq!((access.read, access.write, access.pull_requests), (Some(false), Some(false), Some(false)));
  let warnings = access.warnings(Provider::GitHub);
  assert!(warnings.iter().any(|warning| warning.contains("cannot push") && warning.contains("`repo`")), "{warnings:?}");
}

#[test]
fn a_fine_grained_github_token_can_only_be_tried_for_reading() {
  let (base, _) = serve(|_, target| if target.starts_with("/api/v3/user/repos") { (200, "[]".into()) } else { (200, "{}".into()) });
  let access = check_access(&connection(Provider::GitHub, &base, None));
  assert_eq!((access.read, access.write, access.pull_requests), (Some(true), None, None));
  assert!(access.warnings(Provider::GitHub).is_empty(), "nothing is known to be missing");
  assert!(access.unchecked(Provider::GitHub).unwrap().contains("not checked"));
}

#[test]
fn a_gitlab_token_reports_whether_it_has_the_api_scope() {
  let (api, _) = serve(|_, _| (200, r#"{"scopes":["api"],"expires_at":"2027-01-01"}"#.into()));
  let access = check_access(&connection(Provider::GitLab, &api, None));
  assert_eq!((access.read, access.write, access.pull_requests, access.expires.as_deref()), (Some(true), Some(true), Some(true), Some("2027-01-01")));

  let (git_only, _) = serve(|_, _| (200, r#"{"scopes":["read_repository","write_repository"]}"#.into()));
  let access = check_access(&connection(Provider::GitLab, &git_only, None));
  assert_eq!((access.read, access.write, access.pull_requests), (Some(true), Some(true), Some(false)));
  assert!(access.warnings(Provider::GitLab).iter().any(|warning| warning.contains("pull requests") && warning.contains("`api`")));

  let (old, _) = serve(|_, _| (404, "{}".into()));
  assert_eq!(check_access(&connection(Provider::GitLab, &old, None)), TokenAccess::default());
}

#[test]
fn an_azure_devops_token_is_tried_for_reading_and_the_rest_stays_unknown() {
  let (base, _) =
    serve(|_, target| if target.starts_with("/contoso/_apis/git/repositories") { (200, r#"{"value":[]}"#.into()) } else { (404, "{}".into()) });
  let access = check_access(&connection(Provider::AzureDevOps, &base, Some("contoso")));
  assert_eq!((access.read, access.write, access.pull_requests), (Some(true), None, None));
  assert!(access.warnings(Provider::AzureDevOps).is_empty());
  assert!(access.unchecked(Provider::AzureDevOps).unwrap().contains("Code: Read & write"));
}

#[test]
fn a_refused_request_names_the_permission_the_service_needs() {
  let (base, _) = serve(|_, _| (403, r#"{"message":"nope"}"#.into()));
  let error = create_pull_request(
    &connection(Provider::AzureDevOps, &base, Some("contoso")),
    &RepoRef { provider: Provider::AzureDevOps, namespace: "contoso".into(), name: "r".into(), project: Some("p".into()) },
    &NewPullRequest { title: "t".into(), description: String::new(), source: "a".into(), target: "b".into(), draft: false },
  )
  .unwrap_err();
  assert!(error.contains("Code: Read & write"), "{error}");
}

#[test]
fn permissions_are_probed_without_changing_anything() {
  use vekro_hosting::verify_permissions;
  // A token that may push and open pull requests: the push advertisement is given, the empty pull request is "unprocessable".
  let (base, requests) = serve(|method, target| match (method, target) {
    ("GET", t) if t.starts_with("/o/r.git/info/refs?service=git-receive-pack") => (200, "001e# service=git-receive-pack\n".into()),
    ("POST", "/api/v3/repos/octo/app/pulls") => (422, r#"{"message":"Validation Failed"}"#.into()),
    _ => (404, "{}".into()),
  });
  let github = connection(Provider::GitHub, &base, None);
  let report = verify_permissions(&github, &github_repo(), &format!("{base}/o/r.git"));
  assert_eq!(report, report_of(Some(true), Some(true)));
  assert!(report.missing(Provider::GitHub).is_empty());
  let probe = requests.recv().unwrap();
  assert_eq!(probe.method, "GET");
  assert!(probe.headers.contains("authorization: basic bwu6cgf0"), "the token is the password of a Basic login: {}", probe.headers);
  let post = requests.recv().unwrap();
  assert_eq!((post.method.as_str(), post.body.replace(char::is_whitespace, "").as_str()), ("POST", "{}"), "the pull request probe is empty");

  // A read-only token: the push advertisement and pull requests are refused.
  let (read_only, _) = serve(|_, _| (403, r#"{"message":"Resource not accessible by personal access token"}"#.into()));
  let github = connection(Provider::GitHub, &read_only, None);
  let report = verify_permissions(&github, &github_repo(), &format!("{read_only}/o/r.git"));
  assert_eq!(report, report_of(Some(false), Some(false)));
  let missing = report.missing(Provider::GitHub);
  assert!(missing[0].contains("push") && missing[0].contains("Contents: Read and write"), "{missing:?}");
  assert!(missing[1].contains("pull requests") && missing[1].contains("Pull requests: Read and write"), "{missing:?}");

  // An answer that says nothing (the service is down) leaves it unknown instead of blaming the token.
  let (down, _) = serve(|_, _| (500, "{}".into()));
  let github = connection(Provider::GitHub, &down, None);
  let report = verify_permissions(&github, &github_repo(), &format!("{down}/o/r.git"));
  assert_eq!(report, report_of(None, None));
  assert!(report.missing(Provider::GitHub).is_empty());
}

fn report_of(push: Option<bool>, pull_requests: Option<bool>) -> vekro_hosting::PermissionReport {
  vekro_hosting::PermissionReport { push, pull_requests }
}

#[test]
fn permission_problems_are_told_from_other_failures() {
  use vekro_hosting::is_permission_problem;
  assert!(is_permission_problem("GitHub refused the request: the token lacks a permission. It needs X."));
  assert!(is_permission_problem("GitHub rejected the token. Check that it is correct."));
  assert!(!is_permission_problem("Could not reach GitHub: connection refused"));
  assert!(!is_permission_problem("GitHub could not find it."));
}
