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

Seed `VEKRO_DATA_DIR/projects.json` to start with chosen projects:

```json
{ "projects": ["/path/to/a", "/path/to/b"], "active": "/path/to/a" }
```

## Formatting

2 spaces, LF, final newline, `max_width = 150`. `rustfmt.toml` and `.editorconfig` hold the rules; run
`cargo fmt --all`.
