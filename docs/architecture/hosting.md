# Hosting & accounts

`vekro-hosting` knows three services — **GitHub**, **GitLab** and **Azure DevOps** — and nothing about the interface.

## Types

```rust
pub struct Account { /* service, host, organization, user name, issue: Option<String> */ }
pub struct Accounts { items: Vec<Account> }           // accounts.json
pub trait TokenStore: Send + Sync { /* get, set, delete */ }
pub struct KeychainStore { /* the macOS Keychain through `security` */ }
pub struct MemoryStore(/* tests */);

pub fn verify(connection: &Connection) -> Result<String, String>;
pub fn repositories(connection: &Connection) -> Result<Vec<Repository>, String>;
pub fn pull_requests(connection: &Connection, repository: &RepoRef) -> Result<Vec<PullRequest>, String>;
pub fn create_pull_request(connection: &Connection, repository: &RepoRef, new: &NewPullRequest) -> Result<PullRequest, String>;
pub fn verify_permissions(connection: &Connection, repository: &RepoRef, clone_url: &str) -> PermissionReport;
```

A `Connection` is an `Account` plus its token. `RepoRef` says which repository a remote URL names; `web_repository(url, accounts)` finds the
page for **View on …**, using an account to recognize a self-hosted host.

## Where secrets live

| What | Where |
| --- | --- |
| The account (service, host, organization, user name) | `accounts.json` in the profile |
| The personal access token | the **macOS Keychain** (`security` program) — never a file, never a log |
| A non-default profile's tokens | Keychain entries prefixed with the profile id (`KeychainStore::new(namespace)`, `Accounts::keychain`) |

Tokens reach the Keychain through the **standard input of `security -i`**, never through a command line. Everything that touches the network or
the Keychain runs on a background thread.

## Credentials for Git

For `clone`, `push`, `fetch` and `pull` of an HTTPS remote, `connections::credentials_for_repository` finds the account for the remote's host
and gives the `git` program the user name and token **through environment variables** read by a one-command credential helper
(`vekro_git::CREDENTIAL_HELPER`):

```text
 VEKRO_GIT_USERNAME ─┐
 VEKRO_GIT_PASSWORD ─┴─►  credential helper (one shell command)  ──►  git
                          so they appear in NO command line, file or log
```

If the server refuses the token (HTTP 401/403) the command runs once more with the user's own Git login, and a message names both when both
fail. SSH remotes keep the user's keys. Self-hosted GitHub Enterprise and GitLab are reached by naming the host; Azure DevOps uses the
organization. `Credentials` redacts the token from `Debug` output.

## A refused token marks its account

```text
 push / pull / fetch error contains  TOKEN_REFUSED     ┐
 pull request refused  (PullRequestEvent::Refused)      ├─►  Account.issue = "what is wrong"
 connecting finds a permission missing                  ┘         │
                                                                    ▼
                    warning badge on the Settings button  ──► Settings → Accounts
                                                                    │ "Check again"
                    Workspace::recheck_account ─► verify_permissions (background)
                       asks info/refs?service=git-receive-pack  and creates an EMPTY pull request —
                       neither changes anything ─► clears the issue when fixed
```

## What each service verifies

| Service | Confirms on connect |
| --- | --- |
| GitLab, classic GitHub | the token's **scopes**, so a missing one is named |
| Fine-grained GitHub, Azure DevOps | only **reading** can be tried; the interface says pushing and pull requests were not checked |

## Pull requests in the interface

`Workspace::load_pull_requests` lists open PRs for the branch menu (state `PullRequests::{Idle, Loading, Failed, Loaded}`);
`PullRequestForm` creates one, and the repository's default branch comes from `default_branch`. The branch must be published first.
