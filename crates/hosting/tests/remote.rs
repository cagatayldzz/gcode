use vekro_hosting::{remote_host, Account, Provider, RepoRef};

fn account(provider: Provider, host: &str, organization: Option<&str>) -> Account {
  Account { provider, host: host.into(), organization: organization.map(str::to_string), username: "me".into(), issue: None }
}

#[test]
fn github_remotes_in_every_form_name_the_same_repository() {
  let github = account(Provider::GitHub, "github.com", None);
  for url in [
    "https://github.com/octo/app.git",
    "https://me@github.com/octo/app",
    "git@github.com:octo/app.git",
    "ssh://git@github.com/octo/app.git",
    "https://github.com/octo/app/",
  ] {
    let found = RepoRef::for_account(&github, url).unwrap_or_else(|| panic!("{url}"));
    assert_eq!((found.namespace.as_str(), found.name.as_str(), found.project), ("octo", "app", None), "{url}");
  }
}

#[test]
fn an_account_only_serves_its_own_host() {
  let github = account(Provider::GitHub, "github.com", None);
  assert_eq!(RepoRef::for_account(&github, "https://gitlab.com/octo/app.git"), None);
  assert_eq!(RepoRef::for_account(&github, "https://github.com/lonely"), None);
  assert_eq!(RepoRef::for_account(&github, "not a url"), None);
  let enterprise = account(Provider::GitHub, "https://git.example.com", None);
  assert!(RepoRef::for_account(&enterprise, "git@git.example.com:team/app.git").is_some());
}

#[test]
fn gitlab_keeps_the_whole_group_path() {
  let gitlab = account(Provider::GitLab, "gitlab.com", None);
  let found = RepoRef::for_account(&gitlab, "https://gitlab.com/group/sub/app.git").unwrap();
  assert_eq!((found.namespace.as_str(), found.name.as_str()), ("group/sub", "app"));
  assert_eq!(found.full_name(), "group/sub/app");
}

#[test]
fn azure_devops_remotes_name_the_organization_project_and_repository() {
  let azure = account(Provider::AzureDevOps, "dev.azure.com", Some("Contoso"));
  for url in ["https://contoso@dev.azure.com/contoso/Web%20Shop/_git/shop", "git@ssh.dev.azure.com:v3/contoso/Web%20Shop/shop"] {
    let found = RepoRef::for_account(&azure, url).unwrap_or_else(|| panic!("{url}"));
    assert_eq!((found.namespace.as_str(), found.name.as_str(), found.project.as_deref()), ("contoso", "shop", Some("Web%20Shop")), "{url}");
  }
  assert_eq!(RepoRef::for_account(&azure, "https://dev.azure.com/other/p/_git/r"), None, "another organization");
  let short = RepoRef::for_account(&azure, "https://contoso@dev.azure.com/contoso/_git/shop").unwrap();
  assert_eq!((short.project.as_deref(), short.name.as_str()), (Some("shop"), "shop"));
}

#[test]
fn the_host_of_a_remote_is_found_for_every_form() {
  assert_eq!(remote_host("https://user@github.com:443/a/b.git").as_deref(), Some("github.com"));
  assert_eq!(remote_host("git@gitlab.example.com:a/b.git").as_deref(), Some("gitlab.example.com"));
  assert_eq!(remote_host("/local/path"), None);
}

#[test]
fn account_addresses_work_with_and_without_a_scheme() {
  let plain = account(Provider::GitLab, "gitlab.example.com/", None);
  assert_eq!((plain.hostname(), plain.base_url().as_str()), ("gitlab.example.com", "https://gitlab.example.com"));
  let local = account(Provider::GitLab, "http://127.0.0.1:8080", None);
  assert_eq!((local.hostname(), local.base_url().as_str()), ("127.0.0.1:8080", "http://127.0.0.1:8080"));
}
