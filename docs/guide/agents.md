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

## Mention other projects — `⇧⌘M`

Press `⇧⌘M` in the AI view or in the terminals to open a list of your projects. Tick some, and their paths are written into the
prompt as shell-escaped paths, so one prompt can refer to several projects.

## Choosing which buttons show

**Settings → Activity bar** reorders the activity bar's buttons and hides the ones you do not use.
