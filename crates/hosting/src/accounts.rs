use serde_json::{json, Value};
use std::{
  collections::HashMap,
  fs, io,
  path::Path,
  process::{Command, Stdio},
  sync::Mutex,
};

/// A service that hosts Git repositories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
  GitHub,
  GitLab,
  AzureDevOps,
}

impl Provider {
  pub const ALL: [Provider; 3] = [Provider::GitHub, Provider::GitLab, Provider::AzureDevOps];

  pub fn id(self) -> &'static str {
    match self {
      Self::GitHub => "github",
      Self::GitLab => "gitlab",
      Self::AzureDevOps => "azure",
    }
  }

  pub fn from_id(id: &str) -> Option<Self> {
    Self::ALL.into_iter().find(|provider| provider.id() == id)
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::GitHub => "GitHub",
      Self::GitLab => "GitLab",
      Self::AzureDevOps => "Azure DevOps",
    }
  }

  /// The service's own address, used until the user names a self-hosted one.
  pub fn default_host(self) -> &'static str {
    match self {
      Self::GitHub => "github.com",
      Self::GitLab => "gitlab.com",
      Self::AzureDevOps => "dev.azure.com",
    }
  }

  /// How to create the token: where, what it must be allowed to do, and what else to watch for.
  pub fn token_guide(self) -> TokenGuide {
    match self {
      Self::GitHub => TokenGuide {
        place: "GitHub → Settings → Developer settings → Personal access tokens (fine-grained tokens are preferred).",
        permissions: &[
          ("Contents: Read and write", "clone, fetch, pull and push"),
          ("Pull requests: Read and write", "list and open pull requests"),
          ("Metadata: Read-only", "added automatically"),
        ],
        note: "Choose the repositories the token may use (All repositories, or only the ones you work on). A classic token needs the `repo` scope instead.",
      },
      Self::GitLab => TokenGuide {
        place: "GitLab → Preferences → Access tokens → Add new token.",
        permissions: &[("api", "list repositories, clone, push, and list and open merge requests")],
        note: "With only `read_repository` and `write_repository` Git works, but merge requests do not.",
      },
      Self::AzureDevOps => TokenGuide {
        place: "Azure DevOps → User settings → Personal access tokens → New Token.",
        permissions: &[("Code: Read & write", "list repositories, clone, push, and list and open pull requests")],
        note: "Under Organization choose your organization, not \"All accessible organizations\": tokens for all organizations stop working on December 1, 2026. Pick Custom defined to see the Code scope.",
      },
    }
  }

  /// The permission pushing needs, as the service names it.
  pub fn push_permission(self) -> &'static str {
    match self {
      Self::GitHub => "Contents: Read and write (or the `repo` scope)",
      Self::GitLab => "the `write_repository` or `api` scope",
      Self::AzureDevOps => "Code: Read & write",
    }
  }

  /// The permission listing and opening pull requests needs, as the service names it.
  pub fn pull_request_permission(self) -> &'static str {
    match self {
      Self::GitHub => "Pull requests: Read and write (or the `repo` scope)",
      Self::GitLab => "the `api` scope",
      Self::AzureDevOps => "Code: Read & write",
    }
  }

  /// The permission a refused request most likely lacks, for an error message.
  pub fn permission_hint(self) -> &'static str {
    match self {
      Self::GitHub => "Contents and Pull requests: Read and write, or the `repo` scope",
      Self::GitLab => "the `api` scope",
      Self::AzureDevOps => "Code: Read & write",
    }
  }
}

/// How to create an access token for a service.
#[derive(Clone, Copy, Debug)]
pub struct TokenGuide {
  /// Where in the service's settings tokens are created.
  pub place: &'static str,
  /// `(permission, what Vekro uses it for)`, named as the service names them.
  pub permissions: &'static [(&'static str, &'static str)],
  pub note: &'static str,
}

/// A connected account. The token is not part of it: see [`TokenStore`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
  pub provider: Provider,
  /// The host, such as `github.com` or `gitlab.example.com`. A full URL (`http://localhost:8080`) is used as it is.
  pub host: String,
  /// Azure DevOps only: the organization (`dev.azure.com/<organization>`).
  pub organization: Option<String>,
  /// Who the token belongs to, as the service reported when it was verified.
  pub username: String,
  /// What is wrong with the token's permissions, while something is: set when a push or a pull request is refused (or a check
  /// finds a permission missing), cleared when a check finds it fixed. A new token (connecting again) starts without one.
  pub issue: Option<String>,
}

impl Account {
  /// The address without a scheme: `github.com` for `https://github.com`.
  pub fn hostname(&self) -> &str {
    let host = self.host.trim().trim_end_matches('/');
    let host = host.split_once("://").map_or(host, |(_, rest)| rest);
    host.split('/').next().unwrap_or(host)
  }

  /// The address with a scheme, without a trailing slash.
  pub fn base_url(&self) -> String {
    let host = self.host.trim().trim_end_matches('/');
    if host.contains("://") {
      host.to_string()
    } else {
      format!("https://{host}")
    }
  }

  /// The page of the service where this account's tokens are listed and edited, to open in a browser. (GitHub does not tell Vekro
  /// which token is in use, so it is the list, not one token.)
  pub fn tokens_url(&self) -> String {
    let base = self.base_url();
    match self.provider {
      Provider::GitHub => format!("{base}/settings/personal-access-tokens"),
      Provider::GitLab => format!("{base}/-/user_settings/personal_access_tokens"),
      Provider::AzureDevOps => format!("{base}/{}/_usersSettings/tokens", self.organization.as_deref().unwrap_or_default()),
    }
  }

  /// What names this account in the token store and in lists.
  pub fn key(&self) -> String {
    match &self.organization {
      Some(organization) => format!("{}:{}:{}", self.provider.id(), self.hostname(), organization.to_lowercase()),
      None => format!("{}:{}:{}", self.provider.id(), self.hostname(), self.username.to_lowercase()),
    }
  }

  /// The name shown in lists: `octocat on github.com`, or `my-org on dev.azure.com`.
  pub fn label(&self) -> String {
    format!("{} on {}", self.organization.as_deref().unwrap_or(&self.username), self.hostname())
  }
}

/// Where tokens are kept. Only the account list goes to a file.
pub trait TokenStore: Send + Sync {
  fn get(&self, key: &str) -> Option<String>;
  fn set(&self, key: &str, token: &str) -> Result<(), String>;
  fn delete(&self, key: &str);
}

/// The macOS Keychain, through the `security` program (generic passwords of the service "Vekro").
///
/// `security` takes the password as an argument, so it is visible to other processes of the same user for the moment the
/// command runs; it is never written to a file or a log.
///
/// A namespace (a profile's) prefixes every key, so the same account connected in two profiles has two tokens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeychainStore {
  namespace: Option<String>,
}

const SERVICE: &str = "Vekro";

impl KeychainStore {
  /// A store whose entries belong to `namespace`; `None` is the unprefixed set that existed before namespaces.
  pub fn new(namespace: Option<String>) -> Self {
    Self { namespace }
  }

  fn entry(&self, key: &str) -> String {
    match &self.namespace {
      Some(namespace) => format!("{namespace}:{key}"),
      None => key.to_string(),
    }
  }
}

impl TokenStore for KeychainStore {
  fn get(&self, key: &str) -> Option<String> {
    let key = &self.entry(key);
    let output = Command::new("security").args(["find-generic-password", "-s", SERVICE, "-a", key, "-w"]).stderr(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string()).filter(|token| !token.is_empty())
  }

  fn set(&self, key: &str, token: &str) -> Result<(), String> {
    let key = &self.entry(key);
    let output = Command::new("security")
      .args(["add-generic-password", "-U", "-s", SERVICE, "-a", key, "-w", token])
      .output()
      .map_err(|error| format!("Could not run the security program: {error}"))?;
    if output.status.success() {
      Ok(())
    } else {
      Err(format!("Could not save the token in the Keychain: {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
  }

  fn delete(&self, key: &str) {
    let key = &self.entry(key);
    let _ = Command::new("security").args(["delete-generic-password", "-s", SERVICE, "-a", key]).stdout(Stdio::null()).stderr(Stdio::null()).status();
  }
}

/// Tokens in memory, for tests and for runs that must not touch the Keychain.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl TokenStore for MemoryStore {
  fn get(&self, key: &str) -> Option<String> {
    self.0.lock().ok()?.get(key).cloned()
  }

  fn set(&self, key: &str, token: &str) -> Result<(), String> {
    self.0.lock().map_err(|_| "The token store is unavailable.".to_string())?.insert(key.to_string(), token.to_string());
    Ok(())
  }

  fn delete(&self, key: &str) {
    if let Ok(mut tokens) = self.0.lock() {
      tokens.remove(key);
    }
  }
}

/// An account with its token: what a request to the service needs.
#[derive(Clone, PartialEq, Eq)]
pub struct Connection {
  pub account: Account,
  pub token: String,
}

impl std::fmt::Debug for Connection {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("Connection").field("account", &self.account).field("token", &"…").finish()
  }
}

impl Connection {
  /// The user name and password Git over HTTPS takes for this account: the token is the password. GitHub and Azure DevOps
  /// accept any non-empty user name with a token, GitLab wants `oauth2`.
  pub fn git_login(&self) -> (String, String) {
    let username = match self.account.provider {
      Provider::GitLab => "oauth2".to_string(),
      _ if self.account.username.is_empty() => "vekro".to_string(),
      _ => self.account.username.clone(),
    };
    (username, self.token.clone())
  }
}

/// The connected accounts (`accounts.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Accounts {
  pub items: Vec<Account>,
  /// The Keychain namespace of the profile these accounts belong to (see [`KeychainStore`]).
  pub namespace: Option<String>,
}

impl Accounts {
  /// The Keychain that holds these accounts' tokens.
  pub fn keychain(&self) -> KeychainStore {
    KeychainStore::new(self.namespace.clone())
  }

  /// Reads the list; a missing or damaged file gives an empty one.
  pub fn load(path: &Path) -> Self {
    Self::load_in(path, None)
  }

  /// Reads the list of the profile with this Keychain namespace.
  pub fn load_in(path: &Path, namespace: Option<String>) -> Self {
    let Some(Value::Object(root)) = fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()) else {
      return Self { items: Vec::new(), namespace };
    };
    let items = root
      .get("accounts")
      .and_then(Value::as_array)
      .into_iter()
      .flatten()
      .filter_map(|item| {
        let text = |key: &str| item.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(Account {
          provider: Provider::from_id(&text("provider")?)?,
          host: text("host")?,
          organization: text("organization").filter(|organization| !organization.is_empty()),
          username: text("username").unwrap_or_default(),
          issue: text("issue").filter(|issue| !issue.is_empty()),
        })
      })
      .collect();
    Self { items, namespace }
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let value = json!({
      "accounts": self.items.iter().map(|account| json!({
        "provider": account.provider.id(),
        "host": account.host,
        "organization": account.organization,
        "username": account.username,
        "issue": account.issue,
      })).collect::<Vec<_>>(),
    });
    fs::write(path, serde_json::to_string_pretty(&value)?)
  }

  /// Adds an account, or replaces the one with the same key (a new token for the same account).
  pub fn add(&mut self, account: Account) {
    match self.items.iter().position(|existing| existing.key() == account.key()) {
      Some(index) => self.items[index] = account,
      None => self.items.push(account),
    }
  }

  /// Removes an account and its token.
  pub fn remove(&mut self, key: &str, store: &dyn TokenStore) {
    self.items.retain(|account| account.key() != key);
    store.delete(key);
  }

  /// The account and token for the host of a remote URL (`https://github.com/a/b.git`, `git@gitlab.com:a/b.git`), with the
  /// repository it names. `None` when no connected account serves that host or the account has no token.
  pub fn for_remote(&self, url: &str, store: &dyn TokenStore) -> Option<(Connection, crate::RepoRef)> {
    self.items.iter().find_map(|account| {
      let repository = crate::RepoRef::for_account(account, url)?;
      let token = store.get(&account.key())?;
      Some((Connection { account: account.clone(), token }, repository))
    })
  }
}
