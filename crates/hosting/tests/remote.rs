use ws_hosting::{remote_host, Account, Provider, RepoRef};

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

mod web {
  use ws_hosting::{web_repository, Account, Provider};

  fn url(remote: &str, accounts: &[Account]) -> Option<(String, String)> {
    web_repository(remote, accounts).map(|found| (found.service, found.url))
  }

  fn pair(service: &str, url: &str) -> Option<(String, String)> {
    Some((service.to_string(), url.to_string()))
  }

  #[test]
  fn well_known_hosts_are_told_from_their_names_in_every_remote_form() {
    for remote in
      ["https://github.com/octo/app.git", "git@github.com:octo/app.git", "ssh://git@github.com/octo/app.git", "https://me@github.com/octo/app/"]
    {
      assert_eq!(url(remote, &[]), pair("GitHub", "https://github.com/octo/app"), "{remote}");
    }
    assert_eq!(url("git@gitlab.com:group/sub/app.git", &[]), pair("GitLab", "https://gitlab.com/group/sub/app"));
    assert_eq!(url("https://gitlab.example.org/team/app.git", &[]), pair("GitLab", "https://gitlab.example.org/team/app"));
  }

  #[test]
  fn azure_devops_remotes_become_git_pages() {
    let page = "https://dev.azure.com/contoso/Shop/_git/shop";
    assert_eq!(url("https://contoso@dev.azure.com/contoso/Shop/_git/shop", &[]), pair("Azure DevOps", page));
    assert_eq!(url("git@ssh.dev.azure.com:v3/contoso/Shop/shop", &[]), pair("Azure DevOps", page));
    assert_eq!(url("https://dev.azure.com/contoso/_git/shop", &[]), pair("Azure DevOps", "https://dev.azure.com/contoso/shop/_git/shop"));
    assert_eq!(url("https://dev.azure.com/contoso", &[]), None, "no repository is named");
  }

  #[test]
  fn a_connected_account_says_what_its_host_is() {
    let enterprise = Account { provider: Provider::GitHub, host: "git.example.com".into(), organization: None, username: "me".into(), issue: None };
    assert_eq!(url("git@git.example.com:team/app.git", &[enterprise]), pair("GitHub", "https://git.example.com/team/app"));
    let own_gitlab = Account { provider: Provider::GitLab, host: "code.example.com".into(), organization: None, username: "me".into(), issue: None };
    assert_eq!(url("https://code.example.com/a/b.git", &[own_gitlab]), pair("GitLab", "https://code.example.com/a/b"));
  }

  #[test]
  fn an_unknown_host_is_named_by_itself_and_local_paths_have_no_page() {
    assert_eq!(url("https://git.sr.ht/~me/app", &[]), pair("git.sr.ht", "https://git.sr.ht/~me/app"));
    assert_eq!(url("/Users/me/project.git", &[]), None);
    assert_eq!(url("https://github.com/lonely", &[]), None);
  }
}

#[test]
fn a_remote_url_is_renamed_in_every_form_it_can_take() {
  use ws_hosting::renamed_url;

  assert_eq!(renamed_url("https://github.com/octo/app.git", "app", "tool").as_deref(), Some("https://github.com/octo/tool.git"));
  assert_eq!(renamed_url("https://github.com/octo/app", "APP", "tool").as_deref(), Some("https://github.com/octo/tool"));
  assert_eq!(renamed_url("git@github.com:octo/app.git", "app", "tool").as_deref(), Some("git@github.com:octo/tool.git"));
  assert_eq!(renamed_url(" https://gitlab.com/g/s/app.git/ ", "app", "tool").as_deref(), Some("https://gitlab.com/g/s/tool.git/"));
  assert_eq!(renamed_url("https://dev.azure.com/org/Proj/_git/app", "app", "tool").as_deref(), Some("https://dev.azure.com/org/Proj/_git/tool"));
  assert_eq!(renamed_url("https://github.com/octo/other.git", "app", "tool"), None, "another repository is left alone");
}
