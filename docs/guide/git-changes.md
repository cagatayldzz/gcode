# Git: Changes & commits

The Git view has two tabs, **Changes** and **History**. This page covers **Changes**.

```text
 ┌ Changes ②1 ┬ History ┐
 │ [ Filter                ]            ← narrow the list by path
 │ ☑ 21 changed files          ▤        ← select all · "show all changes"
 ├────────────────────────────────┤
 │ ☑ lib.rs        crates/git/src  M    │
 │ ☑ branch.rs     crates/git/src  A    │  ← click a file: its diff opens in the main area
 │ ☐ notes.md                      A    │  ← unticked: not in the next commit
 ├────────────────────────────────┤
 │ Summary (required)                   │
 │ Description                      ✦ ⚙ │  ← ✦ writes both with AI
 │ [ Commit 20 files to main ]          │
 └────────────────────────────────┘
```

## The list of changes

Every changed file has a checkbox and a mark:

| Mark | Meaning |
| --- | --- |
| **A** | new (untracked or added) |
| **M** | modified |
| **D** | deleted |
| **R** | renamed |
| **!** | conflicted (see [merging](./git-branches.md#merging)) |

- **Click a file** to see its diff against the last commit in the main area, in place of the editor. Staged and unstaged changes are
  shown together, and a new file is shown as all added. Choosing another file replaces the diff; no tab is added. It updates as you
  save.
- **Double-click** a file to open the file itself in the editor, with the Explorer.
- **Filter** narrows the list by path (case-insensitive). The counts and the commit button still refer to *all* changes.
- The list is **virtualized**: only the rows in view are built, so a repository with thousands of changed files (for example an
  un-ignored `target/` folder) stays smooth.
- Images (PNG, JPEG, WebP, BMP, TIFF) show a preview; modified images show the original and the current version side by side. For an SVG,
  **Preview** renders the last commit and the working copy side by side, and **Source** returns to the text diff.
- Colors in diffs follow the file's language, including Kotlin and `Cargo.lock`.

### The file menu

Right-click a file:

| Item | Does |
| --- | --- |
| **Open in Editor** | Opens the file (disabled for a deleted file). |
| **Show in Finder** | Reveals it. For a deleted file: **Open Containing Folder**. |
| **Copy Relative Path** | Copies the path from the repository root. |
| **Ignore File / Ignore Folder / Ignore All .ext Files** | Appends a rule to the repository's `.gitignore` (`/path/file.rs`, `/dir/`, `*.ext`). A rule already in the file is not added twice. Files Git already tracks stay tracked, as with any ignore rule. |
| **Discard Changes…** | Asks first, then throws away every change to the file and deletes it if it is new. |

Right-click the **Show all changes** button (top right of the list) for **Discard All Changes…**.

### Show all changes

The button in the Changes header opens every file's diff in **one scrollable panel**.

- A bar on top shows the totals and has **Jump to file** (a menu with each file's line counts; reviewed files are checked),
  **Next unviewed**, and buttons to collapse or expand every file.
- Clicking a file in the list scrolls the panel to it.
- Each file header shows the name with its folder, can collapse its diff, copy its relative path, and mark the file **Viewed**, and
  shows added and removed line counts.
- Viewed marks are kept while the project is open and are independent of the checkboxes used to select files for a commit.

## Committing

1. **Tick** the files that belong in the commit (the header checkbox ticks them all; it shows a mixed state when only some are ticked).
2. Write a **Summary** (required) and, if you like, a **Description**.
3. Press **Commit N files to &lt;branch&gt;**.

```text
 commit(path, files, summary, description)
   └─ the commit is built from  LAST COMMIT  +  the ticked files
      ⇒ unticked files stay exactly as they are, and still show up in Changes
        (even if you had staged them before)
```

A commit needs `user.name` and `user.email` in your Git configuration. The commit button is disabled while the summary is empty, while
nothing is ticked, during a merge in progress, and while another Git operation runs.

### Commit options (right-click the Commit button)

| Item | Does |
| --- | --- |
| **Amend Last Commit** | Replaces the last commit with one that also contains the ticked files and the message in the box. **Leave the summary empty to keep the old message.** A commit a remote branch already has is refused: that would rewrite published history. |
| **Add Co-author ▸** | Lists people from recent history (not you). Choosing one adds a `Co-authored-by: Name <email>` line to the description, once. |

### Writing the message with AI

The sparkle button beside the description writes the summary and description from the diff of the **ticked** files, as a
Conventional Commits message. Nothing is committed by generating. Configure the provider, model and style with the settings button beside it
(see [Commit messages](../architecture/commit-messages.md)). With **Push after commit** on, the commit button pushes right after committing.

## Keeping it fresh

The change list refreshes when you return to the window and every 2 seconds while it is open (every 10 seconds while another window is in front), so edits made in another app show up
by themselves. The History list is read again only when a commit is made or the branch or its upstream moves. Switching between Changes and History reads nothing while that is so.
