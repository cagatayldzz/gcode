# Persistence

Everything Vekro owns lives in one data directory:

```text
~/Library/Application Support/Vekro/          (override with VEKRO_DATA_DIR)
├── profiles.json            list of profiles, the one used last
├── window.json              window placement, shared by all profiles
├── index/<hash>.idx         per-project file index (binary; a cache)
├── projects.json            ┐
├── accounts.json            ├─ the "default" profile keeps these directly here
├── preferences.json         ┘
└── profiles/<id>/           another profile: its own projects.json, accounts.json, preferences.json
```

## Safe writing

Files Vekro owns are written with `vekro_storage::write_json`:

1. the new content goes to a **temporary file beside the target**;
2. it **replaces** the target in one step;
3. the previous file stays as **`<name>.bak`**.

A file that is not valid JSON is set aside as **`<name>.corrupt`** and the backup is read instead. A user's file in the editor is saved with
`write_atomic` — and the first save is refused if the file changed on disk since it was read.

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
| `window.json` | `WindowPlacement` (position, size, maximized or full screen) | half a second after the window stops moving or resizing, and at once when the window closes or the app quits |
| `index/<hash>.idx` | `ProjectIndex` of one project | in the background when a project is opened and its saved index is no longer right |

The index is a **cache**: deleting the folder only costs a new walk of each project. It is checked with one `stat` per folder and ignore
file; `ProjectIndex::open` uses the saved one while nothing changed, else walks the tree in parallel and saves it.

## Files of each profile

| File | Type | Written |
| --- | --- | --- |
| `projects.json` | `ProjectList` (project paths with name, icon, color and category; the categories and whether each is folded; favorites; the active project) | when a project is added or selected, or a category changes |
| `accounts.json` | `Accounts` (service, host, organization and user name; **never a token**) | when an account is connected or disconnected, or an issue is set or cleared |
| `preferences.json` | `Preferences` | on each settings change |

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

```json
{
  "projects": [
    { "path": "/path/to/a", "category": null, "color": "#8b7bff", "icon": "rocket", "favorite": true },
    { "path": "/path/to/b", "category": "category-1", "name": "Billing" }
  ],
  "categories": [{ "id": "category-1", "name": "", "collapsed": false }],
  "active": "/path/to/a"
}
```

A plain list of paths also loads.

## Not stored yet

Dock layout, open files, running terminals. (Window size and position are stored.)

## Environment variables

| Variable | Effect |
| --- | --- |
| `VEKRO_DATA_DIR` | Use another data directory — handy for scratch runs and for screenshots |
| `VEKRO_OPENCOMMIT_CONFIG` | Use another OpenCommit config file |
| `VEKRO_COMMIT_PROVIDER` | Select the Apple Intelligence provider |
| `VEKRO_GIT_USERNAME` / `VEKRO_GIT_PASSWORD` | Internal: how a token reaches the credential helper |
| `VEKRO_DOCS_BASE` | Base path of the built documentation site |
| `VEKRO_BENCH_SAMPLES` | Sample count of the benchmark suite |
