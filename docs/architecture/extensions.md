# Extensions

```text
 Extensions view ─► ws_extensions::fetch_index()   extensions/index.json (this repository, main branch)
   "Install"     ─► ws_extensions::install(&Available)   download manifest ► SHA-256 ► id ► <data dir>/extensions/<id>/manifest.json
   "Uninstall"   ─► ws_extensions::uninstall(id)         remove the folder
```

An **extension** is optional: nothing is installed on a fresh start. The first one is **AI Agent** (Codex and Claude sessions in their own
view). Until it is installed, the AI agents activity button is hidden, and the AI commands (menu, palette, shortcut) open the Extensions
view with a note instead of starting a session.

## What a package is

The app is compiled, so a package **carries no code**: it is a `manifest.json` that switches on a feature that is already in the app and
chooses among what that feature offers. For AI Agent it names the agents to show (`contributes.agents`: `codex`, `claude`). Which command an
agent runs is decided by the app (`AgentKind::shell_line`), **never by a manifest**: a package published online must not choose what a shell
runs. A new agent therefore needs an app release; a new *choice* of agents only needs a new manifest.

## The registry

The registry is this repository: [`extensions/`](https://github.com/cagatayldzz/gcode/tree/main/extensions). `index.json` lists each
extension with its manifest's path and the **SHA-256 of the manifest**. The app reads both from `raw.githubusercontent.com` under this
repository's `extensions/` folder only (`parse_index` drops an entry whose id, path or hash is not plain, so a tampered index cannot point the
app at another place).

| Check on install | Where |
| --- | --- |
| the manifest's SHA-256 is the one in the index | `install_from` |
| the manifest's `id` is the one that was asked for | `install_from` |
| the id is a plain folder name | `parse_manifest`, `uninstall_in` |

A test (`the_published_index_matches_the_manifests_in_the_repository`) reads `extensions/` and installs every listed extension into a scratch
folder, so changing a manifest without updating its hash in `index.json` fails CI.

**To publish a change:** edit `extensions/<id>/manifest.json`, raise its `version`, update `sha256` in `index.json` (`shasum -a 256`). The
Extensions view then offers **Update** for installed copies whose version differs.

## Drafts

An index entry with `"status": "draft"` (and no `package` or `sha256`) is only a sketch: the Marketplace lists it with **Coming soon** and
`install` refuses it. Spotify, YouTube Music, Issues, Actions and Scanning (code analysis, dependency updates, security alerts) are drafts. To make one real, add its `<id>/manifest.json`, replace `status` with `package` and `sha256`,
and build the feature in the app that the manifest switches on.

## In the interface

`ExtensionsPanel` (`extensions.rs`) is the sidebar view behind the Extensions button of the activity bar: a search field, **Installed**,
and **Marketplace** (what the index has that is not installed). The index is read when the view is first shown and on **Refresh**. What is
installed is read from the data folder at start and kept in one global shared by every window, so installing in one window shows in all.
Installing and removing run on background threads; a failure is shown as a notification.

## Rules

- No network on start: the marketplace is read only when the Extensions view is shown.
- Development and release builds behave the same; there is no default extension.
- `ws-extensions` never depends on the interface.
