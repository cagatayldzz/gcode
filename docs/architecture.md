# Architecture

Cargo workspace, one-way dependencies:

```text
apps/vekro ─► app ─► ui ─► terminal
                │      ├──► git
                │      ├──► opencommit
                │      └──► storage
                └─────────► (ui only)
```

`terminal`, `git`, `opencommit` and `storage` never depend on the interface and are tested without a window.
`app` depends on `ui` only.

## Code map

| Path | Owns |
| --- | --- |
| `apps/vekro/src/main.rs` | executable entry point only; calls `vekro_app::run` |
| `crates/app/src/lib.rs` | startup: key bindings, quit, opening and closing windows |
| `crates/ui/src/workspace.rs` | `Workspace` (one per window): projects, the sidebar, the project rail, the status bar, the sidebar, the dock and the terminals of every project |
| `crates/ui/src/editor.rs` | `EditorPanel`: tabs (`Tabs`) of file editors and file diffs, shown with the Kit default `TabBar` that the panel draws itself (the dock title bar is off), or the empty state |
| `crates/ui/src/terminal.rs` | `Terminal` (a view): draws a `Session`, handles keys, mouse, selection, scrolling and paste |
| `crates/ui/src/commit_settings.rs` | the OpenCommit settings dialog (model, diff size, language, message style, instructions, push after commit) |
| `crates/ui/src/project_setup.rs` | the Add project dialog (New, Clone, Import) |
| `crates/ui/src/project_settings.rs` | the dialog for a project's name, icon and color |
| `crates/ui/src/project_manager.rs` | the Project bar page of Settings: every project and category with its actions |
| `crates/ui/src/settings.rs` | the Settings pages (the Kit `Settings` component) and `SettingsModel`, the settings every window shares |
| `crates/ui/src/explorer.rs` | the folder tree rows |
| `crates/ui/src/selection.rs` | terminal selection math (words, extending, arrow keys) |
| `crates/ui/src/theme.rs` | the light and dark palettes, fonts, applying them to the Kit theme |
| `crates/ui/src/{components,assets}.rs` | the blue "chosen" button style; Kit icons |
| `crates/terminal/src/session.rs` | `Session`: PTY, `alacritty_terminal`, shell directory lookup |
| `crates/terminal/src/{snapshot,palette,keys,mouse,search}.rs` | styled screen snapshot, colors, key and mouse encoding, scrollback search |
| `crates/git/src/lib.rs` | `summary` (branch and changed files), `file_diff`, `diff_files`, `branches`, `create_branch`, `switch_branch`, `commit`, `push`, `create_project`, `clone`, `repository_name`, `find_repositories` |
| `crates/opencommit/src/` | `config.rs` (`Config`, the `~/.opencommit` file), `prompt.rs` (rules, diff splitting, parsing the answer), `client.rs` (`generate`, the chat API request) |
| `crates/storage/src/` | `paths.rs` (data directory), `projects.rs` (`ProjectList`), `preferences.rs` (`Preferences`) |

## Window layout

The `Workspace` renders the title band, then a row of the **project rail** (fixed, 60 px; projects can be dragged onto each other to group them into categories; a category is one tile that opens a scrollable, filterable list of its projects beside the rail), the **sidebar** (an
icon `TabBar` that switches between Explorer, Git and Terminal, and the selected view, in a Kit `h_resizable` pane,
200–960 px) and, right of it, a column with the Kit **`DockArea`** and the status bar below it, so the rail and the sidebar run down to the bottom of the window. The sidebar is plain
workspace content, not a dock panel, so it has no tab bar. The dock holds only the center: `EditorPanel`. The terminals are
not dock panels: the Terminal tab of the sidebar lists them (`render_terminal_sessions`), and while that tab is chosen the
main area draws the open project's shell, or two side by side, in place of the dock (`render_terminal_area`: a title row
with the buttons, then the shells).

The Kit draws the dock chrome of the editor (the title, zoom button and `…` menu).

## Projects and terminals

- `Workspace.projects` is the `ProjectList`, shown as the project rail; the active project decides what the sidebar, the editor
  tree, the Git summary and the Terminal tab show.
- `Workspace::switch_project` is the only way the open project changes. It puts the old project's Explorer and Git
  selections away (`Workspace.views`) and its editor
  tabs away (`EditorPanel::switch_project`, `saved`), applies the change, restores the new project's views, and
  makes sure the new project has a shell (`ensure_terminal`). Editors that other projects have open are never evicted
  from the cache.
- `Workspace.terminals` maps each project folder to its terminals; the entities stay alive in the map while another
  project is open, so their shells keep running. `active_terminal` says which one the Terminal tab shows and
  `split_terminals` which projects show two side by side. A project with no terminal gets one.
- A shell that exits sends `TerminalExited`, and Kill terminal does the same job (`close_terminal`): the workspace
  removes the terminal and, if the project has none left, starts another.

## Opening files quickly

Opening a file must feel instant. Measured in a development build, reading and building the editor take a few
milliseconds; the cost is the first time the highlighter draws a text (up to ~0.1 s for a few tens of KB). So:

- `EditorPanel` keeps the editor of every file it has opened (`files`, up to 24; files with unsaved changes are
  never dropped, and files in a tab of any project stay). Reopening a file is instant and keeps its unsaved text, also after
  its tab was closed. The panel sends `ActiveTab` whenever the shown tab changes, and the workspace selects the same
  file or change in the sidebar.
- When the pointer rests on a file in the Explorer for 120 ms (`HOVER_PREPARE_DELAY`), `EditorPanel::prefetch` reads
  it and draws its editor out of sight for half a second, so the highlighting is done before the click. Files over
  100 KB are skipped, because drawing them would stall the window; they open on click and finish highlighting in the
  background.
- Shortly after launch the panel draws a few lines of each common language out of sight, one every 250 ms, so the
  Kit's syntax definitions are loaded before the first file.
- `Cargo.toml` optimizes all dependencies in the dev profile (`opt-level = 3`); without it the highlighter is about
  ten times slower.

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
`clone` and `push` run the `git` program (not libgit2) so the user's credentials apply. The branch row of the Git tab opens a
menu of local branches (`branches`, filtered by typing) with a way to create one; `switch_branch` uses a safe checkout, so
changes it would overwrite stop the switch instead of being lost.

## Commit messages

The sparkle button beside the commit description calls `Workspace::generate_commit_message`: on a background thread it builds the
diff of the ticked files (`vekro_git::diff_files`) and `vekro_opencommit::generate` asks an OpenAI-compatible chat API for a
Conventional Commits message, which fills the summary and description fields. A diff over the configured token limit is split per
file, each part is summarized, and the summaries are merged in a last request. Nothing is committed by generating. The settings
live in the user's `~/.opencommit` (shared with the `oco` command; unknown lines are kept, the file is mode 0600; the
`VEKRO_OPENCOMMIT_CONFIG` variable names another file for scratch runs); the commit instructions are in `preferences.json`.
With "Push after commit" the commit button pushes with the `git` program after committing.

## Persistence

Data directory `~/Library/Application Support/Vekro` (override with `VEKRO_DATA_DIR`).

| File | Type | Written |
| --- | --- | --- |
| `projects.json` | `ProjectList` (project paths with their name, icon, color and category; the categories and whether each is folded; the active project) | when a project is added or selected, or a category changes |
| `preferences.json` | `Preferences` (theme, projects folder, code font size, commit instructions) | on each settings change |

Not stored yet: window size and position, dock layout, open files, running terminals.

## Rules

- Language, formatting, tests, GPUI Kit conventions and checks: `AGENTS.md`.
- A change to crate boundaries, the window layout or the state files must update this file.
