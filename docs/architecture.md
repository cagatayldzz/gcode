# Architecture

Cargo workspace, one-way dependencies:

```text
apps/vekro ─► app ─► ui ─► terminal
                │      ├──► git
                │      ├──► opencommit
                │      ├──► hosting
                │      ├──► updater ─► version
                │      └──► storage
                └─────────► (ui only)
```

`terminal`, `git`, `opencommit`, `hosting`, `updater`, `version` and `storage` never depend on the interface and are tested without a window.
`app` depends on `ui` only.
`visual-check` is a standalone development tool; the application does not depend on it.

## Code map

| Path | Owns |
| --- | --- |
| `apps/vekro/src/main.rs` | executable entry point only; calls `vekro_app::run` |
| `crates/visual-check/` | offline screenshot pixel inspection and coordinate conversion |
| `crates/app/src/lib.rs` | startup: key bindings, quit, opening and closing windows |
| `crates/ui/src/workspace.rs` | `Workspace` (one per window): projects, the title bar (repository switcher, tabs), the sidebar, the status bar, the dock and the terminals of every project |
| `crates/ui/src/editor.rs` | `EditorPanel`: tabs (`Tabs`) of file editors, shown with the Kit default `TabBar` that the panel draws itself (the dock title bar is off), plus SVG and ICNS previews decoded off the interface thread |
| `crates/ui/src/terminal.rs` | `Terminal` (a view): draws a `Session`, handles keys, mouse, selection, scrolling, paste and its right-click menu |
| `crates/ui/src/menus.rs` | the menu bar (`menus`); the app sets it at startup and the window in front updates the Repository item with the service of the open project's remote (`Workspace::sync_menus`, `view_on_host`, `vekro_hosting::web_repository`) |
| `crates/ui/src/agents.rs` | the AI agents (`AgentKind`: Codex, Claude; the shell line that runs each) and a project's agent sessions as plain data (`AgentTabs`), tested without a window |
| `crates/ui/src/terminal_layout.rs` | the tabs and split panes of a project's terminals as plain data (`Arrangement`, `Node`, `Direction`), tested without a window |
| `crates/ui/src/commit_settings.rs` | the OpenCommit settings dialog (model, diff size, language, message style, instructions, push after commit) |
| `crates/ui/src/all_changes.rs` | `AllChanges`: a single virtualized Kit diff of the working tree; the review controls sit in each file header |
| `crates/ui/src/change_view.rs` | `ChangeView`: the diff of the chosen file of the Changes view in the main area, in place of the editor (re-read every refresh, redrawn only when it differs) |
| `crates/git/src/compare.rs` | comparing the current branch with another (`compare_choices`, `compare`); the History view's "Select Branch to Compare…" uses it |
| `crates/git/src/merge.rs` | predicting merge conflicts and operating on the pending Git merge; the resolver is owned by the workspace |
| `crates/ui/src/commit_view.rs` | `CommitView`: a commit of the History view in the main area (message, changed files, the diff of the chosen one) |
| `crates/ui/src/project_setup.rs` | the Add project dialog (New, Clone, Import) |
| `crates/ui/src/project_settings.rs` | the dialog for a project's name, icon, color and Git remote address (`vekro_git::set_remote_url`), with a page list on the left like Settings (Kit `Settings`, Save / Cancel footer) |
| `crates/ui/src/project_manager.rs` | the Project bar page of Settings: every project and category with its actions |
| `crates/ui/src/settings.rs` | the Settings pages (the Kit `Settings` component) and `SettingsModel`, the settings every window shares |
| `crates/ui/src/explorer.rs` | the folder tree rows |
| `crates/ui/src/selection.rs` | terminal selection math (words, extending, arrow keys) |
| `crates/ui/src/theme.rs` | the light and dark palettes, fonts, applying them to the Kit theme |
| `crates/ui/src/{components,assets}.rs` | the blue "chosen" button style; Kit icons |
| `crates/terminal/src/session.rs` | `Session`: PTY, `alacritty_terminal`, shell directory lookup |
| `crates/terminal/src/{snapshot,palette,keys,mouse,search}.rs` | styled screen snapshot, colors, key and mouse encoding, scrollback search |
| `crates/git/src/lib.rs` | `summary` (branch, changed files, and `Sync`: remote, ahead/behind, last fetch), `file_diff`, `diff_files`, `history`, `commit_detail`, `commit_file_diff`, `undo_commit`, `create_tag`, `remote_url`, `set_remote_url`, `quick_status`, `branches`, `create_branch`, `switch_branch`, `commit`, `push`, `fetch`, `pull`, `create_project`, `clone`, `repository_name`, `find_repositories` |
| `crates/updater/src/`, `crates/version/src/` | release lookup, download, verification, installer script; version comparison |
| `crates/ui/src/updates.rs` | the update state of the shared settings model (`UpdateState`, `AppUpdate`) with its actions, and `UpdatePanel`, the Updates page of Settings |
| `crates/hosting/src/` | `accounts.rs` (`Account`, `Accounts`, `TokenStore`: the macOS Keychain through `security`, or memory), `api.rs` (verify, repositories, pull requests per service), `remote.rs` (`RepoRef`: which repository a remote URL names) |
| `crates/ui/src/{connections,connect_account,repository_picker,pull_request}.rs` | the accounts list in Settings and the helpers that find the login for a remote; the Connect dialog; the repository list of the Clone tab; the Create pull request dialog |
| `crates/opencommit/src/` | `config.rs` (`Config`, the `~/.opencommit` file), `prompt.rs` (rules, diff splitting, parsing the answer), `client.rs` (`generate`, the chat API request) |
| `crates/storage/src/` | `paths.rs` (data directory), `projects.rs` (`ProjectList`), `preferences.rs` (`Preferences`) |

## Window layout

The `Workspace` renders the title band (right of the traffic lights: the **repository switcher**, a fixed first tab with a filterable, category-grouped menu of every project and an Add button; then the Add button), then a row of the **activity bar** (`render_activity_bar`: a 56 px strip with one button each for Explorer and
Git, which choose `sidebar_tab`, the terminal button under them, and Settings at the bottom with the account warning badge) and the **sidebar** (the selected view, in a Kit `h_resizable` pane, 200–960 px) and, right of it, a column with the Kit **`DockArea`** and the status bar below it, so the sidebar runs down to the bottom of the window. The sidebar is plain
workspace content, not a dock panel, so it has no tab bar. The dock holds only the center: `EditorPanel`. The terminals are
not dock panels: the activity bar's terminal button (`ToggleTerminal`, `⌘J`; `terminal_open`) makes them fill everything right of the activity bar in place of the sidebar, the dock and the status bar
(`render_terminal_area`): a tab strip when there are several tabs, and the shown tab as a tree of panes (`terminal_layout.rs`: `Node`,
`Arrangement`; splits are Kit `h_resizable`/`v_resizable` groups, a pane has a header when its tab has several). `⌘T`, `⌘D`,
`⇧⌘D` and `⌘W` act on the shell that has the keyboard; a terminal's right-click menu (`terminal.rs`) uses the same actions.
Showing the editor again keeps the shells running.

The **AI view** is the activity bar's last button (`ToggleAgents`, `⇧⌘A`; `agent_open`). It fills the same area as the terminals (the two never show together; `full_area_open`) but is not a terminal window: its header holds one tab per session (agent icon and name), a menu to start another agent, and the project name, with no splits. A session is a `Terminal` that runs `$SHELL -l -i -c "exec codex"` (or `claude`) in the project folder, so the user's PATH finds the agent; a missing agent prints a message and waits for Enter. `NewCodex` and `NewClaude` start a session. With no session the view shows the choice of agents (`render_agent_empty`); when the last session ends it stays open and shows it again. Sessions live in `Workspace.agent_tabs` per project and keep running while another project or view is shown.

The Kit draws the dock chrome of the editor (the title, zoom button and `…` menu).

## Projects and terminals

- `Workspace.projects` is the `ProjectList`, shown in the repository menu of the title bar; the active project decides what the sidebar, the editor
  tree, the Git summary and the terminals show.
- `Workspace::switch_project` is the only way the open project changes. It puts the old project's Explorer and Git
  selections away (`Workspace.views`) and its editor
  tabs away (`EditorPanel::switch_project`, `saved`), applies the change, restores the new project's views, and
  makes sure the new project has a shell (`ensure_terminal`). Editors that other projects have open are never evicted
  from the cache.
- `Workspace.terminal_tabs` maps each project folder to an `Arrangement` of its terminals (tabs, each a tree of panes); the entities
  stay alive in the map while another project is open, so their shells keep running. A project with no terminal gets one when
  it is opened or when the terminals are shown.
- A shell that exits sends `TerminalExited`, and closing a pane does the same job (`close_pane`): the workspace takes the pane
  out (its sibling takes its place, an empty tab goes) and, if the project has no shell left, shows the editor again.

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
the data differs. `file_diff` produces a unified patch that `ChangeView` hands to the Kit `Diff`.
`clone`, `push`, `fetch` and `pull` (`--ff-only`) run the `git` program (not libgit2) so the user's credentials apply. In the Git tab a toolbar above the editor (`render_git_toolbar`) holds the current branch, whose menu has two tabs: the local branches, and the open pull requests when a connected account serves the remote (`branches`, filtered by typing) with a way to create one, and one button that publishes, pushes, pulls or fetches, whichever the branch needs (`sync_remote`); `switch_branch` uses a safe checkout, so
changes it would overwrite stop the switch instead of being lost.

## Commit messages

The sparkle button beside the commit description calls `Workspace::generate_commit_message`: on a background thread it builds the
diff of the ticked files (`vekro_git::diff_files`) and `vekro_opencommit::generate` asks an OpenAI-compatible chat API for a
Conventional Commits message, which fills the summary and description fields. A diff over the configured token limit is split per
file, each part is summarized, and the summaries are merged in a last request. Nothing is committed by generating. The settings
live in the user's `~/.opencommit` (shared with the `oco` command; unknown lines are kept, the file is mode 0600; the
`VEKRO_OPENCOMMIT_CONFIG` variable names another file for scratch runs); the commit instructions are in `preferences.json`.
With "Push after commit" the commit button pushes with the `git` program after committing.

## Updates

`vekro_updater::check` asks the GitHub API for the latest release of the repository in `[workspace.package]`, and `vekro_version::is_newer`
decides whether it is newer than the running version (`CARGO_PKG_VERSION`, stamped by the release workflow). The release's assets
must be `Vekro-<tag>-macos-arm64.zip` and `SHA256SUMS.txt` under that repository's own download URLs, so a tampered answer cannot
point the updater elsewhere. Only on request (`SettingsModel::download_update`) is the archive downloaded, next to the running app,
and verified: SHA-256 against the checksums, `codesign --verify`, `spctl`, the same Team ID as the installed app, the same bundle
identifier and the release's version. **Restart to Update** (`restart_to_update`) quits and runs `SWAP_SCRIPT`, which waits for
the app to exit, swaps `Vekro.app` and reopens it, restoring the old app if the swap fails. All of it runs on background
threads; progress reaches the model through a channel. Development builds (`debug_assertions`) never check or install. The first
window checks at launch unless `check_updates` is off in `preferences.json`, and announces an available release once.

## Hosting accounts

A connected account (`vekro_hosting::Account`) is listed in `accounts.json`; its personal access token is in the macOS Keychain
(`KeychainStore`, the `security` program; the token is an argument of that command for a moment, and is never written to a
file or a log). Everything that touches the network or the Keychain runs on a background thread: verifying a token when an
account is connected, listing repositories (Clone tab), listing and creating pull requests (`Workspace::load_pull_requests`,
`PullRequestForm`). For `clone`, `push`, `fetch` and `pull` of an HTTPS remote, `connections::credentials_for_repository` finds
the account for the remote's host and gives the `git` program the user name and token through environment variables read by a
one-command credential helper (`vekro_git::CREDENTIAL_HELPER`), so they appear in no command line or file; if the server refuses the token (HTTP 401/403), the command is run once more with the
user's own Git login, and a message names both when both fail. SSH remotes keep the user's keys. Self-hosted GitHub Enterprise and GitLab are reached by naming the host; Azure DevOps uses the organization.

A refused token marks its account: `Account.issue` (in `accounts.json`) holds what is wrong. It is set when `vekro_git::TOKEN_REFUSED`
is in the error of a push, pull or fetch, when a pull request is refused (`PullRequestEvent::Refused`), or when connecting finds a
permission missing; it is cleared by `Workspace::recheck_account`, which runs `vekro_hosting::verify_permissions` (the push
advertisement `info/refs?service=git-receive-pack` and an empty pull request, neither of which changes anything) on a background
thread. While any account has an issue, the gear at the bottom of the activity bar shows a warning badge that opens the Accounts page.

## Persistence

Data directory `~/Library/Application Support/Vekro` (override with `VEKRO_DATA_DIR`).

| File | Type | Written |
| --- | --- | --- |
| `projects.json` | `ProjectList` (project paths with their name, icon, color and category; the categories and whether each is folded; the active project) | when a project is added or selected, or a category changes |
| `accounts.json` | `Accounts` (service, host, organization and user name of each connected account; tokens are in the Keychain) | when an account is connected or disconnected |
| `window.json` | `WindowPlacement` (position, size and whether the window was maximized or full screen) | half a second after the window stops moving or resizing |
| `preferences.json` | `Preferences` (theme, projects folder, code font size, commit instructions) | on each settings change |

Not stored yet: window size and position, dock layout, open files, running terminals.

## Rules

- Language, formatting, tests, GPUI Kit conventions and checks: `AGENTS.md`.
- A change to crate boundaries, the window layout or the state files must update this file.

The activity bar opens Search below Explorer and keeps project settings at its bottom. Search owns a separate sidebar view, and its results can open as a read-only editor tab.
