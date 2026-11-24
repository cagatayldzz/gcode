# Hosting & accounts

`ws-hosting` knows three services — **GitHub**, **GitLab** and **Azure DevOps** — and nothing about the interface.

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
pub fn rename_repository(connection: &Connection, repository: &RepoRef, new_name: &str) -> Result<String, String>;  // the name it has afterwards
pub fn current_name(connection: &Connection, repository: &RepoRef) -> Result<String, String>;                        // the name it has now (follows GitHub's redirect)
pub fn renamed_url(url: &str, old: &str, new: &str) -> Option<String>;                                             // the same remote form, new name
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

## Asking the services less (`traffic`)

Every request goes through `crates/hosting/src/traffic.rs`, so the app does not ask twice and does not run into a service's rate limit:

| What | How |
| --- | --- |
| One agent | connections are reused, not opened per request |
| Reads are kept | an answer is used for 60 s; after that it is asked again with its `ETag` (GitHub does not count a `304` against its limit) |
| Identical reads at once | the second waits for the first and takes its answer (opening a project asks for the repository's name from several places) |
| Changes | a `POST`/`PATCH`/`PUT` makes what was read for that account stale |
| Refresh by the user | `Connection::expire()` (the pull request **Refresh** button, **Check again** on an account) |
| Rate limit | `429`, `Retry-After`, or `X-RateLimit-Remaining: 0` (GitLab: `RateLimit-…`) leave that host alone until its reset; the error says when, and the last answer is shown meanwhile. A `403` for a missing permission is not taken for a limit |

Keys include the token (a fingerprint, in memory only), so a replaced token never gets another's answers. The update check of `ws-updater` keeps its
answer for 10 minutes, and 15 minutes after GitHub refused it for its rate limit. Each request, with whether it was sent, kept or revalidated, is in the activity log.

## Credentials for Git

For `clone`, `push`, `fetch` and `pull` of an HTTPS remote, `connections::credentials_for_repository` finds the account for the remote's host
and gives the `git` program the user name and token **through environment variables** read by a one-command credential helper
(`ws_git::CREDENTIAL_HELPER`):

```text
 WS_GIT_USERNAME ─┐
 WS_GIT_PASSWORD ─┴─►  credential helper (one shell command)  ──►  git
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

## Renaming a repository

`rename_repository` uses `PATCH /repos/{owner}/{repo}` (GitHub), `PUT /projects/{id}` with a name and a path (GitLab) and `PATCH …/_apis/git/repositories/{repo}`
(Azure DevOps), and returns the name the service ended up with. `renamed_url` then rewrites the project's remote address in whatever form it
has (`https`, `ssh://`, `git@host:…`, with or without `.git`). The interface renames the repository **before** touching anything local, and asks
first: see [Projects → Renaming the repository with the project](../guide/projects.md#renaming-the-repository-with-the-project).

A repository renamed on the service itself (in the browser) is found by `current_name`: `Workspace::check_remote_rename` asks for the open project
when it starts, when the project changes and when its settings open. A different answer is kept in `renamed_remotes`, puts a warning badge on the
**Project settings** button and a notice with **Sync changes** at the top of General. Syncing sets the remote with `renamed_url` and renames the
display name when it was the old repository name; the folder on disk is left alone.

## Pull requests in the interface

`Workspace::load_pull_requests` lists open PRs for the branch menu (state `PullRequests::{Idle, Loading, Failed, Loaded}`);
`PullRequestForm` creates one, and the repository's default branch comes from `default_branch`. The branch must be published first.
