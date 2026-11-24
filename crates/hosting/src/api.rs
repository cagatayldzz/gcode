use crate::{Connection, Provider, RepoRef};
use serde_json::{json, Value};
use std::time::Duration;

/// A repository the account can see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
  /// `owner/repo`, `group/subgroup/repo` or `project/repo`.
  pub name: String,
  /// The HTTPS address `git clone` takes.
  pub clone_url: String,
  pub private: bool,
  pub description: String,
}

/// An open pull request (a merge request on GitLab).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
  pub number: u64,
  pub title: String,
  pub author: String,
  /// Its page in the browser.
  pub url: String,
  pub draft: bool,
  pub source: String,
  pub target: String,
}

/// A pull request to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPullRequest {
  pub title: String,
  pub description: String,
  /// The branch with the changes.
  pub source: String,
  /// The branch they should go into.
  pub target: String,
  pub draft: bool,
}

const AZURE_VERSION: &str = "7.1";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 3;

fn base64(input: &[u8]) -> String {
  const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut out = String::new();
  for chunk in input.chunks(3) {
    let value = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
    for index in 0..4 {
      if index <= chunk.len() {
        out.push(TABLE[(value >> (18 - 6 * index) & 63) as usize] as char);
      } else {
        out.push('=');
      }
    }
  }
  out
}

/// Percent-encodes a path segment: GitLab names a project by its encoded path.
fn encode(text: &str) -> String {
  text.bytes().fold(String::new(), |mut out, byte| {
    if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
      out.push(byte as char);
    } else {
      out.push_str(&format!("%{byte:02X}"));
    }
    out
  })
}

impl Connection {
  fn api(&self) -> String {
    let account = &self.account;
    match account.provider {
      Provider::GitHub if account.hostname().eq_ignore_ascii_case("github.com") => "https://api.github.com".into(),
      Provider::GitHub => format!("{}/api/v3", account.base_url()),
      Provider::GitLab => format!("{}/api/v4", account.base_url()),
      Provider::AzureDevOps => format!("{}/{}", account.base_url(), account.organization.as_deref().unwrap_or_default()),
    }
  }

  fn headers(&self) -> Vec<(&'static str, String)> {
    let mut headers = vec![("User-Agent", ws_product::NAME.to_string()), ("Accept", "application/json".to_string())];
    match self.account.provider {
      Provider::GitHub => {
        headers.push(("Authorization", format!("Bearer {}", self.token)));
        headers.push(("X-GitHub-Api-Version", "2022-11-28".into()));
      }
      Provider::GitLab => headers.push(("PRIVATE-TOKEN", self.token.clone())),
      Provider::AzureDevOps => headers.push(("Authorization", format!("Basic {}", base64(format!(":{}", self.token).as_bytes())))),
    }
    headers
  }

  /// One request; the JSON answer. Failures say what to do about them.
  fn request(&self, method: &str, url: &str, body: Option<Value>) -> Result<Value, String> {
    self.request_with(method, url, body, &[]).map(|(value, _)| value)
  }

  /// Like [`Connection::request`], and the values of the response headers named in `capture` (in that order).
  fn request_with(&self, method: &str, url: &str, body: Option<Value>, capture: &[&str]) -> Result<(Value, Vec<Option<String>>), String> {
    ws_activity::timed(
      "http",
      || format!("{method} {url}"),
      || self.request_untimed(method, url, body, capture),
      |result| match result {
        Ok(_) => "ok".into(),
        Err(error) => format!("failed: {error}"),
      },
    )
  }

  fn request_untimed(&self, method: &str, url: &str, body: Option<Value>, capture: &[&str]) -> Result<(Value, Vec<Option<String>>), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).build().into();
    let service = self.account.provider.label();
    let sent = match (method, body) {
      ("POST", Some(body)) => {
        let mut request = agent.post(url);
        for (name, value) in self.headers() {
          request = request.header(name, value);
        }
        request.send_json(&body)
      }
      ("PATCH", Some(body)) => {
        let mut request = agent.patch(url);
        for (name, value) in self.headers() {
          request = request.header(name, value);
        }
        request.send_json(&body)
      }
      ("PUT", Some(body)) => {
        let mut request = agent.put(url);
        for (name, value) in self.headers() {
          request = request.header(name, value);
        }
        request.send_json(&body)
      }
      _ => {
        let mut request = agent.get(url);
        for (name, value) in self.headers() {
          request = request.header(name, value);
        }
        request.call()
      }
    };
    let mut response = sent.map_err(|error| format!("Could not reach {service}: {error}"))?;
    let status = response.status().as_u16();
    let captured: Vec<Option<String>> =
      capture.iter().map(|name| response.headers().get(*name).and_then(|value| value.to_str().ok()).map(str::to_string)).collect();
    let text = response.body_mut().read_to_string().map_err(|error| format!("Could not read the answer of {service}: {error}"))?;
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if (200..300).contains(&status) {
      return Ok((value, captured));
    }
    let detail = ["message", "error_description", "error"]
      .iter()
      .find_map(|key| match &value[*key] {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object.get("message").and_then(Value::as_str).map(str::to_string),
        _ => None,
      })
      .unwrap_or_default();
    let hint = self.account.provider.permission_hint();
    Err(match status {
      401 | 203 => format!(
        "{service} rejected the token. Check that it is correct, has not expired and has the permissions {} needs ({hint}).",
        ws_product::NAME
      ),
      403 => format!("{service} refused the request: the token lacks a permission. It needs {hint}. {detail}").trim().to_string(),
      404 => format!("{service} could not find it. Check the address, the organization and the token's access. {detail}").trim().to_string(),
      422 => format!("{service} did not accept it: {detail}").trim().to_string(),
      _ => format!("{service} answered with HTTP {status}. {detail}").trim().to_string(),
    })
  }

  fn get(&self, url: &str) -> Result<Value, String> {
    self.request("GET", url, None)
  }
}

/// What a token is allowed to do, as far as the service lets the app find out without changing anything. `None` is unknown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenAccess {
  /// List repositories, clone and fetch.
  pub read: Option<bool>,
  /// Push.
  pub write: Option<bool>,
  /// List and open pull requests.
  pub pull_requests: Option<bool>,
  /// When the token expires, as the service says, if it does.
  pub expires: Option<String>,
}

impl TokenAccess {
  /// The permissions the token certainly lacks, for a warning after connecting.
  pub fn warnings(&self, provider: Provider) -> Vec<String> {
    let hint = provider.permission_hint();
    let mut warnings = Vec::new();
    if self.read == Some(false) {
      warnings.push(format!("The token cannot read repositories. It needs {hint}."));
    }
    if self.write == Some(false) {
      warnings.push(format!("The token cannot push. It needs {hint}."));
    }
    if self.pull_requests == Some(false) {
      warnings.push(format!("The token cannot list or open pull requests. It needs {hint}."));
    }
    warnings
  }

  /// What could not be checked, when nothing is known to be missing: not a problem, only something to know.
  pub fn unchecked(&self, provider: Provider) -> Option<String> {
    let confirmed = self.read == Some(false) || self.write == Some(false) || self.pull_requests == Some(false);
    (!confirmed && (self.write.is_none() || self.pull_requests.is_none())).then(|| {
      format!(
        "{} does not tell {} which permissions the token has, so pushing and pull requests were not checked. If one is refused later, the token needs {}.",
        provider.label(),
        ws_product::NAME,
        provider.permission_hint()
      )
    })
  }
}

/// Finds out what the token may do: GitLab and classic GitHub tokens report their scopes; for fine-grained GitHub tokens and
/// Azure DevOps only reading can be tried without changing anything.
pub fn check_access(connection: &Connection) -> TokenAccess {
  let api = connection.api();
  match connection.account.provider {
    Provider::GitHub => {
      let Ok((_, headers)) =
        connection.request_with("GET", &format!("{api}/user"), None, &["x-oauth-scopes", "github-authentication-token-expiration"])
      else {
        return TokenAccess::default();
      };
      let expires = headers.get(1).cloned().flatten();
      match headers.first().cloned().flatten() {
        // A classic token lists its scopes.
        Some(scopes) => {
          let scopes: Vec<&str> = scopes.split(',').map(str::trim).collect();
          if scopes.contains(&"repo") {
            TokenAccess { read: Some(true), write: Some(true), pull_requests: Some(true), expires }
          } else if scopes.contains(&"public_repo") {
            TokenAccess { read: Some(true), write: None, pull_requests: None, expires }
          } else {
            TokenAccess { read: Some(false), write: Some(false), pull_requests: Some(false), expires }
          }
        }
        // A fine-grained token does not: reading repositories is the one thing that can be tried.
        None => {
          let read = connection.get(&format!("{api}/user/repos?per_page=1")).ok().map(|_| true);
          TokenAccess { read, write: None, pull_requests: None, expires }
        }
      }
    }
    Provider::GitLab => match connection.get(&format!("{api}/personal_access_tokens/self")) {
      Ok(token) => {
        let scopes: Vec<&str> = token["scopes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let expires = token["expires_at"].as_str().map(str::to_string);
        if scopes.contains(&"api") {
          TokenAccess { read: Some(true), write: Some(true), pull_requests: Some(true), expires }
        } else {
          TokenAccess {
            read: Some(scopes.contains(&"read_repository") || scopes.contains(&"read_api")),
            write: Some(scopes.contains(&"write_repository")),
            pull_requests: Some(false),
            expires,
          }
        }
      }
      Err(_) => TokenAccess::default(),
    },
    Provider::AzureDevOps => {
      let read = connection.get(&format!("{api}/_apis/git/repositories?api-version={AZURE_VERSION}")).ok().map(|_| true);
      TokenAccess { read, write: None, pull_requests: None, expires: None }
    }
  }
}

/// Checks the token and returns the user name it belongs to.
pub fn verify(connection: &Connection) -> Result<String, String> {
  let api = connection.api();
  match connection.account.provider {
    Provider::GitHub => {
      let user = connection.get(&format!("{api}/user"))?;
      user["login"].as_str().map(str::to_string).ok_or_else(|| "GitHub did not say who the token belongs to.".into())
    }
    Provider::GitLab => {
      let user = connection.get(&format!("{api}/user"))?;
      user["username"].as_str().map(str::to_string).ok_or_else(|| "GitLab did not say who the token belongs to.".into())
    }
    Provider::AzureDevOps => {
      if connection.account.organization.as_deref().is_none_or(str::is_empty) {
        return Err("Enter the Azure DevOps organization.".into());
      }
      // A token that is wrong gets a sign-in page, which is not JSON.
      let projects = connection.get(&format!("{api}/_apis/projects?api-version={AZURE_VERSION}&$top=1"))?;
      if projects.get("value").is_none() {
        return Err("Azure DevOps rejected the token. Check that it is correct, has not expired and belongs to this organization.".into());
      }
      Ok(connection.account.organization.clone().unwrap_or_default())
    }
  }
}

/// The repositories the account can see, most recently active first, up to a few hundred.
pub fn repositories(connection: &Connection) -> Result<Vec<Repository>, String> {
  let api = connection.api();
  let mut found = Vec::new();
  match connection.account.provider {
    Provider::GitHub | Provider::GitLab => {
      for page in 1..=MAX_PAGES {
        let url = match connection.account.provider {
          Provider::GitHub => format!("{api}/user/repos?per_page={PAGE_SIZE}&sort=pushed&page={page}"),
          _ => format!("{api}/projects?membership=true&per_page={PAGE_SIZE}&order_by=last_activity_at&page={page}"),
        };
        let items = connection.get(&url)?;
        let items = items.as_array().cloned().unwrap_or_default();
        let count = items.len();
        for item in items {
          let text = |key: &str| item[key].as_str().unwrap_or_default().to_string();
          found.push(if connection.account.provider == Provider::GitHub {
            Repository {
              name: text("full_name"),
              clone_url: text("clone_url"),
              private: item["private"].as_bool().unwrap_or(false),
              description: text("description"),
            }
          } else {
            Repository {
              name: text("path_with_namespace"),
              clone_url: text("http_url_to_repo"),
              private: item["visibility"].as_str() == Some("private"),
              description: text("description"),
            }
          });
        }
        if count < PAGE_SIZE {
          break;
        }
      }
    }
    Provider::AzureDevOps => {
      let items = connection.get(&format!("{api}/_apis/git/repositories?api-version={AZURE_VERSION}"))?;
      for item in items["value"].as_array().cloned().unwrap_or_default() {
        let name = item["name"].as_str().unwrap_or_default();
        let project = item["project"]["name"].as_str().unwrap_or_default();
        // The address Azure DevOps gives has the organization as a user name (`https://org@dev.azure.com/…`).
        let remote = item["remoteUrl"].as_str().unwrap_or_default();
        let clone_url = match remote.split_once('@') {
          Some((scheme, rest)) if scheme.contains("://") => format!("{}://{rest}", scheme.split("://").next().unwrap_or("https")),
          _ => remote.to_string(),
        };
        if !item["isDisabled"].as_bool().unwrap_or(false) {
          found.push(Repository {
            name: format!("{project}/{name}"),
            clone_url,
            private: item["project"]["visibility"].as_str() == Some("private"),
            description: String::new(),
          });
        }
      }
      found.sort_by_key(|repository| repository.name.to_lowercase());
    }
  }
  Ok(found)
}

fn parse_request(provider: Provider, item: &Value, repository: &RepoRef, base: &str) -> PullRequest {
  let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
  match provider {
    Provider::GitHub => PullRequest {
      number: item["number"].as_u64().unwrap_or(0),
      title: text(&item["title"]),
      author: text(&item["user"]["login"]),
      url: text(&item["html_url"]),
      draft: item["draft"].as_bool().unwrap_or(false),
      source: text(&item["head"]["ref"]),
      target: text(&item["base"]["ref"]),
    },
    Provider::GitLab => PullRequest {
      number: item["iid"].as_u64().unwrap_or(0),
      title: text(&item["title"]),
      author: text(&item["author"]["username"]),
      url: text(&item["web_url"]),
      draft: item["draft"].as_bool().unwrap_or(false),
      source: text(&item["source_branch"]),
      target: text(&item["target_branch"]),
    },
    Provider::AzureDevOps => {
      let number = item["pullRequestId"].as_u64().unwrap_or(0);
      let branch = |value: &Value| text(value).trim_start_matches("refs/heads/").to_string();
      PullRequest {
        number,
        title: text(&item["title"]),
        author: text(&item["createdBy"]["displayName"]),
        url: format!("{base}/{}/_git/{}/pullrequest/{number}", encode(repository.project.as_deref().unwrap_or_default()), encode(&repository.name)),
        draft: item["isDraft"].as_bool().unwrap_or(false),
        source: branch(&item["sourceRefName"]),
        target: branch(&item["targetRefName"]),
      }
    }
  }
}

fn azure_repository_path(api: &str, repository: &RepoRef) -> String {
  format!("{api}/{}/_apis/git/repositories/{}", encode(repository.project.as_deref().unwrap_or_default()), encode(&repository.name))
}

/// The open pull requests of a repository, newest first.
pub fn pull_requests(connection: &Connection, repository: &RepoRef) -> Result<Vec<PullRequest>, String> {
  let api = connection.api();
  let provider = connection.account.provider;
  let (url, field) = match provider {
    Provider::GitHub => (format!("{api}/repos/{}/{}/pulls?state=open&per_page={PAGE_SIZE}", repository.namespace, repository.name), None),
    Provider::GitLab => {
      (format!("{api}/projects/{}/merge_requests?state=opened&order_by=created_at&per_page={PAGE_SIZE}", encode(&repository.full_name())), None)
    }
    Provider::AzureDevOps => {
      (format!("{}/pullrequests?searchCriteria.status=active&api-version={AZURE_VERSION}", azure_repository_path(&api, repository)), Some("value"))
    }
  };
  let answer = connection.get(&url)?;
  let items = field.map_or(&answer, |field| &answer[field]).as_array().cloned().unwrap_or_default();
  let base = connection.account.base_url();
  let base = format!("{base}/{}", connection.account.organization.as_deref().unwrap_or_default());
  Ok(items.iter().map(|item| parse_request(provider, item, repository, &base)).collect())
}

/// Opens a pull request.
pub fn create_pull_request(connection: &Connection, repository: &RepoRef, new: &NewPullRequest) -> Result<PullRequest, String> {
  let api = connection.api();
  let provider = connection.account.provider;
  let (url, body) = match provider {
    Provider::GitHub => (
      format!("{api}/repos/{}/{}/pulls", repository.namespace, repository.name),
      json!({"title": new.title, "body": new.description, "head": new.source, "base": new.target, "draft": new.draft}),
    ),
    Provider::GitLab => {
      let title = if new.draft && !new.title.starts_with("Draft:") { format!("Draft: {}", new.title) } else { new.title.clone() };
      (
        format!("{api}/projects/{}/merge_requests", encode(&repository.full_name())),
        json!({"title": title, "description": new.description, "source_branch": new.source, "target_branch": new.target}),
      )
    }
    Provider::AzureDevOps => (
      format!("{}/pullrequests?api-version={AZURE_VERSION}", azure_repository_path(&api, repository)),
      json!({
        "title": new.title,
        "description": new.description,
        "sourceRefName": format!("refs/heads/{}", new.source),
        "targetRefName": format!("refs/heads/{}", new.target),
        "isDraft": new.draft,
      }),
    ),
  };
  let item = connection.request("POST", &url, Some(body))?;
  let base = format!("{}/{}", connection.account.base_url(), connection.account.organization.as_deref().unwrap_or_default());
  Ok(parse_request(provider, &item, repository, &base))
}

/// Renames a repository on its service and returns the name it has there afterwards: a service may adjust the name (GitHub turns
/// spaces into hyphens). The token needs the permission to administer the repository.
pub fn rename_repository(connection: &Connection, repository: &RepoRef, new_name: &str) -> Result<String, String> {
  let new_name = new_name.trim();
  if new_name.is_empty() || new_name.contains(['/', '\\']) {
    return Err("Enter a repository name without slashes.".into());
  }
  let api = connection.api();
  let (url, body, key) = match connection.account.provider {
    Provider::GitHub => (format!("{api}/repos/{}/{}", repository.namespace, repository.name), json!({ "name": new_name }), "name"),
    Provider::GitLab => (
      format!("{api}/projects/{}", encode(&repository.full_name())),
      // A GitLab project has a display name and a path, which is what the address uses.
      json!({ "name": new_name, "path": new_name.split_whitespace().collect::<Vec<_>>().join("-") }),
      "path",
    ),
    Provider::AzureDevOps => {
      (format!("{}?api-version={AZURE_VERSION}", azure_repository_path(&api, repository)), json!({ "name": new_name }), "name")
    }
  };
  let method = if connection.account.provider == Provider::GitLab { "PUT" } else { "PATCH" };
  let answer = connection.request(method, &url, Some(body)).map_err(|error| {
    // The generic message names the permissions for reading and pushing; renaming needs more than those.
    if error.contains("the token lacks a permission") {
      let needs = match connection.account.provider {
        Provider::GitHub => "the Administration permission: Read and write (a fine-grained token), or the `repo` scope (a classic token)",
        Provider::GitLab => "the `api` scope and the Owner or Maintainer role on the project",
        Provider::AzureDevOps => "the permission Code: Read, write & manage, and the right to manage the repository",
      };
      format!(
        "{} refused the request: renaming a repository needs {needs}. Change the token's permissions, or connect the account again with a new token in Settings → Accounts.",
        connection.account.provider.label()
      )
    } else {
      error
    }
  })?;
  Ok(answer[key].as_str().filter(|name| !name.is_empty()).unwrap_or(new_name).to_string())
}

/// The name a repository has on its service now, for a repository that was renamed there: GitHub answers a request for the old name
/// with a redirect to the renamed one, which is followed. `Err` when the service does not know the old name any more.
pub fn current_name(connection: &Connection, repository: &RepoRef) -> Result<String, String> {
  let api = connection.api();
  let (url, key) = match connection.account.provider {
    Provider::GitHub => (format!("{api}/repos/{}/{}", repository.namespace, repository.name), "name"),
    Provider::GitLab => (format!("{api}/projects/{}", encode(&repository.full_name())), "path"),
    Provider::AzureDevOps => (format!("{}?api-version={AZURE_VERSION}", azure_repository_path(&api, repository)), "name"),
  };
  let answer = connection.get(&url)?;
  answer[key].as_str().filter(|name| !name.is_empty()).map(str::to_string).ok_or_else(|| "The service did not say the repository's name.".into())
}

/// The branch pull requests go into by default.
pub fn default_branch(connection: &Connection, repository: &RepoRef) -> Result<String, String> {
  let api = connection.api();
  let (url, key) = match connection.account.provider {
    Provider::GitHub => (format!("{api}/repos/{}/{}", repository.namespace, repository.name), "default_branch"),
    Provider::GitLab => (format!("{api}/projects/{}", encode(&repository.full_name())), "default_branch"),
    Provider::AzureDevOps => (format!("{}?api-version={AZURE_VERSION}", azure_repository_path(&api, repository)), "defaultBranch"),
  };
  let answer = connection.get(&url)?;
  answer[key]
    .as_str()
    .map(|branch| branch.trim_start_matches("refs/heads/").to_string())
    .filter(|branch| !branch.is_empty())
    .ok_or_else(|| "The repository has no default branch yet.".into())
}

/// Whether an error message from this crate says the token was refused or lacks a permission (as opposed to a network failure or
/// a missing repository).
pub fn is_permission_problem(message: &str) -> bool {
  message.contains("rejected the token") || message.contains("the token lacks a permission")
}

/// What a token can do in one repository, found by asking the service in ways that change nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PermissionReport {
  /// Push (the service lets the token start a push). `None` when the answer was not clear.
  pub push: Option<bool>,
  /// List and open pull requests.
  pub pull_requests: Option<bool>,
}

impl PermissionReport {
  /// The permissions found missing, each as "what it is for (what to give the token)".
  pub fn missing(&self, provider: Provider) -> Vec<String> {
    let mut missing = Vec::new();
    if self.push == Some(false) {
      missing.push(format!("push ({})", provider.push_permission()));
    }
    if self.pull_requests == Some(false) {
      missing.push(format!("pull requests ({})", provider.pull_request_permission()));
    }
    missing
  }
}

impl Connection {
  /// The status of a request, whatever it is: used to find out what the service allows from how it answers.
  fn status_of(&self, method: &str, url: &str, body: Option<Value>, headers: Vec<(&'static str, String)>) -> Option<u16> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).build().into();
    let sent = match (method, body) {
      ("POST", Some(body)) => {
        let mut request = agent.post(url);
        for (name, value) in headers {
          request = request.header(name, value);
        }
        request.send_json(&body)
      }
      _ => {
        let mut request = agent.get(url);
        for (name, value) in headers {
          request = request.header(name, value);
        }
        request.call()
      }
    };
    sent.ok().map(|response| response.status().as_u16())
  }
}

/// Asks the service what the token may do in `repository`, without changing anything:
/// - push: the advertisement a `git push` starts with (`info/refs?service=git-receive-pack`) is only given to a login that may push;
/// - pull requests: a pull request is requested with an empty body, which the service refuses for a missing field when the token is
///   allowed to open one, and for a missing permission when it is not.
pub fn verify_permissions(connection: &Connection, repository: &RepoRef, clone_url: &str) -> PermissionReport {
  let provider = connection.account.provider;
  let (username, password) = connection.git_login();
  let basic = format!("Basic {}", base64(format!("{username}:{password}").as_bytes()));
  let advertisement = format!("{}/info/refs?service=git-receive-pack", clone_url.trim_end_matches('/'));
  let push = match connection.status_of("GET", &advertisement, None, vec![("User-Agent", ws_product::NAME.into()), ("Authorization", basic)]) {
    Some(200) => Some(true),
    Some(401 | 403 | 404) => Some(false),
    _ => None,
  };
  let api = connection.api();
  let url = match provider {
    Provider::GitHub => format!("{api}/repos/{}/{}/pulls", repository.namespace, repository.name),
    Provider::GitLab => format!("{api}/projects/{}/merge_requests", encode(&repository.full_name())),
    Provider::AzureDevOps => format!("{}/pullrequests?api-version={AZURE_VERSION}", azure_repository_path(&api, repository)),
  };
  // A permitted token gets "unprocessable" (GitHub) or "bad request" (GitLab, Azure DevOps) for the empty body.
  let pull_requests = match connection.status_of("POST", &url, Some(json!({})), connection.headers()) {
    Some(400 | 422) => Some(true),
    Some(401 | 403 | 404) => Some(false),
    _ => None,
  };
  PermissionReport { push, pull_requests }
}
