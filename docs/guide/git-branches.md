# Git: Branches, sync & merging

A bar above the editor in the Git view holds the **current branch** and the **sync button**.

```text
 ┌──────────────────────────┬───────────────────────────────┬──────────────────┐
 │ ⑂ Current branch      ▾  │ ⟳ Fetch origin                │ ⚠ Resolve merge  │
 │   main                   │   Last fetched 9m ago         │   (only mid-merge)│
 └──────────────────────────┴───────────────────────────────┴──────────────────┘
        branch menu                  sync button
```

## The sync button

One button does whatever the branch needs, in this order:

| The branch… | The button says | It runs |
| --- | --- | --- |
| was never published | **Publish branch** | `git push --set-upstream origin HEAD` (then a fetch) |
| is **behind** its remote | **Pull origin** (↓ n) | `git pull` |
| is **ahead** of its remote | **Push origin** (↑ n) | `git push` (then a fetch) |
| is in step | **Fetch origin** | `git fetch` |

- While it works the button shows Git's own progress text ("Writing objects: 45% …") and fills like a progress bar.
- **Pull merges.** A branch that only fell behind moves forward; one that has commits of its own gets a **merge commit**. If your Git
  setting `pull.rebase` is set, it is honored instead. A pull that conflicts leaves the merge **in progress** and shows
  **Resolve merge** (see below).
- Network commands run the `git` program (not libgit2), so your own credentials apply. With a connected [account](./accounts.md) its token is
  tried first; if the host refuses it, your own Git login is tried before an error is shown.

## The branch menu

Click the current branch. The menu has two tabs: **Branches** and, when a connected account serves the repository's host,
**Pull Requests**.

### Branches

```text
 ┌ [ Find or create a branch   ] [New Branch] ┐
 │ ✓ main                                       │
 │   feature/search                             │
 │   hotfix                                     │
 │   teammate-work                      origin  │  ← exists only on the remote
 └──────────────────────────────────────────────┘
```

- **Type** to filter. **New Branch** creates a branch at the current commit and switches to it (uncommitted changes come along); the
  filter text is used as the suggested name.
- A branch that **only a remote has** is listed with the remote's name beside it. Choosing it creates a local branch that
  **follows** the remote one and switches to it, like `git switch`.
- **Right-click** a branch:

| Item | Does |
| --- | --- |
| **Rename…** | Renames a local branch. The name must be valid and not taken; a branch that follows a remote keeps following it. Disabled for remote-only branches. |
| **Delete…** | Asks first. A local branch is deleted (its commits stay until Git cleans up). If it also exists on its remote you can delete it **there too**. A remote-only branch is deleted on the remote. The branch you are on cannot be deleted. |

### Switching with uncommitted changes

If the working tree has changes, choosing another branch asks what to do with them:

```text
 ┌ Switch to feature ─────────────────────────────────────────┐
 │ You have changes on main (3 changed files).                 │
 │                                                             │
 │ Leave my changes on main  → stashed, waiting on main        │
 │ Bring my changes to feature → applied on feature            │
 │                                                             │
 │              [ Cancel ] [ Bring My Changes ] [ Leave My Changes ] │
 └─────────────────────────────────────────────────────────────┘
```

| Choice | What happens |
| --- | --- |
| **Leave My Changes** | Everything (untracked files included) is stashed under the branch you are leaving. The stash is named after that branch, and a newer stash of the same branch replaces the older one only after it was created. |
| **Bring My Changes** | The changes are stashed, the branch is switched, and they are applied on the new branch. **If they collide** with that branch, they stay safely in the stash and the message says so. Nothing is lost. |

When you switch to a branch that has changes stashed on it, gcode asks **Restore / Discard / Not Now**.

Without changes, the switch happens immediately. A plain safe checkout never overwrites anything: when files are in the way the
message says *"Your changes would be overwritten by switching to …"*.

### Pull requests

When a connected account serves the host, the second tab lists the repository's **open pull requests** with a filter and a refresh
button. Click one to open it in the browser. With none open it says *You're all set!*.

**Create pull request…** (for the current branch) asks for a title, description, target branch and whether it is a draft. The branch
must be published first. See [Accounts](./accounts.md) for the permissions the token needs.

## Merging

Merging starts in **History → Select Branch to Compare…** ([details](./git-history.md#comparing-branches)). While the other branch is ahead,
**Create a merge commit** merges it into the current branch (`git merge --no-ff`), **only when you press it**.

- Uncommitted changes are fine as long as they do not touch the files the merge changes. If they do, Git refuses and **nothing is
  changed**; the message names the files.
- Before you press, the comparison already **predicts conflicts** and shows how many files would conflict.

### Resolving conflicts

If the merge conflicts, whether from this button or from a pull, it is left **in progress** and a resolver opens. The **Resolve merge**
button in the bar reopens it any time.

| Control | Does |
| --- | --- |
| **Use file from &lt;branch&gt;** / **Keep deletion from &lt;branch&gt;** | Resolve a file by taking one side whole (current or incoming). |
| **Undo** | Takes a resolution back for a file. |
| **Open in editor** / **Open with default app** / **Reveal in Finder** | Resolve by hand; then stage the file in Git and press **Refresh**. |
| **Open terminal** | A shell in the project for anything else. |
| **Refresh** | Reads the conflicts again. |
| **Continue merge** | Enabled when every conflict is resolved; finishes the merge commit. |
| **Abort merge** | Returns the repository to its state before the merge; resolutions are discarded. |

While a merge is in progress, committing, switching branches and syncing are disabled.

## Stashes, in short

gcode uses Git's stash for **Leave/Bring My Changes**, naming each entry `!!app-stash<branch>`. Your own stashes are never touched.
