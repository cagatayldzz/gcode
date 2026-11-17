use crate::{Account, Provider};

/// A repository on a hosting service, as its remote URL names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRef {
  pub provider: Provider,
  /// GitHub: the owner. GitLab: the group path (`group/subgroup`). Azure DevOps: the organization.
  pub namespace: String,
  pub name: String,
  /// Azure DevOps only: the project.
  pub project: Option<String>,
}

impl RepoRef {
  /// The repository a remote URL names, if `account` serves its host. Understands `https://host/a/b(.git)`,
  /// `git@host:a/b(.git)` and `ssh://git@host[:port]/a/b(.git)`, and Azure DevOps's `…/org/project/_git/repo` and
  /// `git@ssh.dev.azure.com:v3/org/project/repo`.
  pub fn for_account(account: &Account, url: &str) -> Option<Self> {
    let (host, segments) = split_remote(url)?;
    let account_host = account.hostname().split(':').next().unwrap_or_default();
    match account.provider {
      Provider::GitHub | Provider::GitLab => {
        if !host.eq_ignore_ascii_case(account_host) || segments.len() < 2 {
          return None;
        }
        let (name, namespace) = segments.split_last()?;
        let namespace = if account.provider == Provider::GitHub { namespace.first()?.clone() } else { namespace.join("/") };
        Some(Self { provider: account.provider, namespace, name: name.clone(), project: None })
      }
      Provider::AzureDevOps => {
        let organization = account.organization.as_deref()?;
        let hosted = host.eq_ignore_ascii_case(account_host)
          || (account_host.eq_ignore_ascii_case("dev.azure.com") && host.eq_ignore_ascii_case("ssh.dev.azure.com"));
        if !hosted {
          return None;
        }
        let (org, project, name) = match segments.as_slice() {
          [v3, org, project, name] if v3 == "v3" => (org, project, name),
          [org, project, git, name] if git == "_git" => (org, project, name),
          // A project and repository of the same name may be shortened to `org/_git/repo`.
          [org, git, name] if git == "_git" => (org, name, name),
          _ => return None,
        };
        org.eq_ignore_ascii_case(organization).then(|| Self {
          provider: Provider::AzureDevOps,
          namespace: org.clone(),
          name: name.clone(),
          project: Some(project.clone()),
        })
      }
    }
  }

  /// `owner/repo`, `group/subgroup/repo` or `project/repo`: how lists and messages name it.
  pub fn full_name(&self) -> String {
    match &self.project {
      Some(project) => format!("{project}/{}", self.name),
      None => format!("{}/{}", self.namespace, self.name),
    }
  }
}

/// The host and the path segments of a remote URL, without a trailing `.git` and any user name or port.
fn split_remote(url: &str) -> Option<(String, Vec<String>)> {
  let url = url.trim();
  let (host, path) = if let Some((_, rest)) = url.split_once("://") {
    let (authority, path) = rest.split_once('/')?;
    let authority = authority.rsplit('@').next()?;
    (authority.split(':').next()?.to_string(), path.to_string())
  } else {
    // scp-like: git@host:path
    let (authority, path) = url.split_once(':')?;
    (authority.rsplit('@').next()?.to_string(), path.to_string())
  };
  let path = path.split(['?', '#']).next()?.trim_matches('/');
  let path = path.strip_suffix(".git").unwrap_or(path);
  let segments: Vec<String> = path.split('/').filter(|segment| !segment.is_empty()).map(str::to_string).collect();
  (!host.is_empty() && !segments.is_empty()).then_some((host, segments))
}

/// The host of a remote URL (`github.com`), for choosing which account's credentials Git gets.
pub fn remote_host(url: &str) -> Option<String> {
  split_remote(url).map(|(host, _)| host)
}
