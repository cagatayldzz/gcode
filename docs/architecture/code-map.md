# Code map

A directory-level tour of the repository, then each crate in detail.

```text
repository/
├── product.toml           the ONE place the product's name is written
├── apps/desktop/          executable entry point (binary ws-desktop), macOS bundle files (Info.plist template, icons)
├── crates/
│   ├── app/               startup, key bindings, menus wiring, windows
│   ├── ui/                all views  ── the biggest crate
│   │   ├── src/
│   │   │   ├── workspace.rs          the Workspace: one per window
│   │   │   └── workspace/            `impl Workspace`, split by topic
│   │   ├── tests/                    headless tests (window render, highlighting, layouts…)
│   │   └── benches/performance.rs    benchmark suite
│   ├── product/           ws-product: the name, as constants generated from product.toml
│   ├── terminal/          PTY + emulator
│   ├── git/               libgit2 reads, git-program network & merge operations
│   ├── storage/           files gCode owns
│   ├── index/             project file index, fuzzy matching, line highlighters
│   ├── hosting/           hosting-service accounts and APIs
│   ├── activity/         activity log (debug button opens its bottom panel)
│   ├── commitgen/        commit message generation
│   ├── language/          language server client: errors and warnings of open files
│   ├── updater/           self-update
│   ├── version/           version comparison
│   ├── capture/           screenshot CLI for the running app (window or screen)
│   └── visual-check/      screenshot pixel inspection tool
├── docs/                  this site (VitePress)
├── scripts/               build, bundle, sign, release, profile; shell tests in scripts/tests/
├── .github/workflows/     ci.yml · docs.yml · release.yml
└── AGENTS.md              rules for contributors and AI agents
```

## `crates/ui` — the interface

### The workspace

| File | Owns |
| --- | --- |
| `workspace.rs` | `Workspace` (one per window): projects, the title bar, the sidebar, the status bar, the dock and the terminals of every project |
| `workspace/projects.rs` | the project list, `switch_project`, background refresh |
| `workspace/entries.rs` | explorer entries: new file/folder, name prompts (`ask_name`) |
| `workspace/file_actions.rs` | the file menu: cut/copy/paste, rename, delete, reveal, timeline |
| `workspace/changes.rs` | the list of changed files: discard, commit |
| `workspace/commit.rs` | updates, account connection, commit message generation |
| `workspace/git.rs` | Git state: sync button, history, comparing, tags, pull requests, branch menu |
| `workspace/branches.rs` | branch actions: switch with changes, stash prompts, rename, delete; commit actions: revert, cherry-pick, branch from commit, amend, co-authors, ignore rules |
| `workspace/search.rs` | project search and replacement |
| `workspace/terminals.rs` | terminal and agent panes |
| `workspace/accounts.rs` | account menus and checks |
| `workspace/profiles.rs` | adding, renaming, deleting profiles from a window |
| `workspace/quick_open.rs`, `command_palette.rs` | the Go to File and command palette entry points |
| `workspace/render_panels.rs` | Explorer, sidebar, activity bar, terminal and agent panes |
| `workspace/render_git.rs` | the Git toolbar, the changes list, history and compare views |
| `workspace/render_menus.rs` | status bar and pickers: the branch and project menus |
| `workspace/helpers.rs` | free helpers shared by the workspace views |

### Views

| File | Owns |
| --- | --- |
| `editor.rs` | `EditorPanel`: file editors in tabs, SVG/ICNS previews, the editor cache |
| `minimap.rs`, `minimap/` | the minimap beside long files |
| `terminal.rs` | `Terminal`: draws a `Session`; keys, mouse, selection, scrolling, paste, right-click menu |
| `change_view.rs` | `ChangeView`: the diff of the chosen file in the Changes view |
| `commit_view.rs` | `CommitView`: a commit of History (message, files, diff) |
| `all_changes.rs` | `AllChanges`: one virtualized diff of the whole working tree |
| `diff_view.rs` | `ShownDiff`: the Kit `Diff` with theme-colored lines, and `parse_patch` (language-aware) |
| `merge_dialog.rs` | the merge conflict resolver |
| `explorer.rs` | folder tree rows |
| `project_search.rs` | search/replace engine (runs on the background executor) |
| `quick_open.rs`, `command_palette.rs` | the Go to File and command palette dialogs |
| `mention_picker.rs` | the project list opened by `⇧⌘M` |
| `project_setup.rs` | the Add project dialog (New, Clone, Import) |
| `project_settings.rs` | per-project settings: General (display name, folder rename, repository rename, Git remote), Appearance, Index, Remove; the workspace applies a save in `workspace/projects.rs` (`apply_project_settings`) |
| `project_manager.rs` | the Project list page of Settings |
| `settings.rs` | `SettingsModel`, shared by windows of a profile, and the helpers its pages use |
| `settings_window.rs` | the Settings dialog: pages grouped in a searchable list, each page cards of rows (name, description, control) |
| `profiles.rs` | the profile list, `open_window`, profile menus |
| `commit_settings.rs` | the commit message dialog |
| `connections.rs`, `connect_account.rs`, `repository_picker.rs`, `pull_request.rs` | accounts list, the Connect dialog, the repository list of Clone, the Create pull request dialog |
| `updates.rs` | `UpdateState`, `AppUpdate` and the Updates page |
| `menus.rs` | the menu bar and Dock menu |

### Plain data (no window needed)

| File | Owns |
| --- | --- |
| `agents.rs` | `AgentKind` (Codex, Claude), the shell line that runs each, `AgentTabs` |
| `terminal_layout.rs` | `Arrangement`, `Node`, `Direction`: tabs and split panes as data |
| `selection.rs` | terminal selection math (words, extending, arrow keys) |
| `close_order.rs` | the order `⌘W` closes tabs in |
| `labels.rs` | short texts for numbers: "5m ago", sizes |
| `syntax.rs` | extra tree-sitter registrations (Swift, Kotlin) and `kotlin.scm` |
| `theme.rs` | light and dark palettes, fonts, applying them to the Kit theme |
| `components.rs`, `assets.rs` | shared components (soft inputs, the blue "chosen" button); Kit icons |

## Library crates

| Crate | Key items |
| --- | --- |
| **`ws-product`** | `NAME`, `ENV_PREFIX`, `BUNDLE_ID`, `LEGACY_NAME`, `env_name`, `env_var`, `slug` — generated by `build.rs` from `product.toml` |
| **`ws-terminal`** | `Session` (PTY, `alacritty_terminal`, shell directory lookup), `Snapshot`/`Span`/`Cursor` (styled screen), `Palette`, `encode_key`, `encode_mouse`, `paste_bytes`, `dropped_paths`, scrollback `search` |
| **`ws-git`** | See [Git](./git.md): `summary`, `commit`, `amend_commit`, `branch_entries`, `switch_branch_with`, `revert_commit`, `cherry_pick`, `pull`, `push`, `fetch`, `start_merge`, `compare`… |
| **`ws-storage`** | `data_dir`, `write_atomic`/`write_json`/`read_json`, `Preferences`, `ProjectList`, `ProfileList`, `WindowPlacement` |
| **`ws-index`** | `ProjectIndex` (every file the ignore rules let through), `fuzzy::{score, rank}`, `highlight::{Language, Role, Highlights}` |
| **`ws-hosting`** | `Account`, `Accounts`, `TokenStore` (`KeychainStore`, `MemoryStore`), `verify`, `repositories`, `pull_requests`, `create_pull_request`, `RepoRef`, `web_repository` |
| **`ws-commitgen`** | `Config` (`commit.json`), `generate`, `split_diff`, `parse_message`, `system_prompt` |
| **`ws-language`** | `find_server` (the project's own TypeScript server, else `typescript-language-server`), `Session` (`start`, `sync`, `close`), `Update`, `Problem`, `Severity` |
| **`ws-activity`** | `timed`, `record`, `error`, `entries`, `version`, `clear` (the newest 2000 lines are always kept in memory; `start`/`stop` also write a file): lines marked `INFO`, `SLOW` (over 500 ms) or `ERROR`: Git commands, requests, refresh and history reads, the commit message steps, and every error notification |
| **`ws-updater`** | `check`, `prepare`, `install`, `SWAP_SCRIPT`, `Release` |
| **`ws-version`** | `is_newer` |
| **`ws-capture`** | `cargo run -p ws-capture -- window\|screen [--out FILE] [--delay SECONDS]`: saves a PNG (default `<data dir>/captures/`) with macOS `screencapture`; needs Accessibility and Screen Recording access for the terminal |
| **`ws-visual-check`** | `cargo run -p ws-visual-check -- <png> <x> <y> <w> <h>` |

## Finding things quickly

| I want to change… | Start at |
| --- | --- |
| a key binding | `crates/app/src/lib.rs` |
| a menu item | `crates/ui/src/menus.rs` |
| what the branch menu shows | `workspace/render_menus.rs` (rows) and `workspace/branches.rs` (actions) |
| a Git operation | `crates/git/src/` — add it there with a test in `crates/git/tests/`, then wire it in `workspace/` |
| a Settings page | `crates/ui/src/settings_window.rs`, and `crates/storage/src/preferences.rs` for the stored value |
| colors | `crates/ui/src/theme.rs` |
| the product's name, bundle id or environment prefix | `product.toml` (nowhere else) |
| a language's highlighting | `crates/ui/src/syntax.rs`, and `crates/ui/src/editor.rs` (`language_for`) |
| a file gCode saves | `crates/storage/src/` and [Persistence](./persistence.md) |
