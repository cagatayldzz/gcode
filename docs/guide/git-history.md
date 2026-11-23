# Git: History

The **History** tab lists the commits of the current branch, newest first, **50 at a time**. Scroll to the end of the list, or press
**Load 50 more commits**, for the next page.

```text
 ┌ Select Branch to Compare…  ▾ ┐
 ├──────────────────────────────┤
 │ Fix sync button state        │
 │ ◉ Ada · 2 hours ago       ↑  │   ← ↑ : the remote does not have this commit yet
 │ Add branch rename            │
 │ ◉ Çağatay · yesterday  [v1.2]│   ← tags are shown on the right
 └──────────────────────────────┘
```

Each row shows the subject, who made it and when, its tags, and an arrow while the remote does not have it.

## A commit in the main area

Choosing a commit shows it in place of the editor:

- its **subject**, and its **message** in a fixed-width scrolling box (the button beside the subject unfolds it to most of the window and
  writes out the author's e-mail, the full id and the added and removed lines);
- the author, the short id and the `+` / `-` totals;
- the **files it changed** on the left and the **diff** of the chosen one on the right.

The first commit is shown when you open the tab. The editor tabs come back when you return to Changes or another sidebar tab.

## The commit menu

Right-click a commit:

| Item | Does |
| --- | --- |
| **Revert Changes in Commit…** | Asks, then adds a **new commit that undoes** this one. Nothing is rewritten. A merge commit is reverted against its first parent. |
| **Create Branch from Commit…** | Asks for a name, creates a branch at that commit and switches to it. |
| **Cherry-pick Commit to ▸** | Lists your other local branches. Choosing one switches to it and copies the commit onto it. Only a commit with one parent can be cherry-picked. |
| **Undo Commit…** | Only the **last** commit, and only while the remote does not have it. It goes back like `git reset --soft`: the changes return to Changes and the message to the commit box. |
| **Create Tag…** | Adds a lightweight tag to the commit. It stays local until you push it. |
| **Copy SHA** | Copies the full commit id. |

::: info Safe by design
Revert and cherry-pick need a clean working tree. If either one hits a **conflict**, gcode aborts it, puts you back on the branch you
started on and tells you which files conflicted. **Nothing is left half done.**
:::

```text
 revert / cherry-pick
   ├─ working tree dirty?    → refused ("Commit or discard your changes before …")
   ├─ success                → new commit, history reloads
   └─ conflict               → git <revert|cherry-pick> --abort
                               switch back to the original branch
                               "… conflicts in a.txt. Nothing was changed."
```

## Comparing branches

Above the list, **Select Branch to Compare…** lists the other branches: the default branch, the ones you used lately, then all the
others (local and remote). Each shows how many commits it is **behind (↓)** and **ahead (↑)** of the current branch. Type to
narrow the list; Enter takes the first match.

Choosing a branch shows two tabs:

| Tab | Shows |
| --- | --- |
| **Behind (n)** | The commits of that branch the current one does not have. |
| **Ahead (n)** | The commits of the current branch that one does not have. |

Each list shows up to 300 commits (the number in the tab is the real count). Click a commit to see it in the main area. While the other
branch is ahead, **Create a merge commit** merges it — see [Merging](./git-branches.md#merging). The cross in the field goes back to
the plain History list.
