# Git

`ws-git` is the one crate that knows how to talk to a repository. It never depends on the interface, creates its test repositories in
temporary directories, and exposes plain functions that return `Result<_, String>` — the string is a sentence meant for the user.

## Two engines, one rule

```text
              reading state, local edits                 commit, network and sequencer
             ┌────────────────────────────┐             ┌──────────────────────────────┐
             │ libgit2  (git2 crate)       │             │ the `git` program             │
             │  summary · history · diff   │             │  clone · fetch · pull · push  │
             │  branches · tags            │             │  merge · revert · cherry-pick │
             │  create repository          │             │  commit · amend · stash       │
             └────────────────────────────┘             └──────────────────────────────┘
                  fast, in-process                          the user's own credentials apply
```

**Rule:** `git2` is used to read state and create repositories; the `git` program is used for `clone` and everything that needs the user's
credentials, hooks, signing, or Git's full merge machinery. And **no Git command that changes the user's repository runs from the interface without an
explicit user action.**

## What the crate exposes

| Area | Functions |
| --- | --- |
| **State** | `summary`, `quick_status`, `head_summary`, `head_message`, `IgnoreRules` |
| **Diffs** | `file_diff`, `all_changes_diff`, `diff_files`, `file_versions` (for image previews), `commit_file_diff` |
| **History** | `history`, `file_history`, `commit_detail`, `recent_authors` |
| **Commit** | `commit`, `amend_commit`, `undo_commit`, `discard_changes`, `create_tag`, `add_to_gitignore` |
| **Branches** | `branch_entries`, `create_branch`, `switch_branch`, `switch_branch_with`, `delete_branch`, `delete_remote_branch`, `rename_branch`, `branch_from_commit`, `upstream_remote` |
| **Stash** | `stash_changes`, `stash_ids`, `restore_stash_by_id`, `discard_stash_by_id` |
| **Operation state** | `operation`, `resume_operation` |
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

Commit and amend use `git commit --only`, so Git enforces repository hooks, commit signing and its expected-HEAD update. Files
are selected by literal paths, including both paths of a selected rename. The selection uses whole working-tree files. Staged
changes on unticked paths stay outside the commit unless a hook explicitly stages them into it.

The app holds the worktree's real `index.lock` and copies its index to a private, temporary directory under the Git directory.
Untracked selections are registered with intent to add in that copy. Git only writes the isolated index until the commit succeeds.
The app aligns selected paths and hook-added paths with the committed tree, preserving prior staging for unrelated files, and
atomically publishes the index. A hook rejection or signing failure leaves the real index intact. Hooks can still change files in
the working tree. When Git commits successfully but publishing the index fails, the result carries the created commit and a
separate warning. The interface skips automatic push on that outcome.

Message-only amend uses the same transaction with an empty selection. Amend checks that the existing commit is unpublished and
is not a merge commit. The normal commit and amend flows refuse to start during another Git operation.

### Pull

```rust
// honors the user's pull.rebase; otherwise merges
let args = if rebase_configured { ["pull"] } else { ["pull", "--no-rebase", "--no-edit"] };
```

The repository state distinguishes an interrupted merge from a rebase. The Git toolbar opens the merge resolver or offers
Continue, Skip and Abort for rebase, cherry-pick and revert. Resolve and stage conflicts before continuing. Bisect and patch
application are shown as active operations with terminal guidance. Commit, amend, undo, branch switching and Git discard
check the state again before changing the repository.

### Switching branches

```text
 switch_branch(name)            safe checkout; a remote-only name first gets a tracking branch
 switch_branch_with(name, Leave | Bring)
    no changes?          → switch_branch
    create stash(current)           untracked files included; capture the new stash ID
    switch_branch(name)  fails?      → attempt to restore that stash on the original branch
    Bring                            → apply that stash and restore its index
```

New stashes retain all earlier entries. The branch list recognizes existing `!!app-stash<branch>` markers. Restore uses
the selected stash commit ID. After successful application, the current reflog selector is found again before dropping
that entry. A no-op stash push does not select an older entry. Git's stash reflog is shared with the terminal.

### Revert and cherry-pick

Both require a clean working tree and an idle repository. A conflicting Git command is aborted. Cherry-pick also attempts
to return to the original branch or detached commit. If the abort or branch restoration fails, the error reports that
failure and the toolbar shows any operation still in progress.

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
reaches Git through environment variables read by a one-command credential helper — see [Hosting](./hosting.md). Remote commands stream
standard output and standard error to the interface. Git runs the repository's configured pre-push hook with the user's login-shell PATH.
A failing hook stops the push, and its diagnostics can appear in the failure notification alongside Git or remote errors.

## Diffs in the interface

`file_diff` and `commit_file_diff` carry the whole file as context, so that the Kit's Expand Up / Down / All reach lines far from the changes (the Kit
shows three lines around each change at first). `all_changes_diff` keeps the usual three, since it covers many files. `ChangeView` and `CommitView` hand the patch to `DiffTable` (`diff_table.rs`, over the lines and folds of `diff_model.rs`), which draws
the one-file diff itself: three lines around each change, a band of folded lines with expand arrows (20 lines at a time), unified or side by side, long lines wrapped
or scrolling sideways (the same `WrapLines` setting as the Kit diffs), and text chosen by dragging or lines chosen by their numbers (⌘C copies). The all-changes view and the editor's preview still use the Kit `Diff` through `ShownDiff`; three details live in `diff_view.rs`:

- **Wrapping** is one setting (`WrapLines`) read by `ShownDiff::view`, so every place that shows a diff follows it.

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
