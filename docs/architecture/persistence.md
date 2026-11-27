# Persistence

Everything gCode owns lives in one data directory:

```text
~/Library/Application Support/gCode/          (override with GCODE_DATA_DIR)
├── profiles.json            list of profiles, the one used last
├── commit.json              commit message generation settings (mode 0600, holds the API key)
├── activity.log             only when `ws_activity::start` is called; the bug button's panel reads the log from memory instead
├── logs/crash-<date>-<time>-<pid>.log   one report per crash (a panic), the newest 20 stay; see Crash reports
├── window.json              window placement, shared by all profiles
├── recent.json              files edited and projects opened last (with their profile), shared by all profiles; feeds the Dock menu
├── index/<hash>.idx         per-project file index (binary; a cache)
├── extensions/<id>/manifest.json   an installed extension (see Extensions); shared by all profiles
├── projects.json            ┐
├── accounts.json            ├─ the "default" profile keeps these directly here
├── preferences.json         ┘
├── editor-recovery/         private unsaved editor snapshots for the default profile
└── profiles/<id>/           another profile: its own projects.json, accounts.json, preferences.json, editor-recovery/
```

## Crash reports

`ws_app::run` calls `ws_storage::install_panic_hook` first. A panic anywhere (a click handler, a background task) writes a report to
`logs/crash-<UTC date>-<UTC time>-<pid>.log` before the process ends, then reaches the usual error output. A panic inside a GPUI event handler
aborts the app, so the file is the only trace left. A report holds the time, app version, thread, source location, panic message and a
backtrace. The home folder is written as `~`; read a report before sharing it, since a message can still contain project names or file paths.
Only the newest 20 reports are kept (`KEPT_REPORTS`). Open the folder with `open "$HOME/Library/Application Support/gCode/logs"`.

## Safe writing

Files gCode owns are written with `ws_storage::write_json`:

1. the new content goes to a **temporary file beside the target**;
2. it **replaces** the target in one step;
3. the previous file stays as **`<name>.bak`**.

A file that is not valid JSON is set aside as **`<name>.corrupt`** and the backup is read instead. A user's file in the editor is saved with
`write_if_current`, which rechecks the approved disk version after preparing the temporary file.

## Profiles

A **profile** (`Profile`, `ProfileList`) is a separate set of projects, settings and connected accounts, like a browser profile. Each window
shows one profile (`Workspace.profile`); windows of the same profile share one `SettingsModel` (`SettingsRegistry`), windows of different
profiles do not.

- `open_window` refuses a second window for the same profile, so no path can open one.
- The **default** profile (`default`) keeps its files directly in the data directory, so nothing moved when profiles were added.
- A profile with an open window cannot be deleted. Deleting removes its files (`delete_profile_files`) and its Keychain entries.

## Files shared by every profile

| File | Type | Written |
| --- | --- | --- |
| `profiles.json` | `ProfileList` (id and name of each profile, the one used last) | when a profile is added, renamed or deleted, and when a window of another profile comes to the front |
| `recent.json` | `RecentList` (the 20 files saved last and the 20 projects opened last, each with the id of its profile) | when a file is saved or a project is opened, only if the order changed; entries of a removed project or deleted profile are dropped |
| `commit.json` | `ws_commitgen::Config` (provider, API key, model, language, style, push after commit) | when the commit message settings are saved |
| `window.json` | `WindowPlacement` (position, size, maximized or full screen) | half a second after the window stops moving or resizing, and at once when the window closes or the app quits |
| `index/<hash>.idx` | `ProjectIndex` of one project | in the background when a project is opened and its saved index is no longer right |

The index is a **cache**: deleting the folder only costs a new walk of each project. It is checked with one `stat` per folder and ignore
file; `ProjectIndex::open` uses the saved one while nothing changed, else walks the tree in parallel and saves it.

## Files of each profile

| File | Type | Written |
| --- | --- | --- |
| `projects.json` | `ProjectList` (project paths with name, icon, color and category; the categories and whether each is folded; favorites and each project's starred branches; the active project) | when a project is added or selected, or a category changes |
| `accounts.json` | `Accounts` (service, host, organization and user name; **never a token**) | when an account is connected or disconnected, or an issue is set or cleared |
| `preferences.json` | `Preferences` | on each settings change |
| `editor-recovery/<path-hash>.json` | Unsaved text, file path and original disk version | background checkpoints about every 500 ms while changes are pending, and before normal quit |

Recovery snapshots contain the draft text. They are written atomically with file mode 0600 in a directory created with mode 0700.
Saving or explicitly discarding a draft queues its removal. Restarting a profile restores its drafts in the editor, without writing to
their original files. Force quit or a crash restores only changes that reached a checkpoint.

### `preferences.json`

```json
{
  "theme": "system",
  "projects_dir": null,
  "code_font_size": 13,
  "explorer_font_size": 14,
  "commit_instructions": null,
  "check_updates": true,
  "activity_order": [],
  "activity_hidden": []
}
```

### `projects.json`

Each project's optional `editor` object stores `tab_size`, `hard_tabs`, `line_endings` (`preserve`, `lf` or `crlf`), and
`format_on_save`. Older entries default to two spaces, preserving line endings, and manual formatting.

```json
{
  "projects": [
    { "path": "/path/to/a", "category": null, "color": "#8b7bff", "icon": "rocket", "favorite": true, "favorite_branches": ["develop"], "remote_url": "git@example.com:o/a.git" },
    { "path": "/path/to/b", "category": "category-1", "name": "Billing" }
  ],
  "categories": [{ "id": "category-1", "name": "", "collapsed": false }],
  "active": "/path/to/a"
}
```

A plain list of paths also loads.

`remote_url` is the address of the repository as it was last seen (read from the open project's remote). It is what **Clone Again** uses when
the project's folder is deleted outside the app: such a project stays in the list, and opening it shows "Can't find …" with **Locate…** (points the
project at another folder, `ProjectList::relocate`), **Clone Again** (when an address is known) and **Remove**. The folders are checked in the
background on every refresh round (`Workspace::sync_projects`); a project whose folder returns is shown again.

## Not stored yet

Dock layout, open files, running terminals. (Window size and position are stored.)

## The product's name

The name appears in `product.toml`, nowhere else in the code:

```toml
name = "gCode"                       # the app bundle, executable, Application Support folder, Keychain service, user agent
env_prefix = "GCODE"                 # <prefix>_DATA_DIR and friends
bundle_id = "com.cagatayldzz.gcode"  # the development build adds ".dev"
legacy_name = "…"                    # the previous name, read only by the first-start migration
```

- **Rust** reads it through `ws-product` (constants generated by its `build.rs`, so a missing key fails the build).
- **Scripts and workflows** read it with `scripts/product.sh <key>`; `Info.plist` holds `@NAME@` and `@BUNDLE_ID@` placeholders that the bundle step fills in.
- The Cargo executable is called `ws-desktop`; the bundle step copies it to `Contents/MacOS/<name>`.

### Moving data from the previous name

When the product's name changes, the first start moves what the old name saved, so nobody loses projects, settings or accounts:

| What | How it moves |
| --- | --- |
| The data folder | `ws_storage::migrate_legacy_data_dir` renames `Application Support/<old>` to `Application Support/<new>` once: only when the new folder does not exist yet and the old one does, and never when `<prefix>_DATA_DIR` names the folder. Called by `ws_ui::init` before anything reads. |
| Keychain tokens | `KeychainStore::get` looks under the new service name first, then the old one; a token found there is saved under the new name and removed from the old one. |

Delete `legacy_name` and the two fallbacks once nobody has the old version any more. A new name also changes the bundle identifier, so an installed old build cannot update itself into the new one: it is installed again by hand, once.

## Environment variables and config keys

| Variable | Effect |
| --- | --- |
| `GCODE_DATA_DIR` | Use another data directory — handy for scratch runs and for screenshots |
| `WS_GIT_USERNAME` / `WS_GIT_PASSWORD` | Internal: how a token reaches the credential helper |
| `DOCS_BASE` | Base path of the built documentation site |
| `GCODE_BENCH_SAMPLES` | Sample count of the benchmark suite |

The prefix of the `GCODE_*` variables is `env_prefix` in `product.toml`.
