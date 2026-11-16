# Development

## Platform

Supported and tested: macOS 15+ on Apple Silicon. Intel, Linux and Windows are unsupported. The minimum
system version is `LSMinimumSystemVersion` in `apps/vekro/macos/Info.plist` (`scripts/app.sh` links
against it).

Requirements: Command Line Tools (not Xcode), Rust 1.92+ (stable), Bun, and `git` on the path (cloning
runs it). After cloning run `bun install`.

## Commands

| Command | Does |
| --- | --- |
| `bun run dev` | debug build, launch `dist/Vekro.app` |
| `bun run build` | release build → `dist/Vekro.app` |
| `bun run check` | `cargo fmt --check`, clippy with warnings as errors, tests (run before finishing a change) |
| `bun run test` | `cargo test --locked --workspace` |
| `bun run icon` | redraw `AppIcon.icns` and `DevIcon.icns` from `apps/vekro/macos/icon.svg` (needs Google Chrome) |
| `bun run commit` | `git add -A && oco --yes` (needs [OpenCommit](https://github.com/di-sukharev/opencommit)) |

## Packages

| Package | Kind | Purpose |
| --- | --- | --- |
| `vekro` | app | executable entry point (binary `Vekro`) |
| `vekro-app` | lib | startup, window, key bindings |
| `vekro-ui` | lib | views built with GPUI Kit |
| `vekro-terminal` | lib | PTY, emulation, key and mouse encoding |
| `vekro-git` | lib | repository summary, file diffs, creating and cloning projects |
| `vekro-storage` | lib | data directory, project list, preferences |

Code map and data flow: [architecture](architecture.md).

## Tests

Tests live in `crates/<crate>/tests/`, never inline. Terminal session tests run real programs
(`/bin/sh -c …`) through a PTY. Git tests create repositories in temporary directories. Tests that touch
the data directory use temporary directories; `VEKRO_DATA_DIR` overrides the location.

The interface has no automated tests; check a change by running `bun run dev`.

## Running the app in isolation

Use a scratch data directory so your own project list stays out:

```sh
VEKRO_DATA_DIR=/tmp/vekro-data ./target/debug/Vekro
```

Seed `VEKRO_DATA_DIR/projects.json` to start with chosen projects (a plain list of paths also loads):

```json
{
  "projects": [
    { "path": "/path/to/a", "category": null, "color": "#8b7bff", "icon": "rocket" },
    { "path": "/path/to/b", "category": "category-1", "name": "Billing" }
  ],
  "categories": [{ "id": "category-1", "name": "", "collapsed": false }],
  "active": "/path/to/a"
}
```

## README screenshots

The images in `docs/images` show a staged demo project ("atlas") so that no real name, machine name, path or
terminal history appears. To redo them:

1. Make a scratch home with a few small Git repositories (one with uncommitted changes) in `~/Developer`.
2. Put a `.zshrc` in a scratch `ZDOTDIR` that clears the screen and sets a plain prompt such as `atlas main $ `,
   and a `projects.json` and `preferences.json` (`{"theme":"dark"}` or `"light"`) in a scratch `VEKRO_DATA_DIR`.
3. Run `HOME=<scratch home> ZDOTDIR=<scratch zsh> VEKRO_DATA_DIR=<scratch data> ./target/debug/Vekro`, size the window
   to about 1040×660, open the file or diff to show, then capture only the window without its shadow
   (`screencapture -o -l <window id>`).

Check every image for names, paths and dates before committing it; the status bar shows the home folder as `~`.

## Formatting

2 spaces, LF, final newline, `max_width = 150`. `rustfmt.toml` and `.editorconfig` hold the rules; run
`cargo fmt --all`.
