# Architecture

Cargo workspace, one-way dependencies:

```text
apps/vekro ─► app ─► ui ─► terminal
                │      ├──► git
                │      └──► storage
                └─────────► (ui only)
```

`terminal`, `git` and `storage` never depend on the interface and are tested without a window.
`app` depends on `ui` only.

## Code map

| Path | Owns |
| --- | --- |
| `apps/vekro/src/main.rs` | executable entry point only; calls `vekro_app::run` |
| `crates/app/src/lib.rs` | startup: key bindings, quit, opening and closing windows |
| `crates/ui/src/workspace.rs` | `Workspace` (one per window): projects, the sidebar, the project rail, the status bar, the dock and the terminals of every project |
| `crates/ui/src/panels.rs` | `ExplorerPanel`: the dock panel that draws the sidebar the workspace holds |
| `crates/ui/src/editor.rs` | `EditorPanel`: the editor for a file, the diff of a changed file, or the empty state |
| `crates/ui/src/terminal.rs` | `Terminal` (a dock panel): draws a `Session`, handles keys, mouse, selection, scrolling and paste |
| `crates/ui/src/project_setup.rs` | the Add project dialog (New, Clone, Import) |
| `crates/ui/src/settings.rs` | the Settings dialog |
| `crates/ui/src/explorer.rs` | the folder tree rows |
| `crates/ui/src/selection.rs` | terminal selection math (words, extending, arrow keys) |
| `crates/ui/src/theme.rs` | the light and dark palettes, fonts, applying them to the Kit theme |
| `crates/ui/src/{components,assets}.rs` | the blue "chosen" button style; Kit icons |
| `crates/terminal/src/session.rs` | `Session`: PTY, `alacritty_terminal`, shell directory lookup |
| `crates/terminal/src/{snapshot,palette,keys,mouse,search}.rs` | styled screen snapshot, colors, key and mouse encoding, scrollback search |
| `crates/git/src/lib.rs` | `summary` (branch and changed files), `file_diff`, `create_project`, `clone`, `repository_name` |
| `crates/storage/src/` | `paths.rs` (data directory), `projects.rs` (`ProjectList`), `preferences.rs` (`Preferences`) |

## Window layout

The `Workspace` renders the title band, then a row of the **project rail** (fixed, outside the dock),
the Kit **`DockArea`**, and the status bar below. The dock holds:

- left dock: `ExplorerPanel` (not closable, not zoomable);
- center: `EditorPanel`;
- bottom dock: one `Terminal` panel per running shell of the open project.

The Kit draws the dock chrome (tab bars, drag and drop, resize handles, the `…` menu). Panels opt out of
zoom (`zoomable`, `zoom_control`), but the Kit still draws the `…` button.

`ExplorerPanel` and the terminals are separate entities. The panel draws by calling
`Workspace::render_sidebar`, so the workspace tells it to redraw whenever its data changes
(`Workspace::changed`); use that instead of a bare `cx.notify()` for anything the sidebar shows.

## Projects and terminals

- `Workspace.projects` is the `ProjectList`; the active project decides what the sidebar, the editor
  tree, the Git summary and the terminal dock show.
- `Workspace.terminals` maps each project folder to its terminals. Switching projects removes the old
  project's panels from the dock (`hide_terminals`) and adds the new one's (`show_terminals`); the entities
  stay alive in the map, so their shells keep running. A project with no terminal gets one.
- A shell that exits sends `TerminalExited`; the workspace removes the panel and, if the project has no
  terminal left, starts another.

## Terminal data flow

1. `Terminal::start` opens a PTY and the login shell through `Session::spawn`; a thread feeds output to
   `alacritty_terminal`.
2. The thread reports `SessionEvent` (`Wakeup`, `Title`, `Directory`, `ClipboardStore`, `Exit`) by callback
   into a channel. `Terminal` receives them in a task (`_events_task`), coalescing bursts of wakeups, and
   calls `cx.notify()`.
3. Each render fits the PTY to the panel (`Session::resize`) and takes a `Snapshot` (styled spans, resolved
   RGB, cursor). `Session::modes` tells the view whether a program wants mouse, alternate-screen or focus
   reports.
4. Keys go through `encode_key` to the PTY. `⌘C`, `⌘V` and `⌘K` are handled by the view.

## Git data flow

`vekro_git::summary` reads the branch and changed files (libgit2). `Workspace::refresh_in_background` runs
it, with the folder tree read, on a background thread when the window is activated and every 2 seconds
(`REFRESH_INTERVAL`), and applies the result only if the project and the expanded folders are unchanged and
the data differs. `file_diff` produces a unified patch that `EditorPanel` hands to the Kit `Diff`.
`clone` runs the `git` program (not libgit2) so the user's credentials apply.

## Persistence

Data directory `~/Library/Application Support/Vekro` (override with `VEKRO_DATA_DIR`).

| File | Type | Written |
| --- | --- | --- |
| `projects.json` | `ProjectList` (project paths, the active one) | when a project is added or selected |
| `preferences.json` | `Preferences` (theme, folder for new projects) | on each settings change |

Not stored yet: window size and position, dock layout, open files, running terminals.

## Rules

- Language, formatting, tests, GPUI Kit conventions and checks: `AGENTS.md`.
- A change to crate boundaries, the window layout or the state files must update this file.
