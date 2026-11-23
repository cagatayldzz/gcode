# Git

`vekro-git` is the one crate that knows how to talk to a repository. It never depends on the interface, creates its test repositories in
temporary directories, and exposes plain functions that return `Result<_, String>` — the string is a sentence meant for the user.

## Two engines, one rule

```text
              reading state, local edits                 talking to a remote, merging
             ┌────────────────────────────┐             ┌──────────────────────────────┐
             │ libgit2  (git2 crate)       │             │ the `git` program             │
             │  summary · history · diff   │             │  clone · fetch · pull · push  │
             │  commit · branches · tags   │             │  merge · revert · cherry-pick │
             │  create repository          │             │  stash                        │
             └────────────────────────────┘             └──────────────────────────────┘
                  fast, in-process                          the user's own credentials apply
```

**Rule:** `git2` is used to read state and create repositories; the `git` program is used for `clone` and everything that needs the user's
credentials or Git's full merge machinery. And **no Git command that changes the user's repository runs from the interface without an
explicit user action.**

## What the crate exposes

| Area | Functions |
| --- | --- |
| **State** | `summary`, `quick_status`, `head_summary`, `head_message`, `IgnoreRules` |
| **Diffs** | `file_diff`, `all_changes_diff`, `diff_files`, `file_versions` (for image previews), `commit_file_diff` |
| **History** | `history`, `file_history`, `commit_detail`, `recent_authors` |
| **Commit** | `commit`, `amend_commit`, `undo_commit`, `discard_changes`, `create_tag`, `add_to_gitignore` |
| **Branches** | `branch_entries`, `create_branch`, `switch_branch`, `switch_branch_with`, `delete_branch`, `delete_remote_branch`, `rename_branch`, `branch_from_commit`, `upstream_remote` |
| **Stash** | `stash_changes`, `has_stash`, `restore_stash`, `discard_stash` |
| **Remote** | `fetch`, `pull`, `push`, `clone`, `remote_url`, `set_remote_url` |
| **Compare & merge** | `compare_choices`, `compare`, `start_merge`, `read_merge`, `MergeState::{resolve, undo, finish}` |
| **History actions** | `revert_commit`, `cherry_pick` |
| **Projects** | `create_project`, `repository_name`, `find_repositories` |

### The shape of the state

```rust
pub struct Summary {
  pub branch: String,          // or a short commit id when HEAD is detached
  pub changes: Vec<Change>,    // working tree + index, untracked included, sorted by path
  pub sync: Option<Sync>,      // None when the repository has no remote
  pub merging: bool,
}

pub struct Sync {
  pub remote: String,          // the followed remote, else origin
  pub url: String,
  pub upstream: bool,          // the branch has an upstream on the remote
  pub ahead: usize,            // commits only here  → push
  pub behind: usize,           // commits only there → pull (as of the last fetch)
  pub last_fetched: Option<SystemTime>,   // from the time of FETCH_HEAD
}
```

The sync button is a direct function of this value (`render_git_toolbar`):

```text
 !upstream        → "Publish branch"
 behind  > 0      → "Pull <remote>"       (pull first: a diverged branch merges, then ahead > 0 remains)
 ahead   > 0      → "Push <remote>"
 otherwise        → "Fetch <remote>"
```

## Operations in detail

### Commit

`commit(path, files, summary, description)` builds the tree **from the last commit plus the chosen files only**, in memory, so what was
staged for other files is left out. Afterwards the index of the chosen files matches the commit. `amend_commit` is the same with HEAD's
parents, then moves the branch reference to the new commit (libgit2 refuses to create a commit whose parent is not the tip, so the commit
is created without updating HEAD and the branch is moved afterwards). A commit a remote already has cannot be amended.

### Pull

```rust
// honors the user's pull.rebase; otherwise merges
let args = if rebase_configured { ["pull"] } else { ["pull", "--no-rebase", "--no-edit"] };
```

If the command fails and the repository is in the `Merge` state, the error says there are conflicts and the merge stays in progress for the
resolver. (git prints `CONFLICT` on stdout, which Vekro does not read; the repository state is the reliable signal.)

### Switching branches

```text
 switch_branch(name)            safe checkout; a remote-only name first gets a tracking branch
 switch_branch_with(name, Leave | Bring)
    no changes?          → switch_branch
    stash_changes(current)           "!!Vekro<current>"  (untracked included)
    switch_branch(name)  fails?      → restore_stash(current); return the error
    Bring: restore_stash(current)    fails? → "…stay stashed on <current>: <why>"
```

A stash entry is named `!!Vekro<branch>`; a newer stash for the same branch replaces the older one **only after the new one exists**, so a
failure never loses work. User-made stashes are never touched.

### Revert and cherry-pick

Both require a clean working tree and a clean repository state. Each runs the `git` program; on failure it runs `git <command> --abort` and,
for cherry-pick, switches back to the original branch. The error names the conflicted files and says **Nothing was changed**.

### Merge

`start_merge` runs `git merge --no-ff --no-edit -- <branch>`. Uncommitted changes are allowed; Git itself refuses (changing nothing) when the
merge would overwrite them. On conflicts the merge is left in progress and `read_merge` lists the files:

```rust
pub struct MergeFile { path: String, conflicts: usize, ours: bool, theirs: bool, can_choose_side: bool }
```

`compare` also **predicts** conflicts before the working tree is touched (`merge_commits` in memory), so the interface can show a count.

### Network commands and credentials

`fetch`, `pull` and `push` go through `with_login`: run with the connected account's token; if the output looks like a refusal (403/401,
"Authentication failed", "denied", …) run again with the user's own login; if both are refused, return one message naming both. The token
reaches Git through environment variables read by a one-command credential helper — see [Hosting](./hosting.md). Progress lines from git
(`Writing objects: 45% …`) are forwarded to the interface while the command runs, and failure messages are reduced to the reason (the
rejected branch line, or what the remote said) instead of git's generic last line.

## Diffs in the interface

`file_diff` produces a unified patch that `ChangeView` hands to the Kit `Diff` through `ShownDiff`. Two details live in `diff_view.rs`:

- **Whole-line backgrounds** for added and removed lines come from the theme palette (`diff_added`, `diff_removed`), computed from the patch.
- **`parse_patch`** gives each file the language the editor uses for it, because the Kit detects languages from the extension alone. A file the
  Kit calls plain text (`Cargo.lock`, `.zsh`, `.yml`, `.kt`…) gets the editor's language when the registry knows it.

## Testing

Every function above has a test in `crates/git/tests/` that works on real repositories in temporary directories:

| File | Covers |
| --- | --- |
| `summary.rs` | the repository summary and quick status |
| `history.rs` | log, commit detail, tags, undo |
| `branches.rs` | branches, switching, sync counts, push/fetch/pull, credentials |
| `compare.rs` | branch comparison, merge, conflicts |
| `remote_workflow.rs` | a bare `origin` and two clones: fast-forward, diverged and conflicting pulls, publish, remote-only branches, rename, delete, stash Leave/Bring, merge with local changes |
| `commit_actions.rs` | revert, cherry-pick, branch from commit, amend, ignore rules, co-author suggestions |
