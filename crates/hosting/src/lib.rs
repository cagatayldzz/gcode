//! Connections to the services that host Git repositories: GitHub, GitLab and Azure DevOps. A connection is an account
//! (where, as whom) and a personal access token kept outside the account file. With it the app lists the repositories to clone,
//! lists and opens pull requests, and gives `git` the credentials for the same host. Never depends on the interface.

mod accounts;
mod api;
mod remote;

pub use accounts::{Account, Accounts, Connection, KeychainStore, MemoryStore, Provider, TokenGuide, TokenStore};
pub use api::{
  check_access, create_pull_request, default_branch, is_permission_problem, pull_requests, rename_repository, repositories, verify,
  verify_permissions, NewPullRequest, PermissionReport, PullRequest, Repository, TokenAccess,
};
pub use remote::{remote_host, renamed_url, web_repository, RepoRef, WebRepository};
