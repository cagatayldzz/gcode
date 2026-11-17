//! Connections to the services that host Git repositories: GitHub, GitLab and Azure DevOps. A connection is an account
//! (where, as whom) and a personal access token kept outside the account file. With it Vekro lists the repositories to clone,
//! lists and opens pull requests, and gives `git` the credentials for the same host. Never depends on the interface.

mod accounts;
mod api;
mod remote;

pub use accounts::{Account, Accounts, Connection, KeychainStore, MemoryStore, Provider, TokenGuide, TokenStore};
pub use api::{
  check_access, create_pull_request, default_branch, pull_requests, repositories, verify, NewPullRequest, PullRequest, Repository, TokenAccess,
};
pub use remote::{remote_host, RepoRef};
