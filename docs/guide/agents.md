# AI agents

The last button of the activity bar (`⇧⌘A`) opens the **AI view**. It fills the same area as the terminals — the two never show at
the same time — but it is organized around coding agents instead of shells.

```text
 ┌ ✦ Codex ─┬ ✦ Claude ─┬ ▾ ┬ project: atlas ─┐
 ├──────────┴───────────┴───┴─────────────────┤
 │  an interactive agent running in the         │
 │  project folder                              │
 └──────────────────────────────────────────────┘
   tab per session │ menu to start another │ no splits
```

## Sessions

- Two agents are supported: **Codex** and **Claude** (`codex` and `claude` on your PATH).
- A session is a terminal that runs `$SHELL -l -i -c "exec codex"` (or `claude`) **in the project folder**, through your login shell,
  so your usual PATH finds the agent. A missing agent prints a message and waits for Enter.
- Start one with the menu in the header, **View → New Codex Session** or **New Claude Session**.
- With no session, the view shows the choice of agents. When the last session ends the view stays open and shows it again.
- Sessions live **per project** and keep running while you use another project or view.

## Agent worktrees

From **New Worktree** or the **New Session** menu, choose **Codex in New Worktree** or **Claude in New Worktree**. The Command Palette also has **New Codex Worktree**
and **New Claude Worktree**. Each starts a fresh branch at the current project’s HEAD, with its own working directory and staging area.
Changes that are still uncommitted in the source project stay there. The terminal tab shows the agent and its worktree name.

Open **Agent Changes** in the AI view or **Review Agent Changes** in the Command Palette. Choose a session to see file diffs and line
totals against its starting commit. This shows the net changes on disk, including commits made during the session and untracked files
that Git does not ignore. Use **Refresh** to read the latest files.

**Open Git Changes** opens the worktree as a separate project. Use its Git panel to stage and commit pending changes, or History to see
commits already made. To bring agent commits into the source branch, switch back to the source project, compare the agent branch in Git History, and choose
**Create a merge commit**.

Worktrees remain after the agent exits or the app closes. To remove a clean checkout, stop its agent and use
[Git: Manage Worktrees](./git-branches.md#worktrees).

## Mention other projects — `⇧⌘M`

Press `⇧⌘M` in the AI view or in the terminals to open a list of your projects. Type to filter, move with `↑` and `↓`, and press
`Enter` to write the highlighted project's path into the prompt. To mention several, press `⇧Enter` on each one to tick it, then `Enter`
(the mouse works too: click a row to tick it). Paths are shell-escaped, so one prompt can refer to several projects.

## Choosing which buttons show

**Settings → Activity bar** reorders the activity bar's buttons and hides the ones you do not use.
