use std::fs;
use vekro_hosting::{Account, Accounts, MemoryStore, Provider, TokenStore};

fn github(username: &str) -> Account {
  Account { provider: Provider::GitHub, host: "github.com".into(), organization: None, username: username.into(), issue: None }
}

#[test]
fn accounts_survive_a_save_and_load_and_never_hold_a_token() {
  let dir = std::env::temp_dir().join(format!("vekro-hosting-accounts-{}", std::process::id()));
  fs::create_dir_all(&dir).unwrap();
  let file = dir.join("accounts.json");
  let mut accounts = Accounts::default();
  accounts.add(github("octo"));
  accounts.add(Account {
    provider: Provider::AzureDevOps,
    host: "dev.azure.com".into(),
    organization: Some("contoso".into()),
    username: "contoso".into(),
    issue: Some("Pushing was refused.".into()),
  });
  accounts.save(&file).unwrap();
  assert_eq!(Accounts::load(&file), accounts);
  assert!(!fs::read_to_string(&file).unwrap().to_lowercase().contains("token"));
  assert_eq!(Accounts::load(&dir.join("missing.json")), Accounts::default());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn adding_the_same_account_again_replaces_it() {
  let mut accounts = Accounts::default();
  accounts.add(github("octo"));
  accounts.add(github("Octo"));
  assert_eq!(accounts.items.len(), 1);
  assert_eq!(accounts.items[0].username, "Octo");
}

#[test]
fn removing_an_account_deletes_its_token() {
  let store = MemoryStore::default();
  let mut accounts = Accounts::default();
  let account = github("octo");
  store.set(&account.key(), "secret").unwrap();
  accounts.add(account.clone());
  accounts.remove(&account.key(), &store);
  assert!(accounts.items.is_empty());
  assert_eq!(store.get(&account.key()), None);
}

#[test]
fn a_remote_finds_the_account_and_token_for_its_host() {
  let store = MemoryStore::default();
  let mut accounts = Accounts::default();
  let account = github("octo");
  accounts.add(account.clone());
  assert!(accounts.for_remote("https://github.com/octo/app.git", &store).is_none(), "an account without a token is not used");
  store.set(&account.key(), "secret").unwrap();
  let (connection, repository) = accounts.for_remote("git@github.com:octo/app.git", &store).unwrap();
  assert_eq!((connection.token.as_str(), repository.name.as_str()), ("secret", "app"));
  assert!(accounts.for_remote("https://gitlab.com/octo/app.git", &store).is_none());
  assert!(!format!("{connection:?}").contains("secret"), "the token stays out of debug output");
}

#[test]
fn git_logins_use_the_token_as_the_password() {
  let connection = |provider, username: &str| vekro_hosting::Connection {
    account: Account { provider, host: "h".into(), organization: None, username: username.into(), issue: None },
    token: "tok".into(),
  };
  assert_eq!(connection(Provider::GitHub, "octo").git_login(), ("octo".to_string(), "tok".to_string()));
  assert_eq!(connection(Provider::GitLab, "gl").git_login(), ("oauth2".to_string(), "tok".to_string()));
  assert_eq!(connection(Provider::AzureDevOps, "").git_login().0, "vekro");
}

#[test]
fn every_service_tells_where_to_create_the_token_and_what_it_needs() {
  for provider in Provider::ALL {
    let guide = provider.token_guide();
    assert!(guide.place.contains("→") && !guide.permissions.is_empty() && !guide.note.is_empty(), "{provider:?}");
    assert!(!provider.permission_hint().is_empty());
  }
  let azure = Provider::AzureDevOps.token_guide();
  assert!(azure.permissions.iter().any(|(name, _)| *name == "Code: Read & write"));
  assert!(azure.note.contains("All accessible organizations") && azure.note.contains("December 1, 2026"));
  assert!(Provider::GitHub.token_guide().permissions.iter().any(|(name, _)| name.starts_with("Pull requests")));
  assert!(Provider::GitLab.token_guide().permissions.iter().any(|(name, _)| *name == "api"));
}

#[test]
fn each_service_has_a_page_where_the_token_is_edited() {
  let account = |provider, host: &str, organization: Option<&str>| Account {
    provider,
    host: host.into(),
    organization: organization.map(str::to_string),
    username: "me".into(),
    issue: None,
  };
  assert_eq!(account(Provider::GitHub, "github.com", None).tokens_url(), "https://github.com/settings/personal-access-tokens");
  assert_eq!(account(Provider::GitHub, "git.example.com", None).tokens_url(), "https://git.example.com/settings/personal-access-tokens");
  assert_eq!(account(Provider::GitLab, "gitlab.com", None).tokens_url(), "https://gitlab.com/-/user_settings/personal_access_tokens");
  assert_eq!(account(Provider::AzureDevOps, "dev.azure.com", Some("contoso")).tokens_url(), "https://dev.azure.com/contoso/_usersSettings/tokens");
}
