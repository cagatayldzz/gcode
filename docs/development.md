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
| `bun run check` | script syntax and shell tests, `cargo fmt --check`, clippy with warnings as errors, tests (run before finishing a change) |
| `bun run test` | shell tests, then `cargo test --locked --workspace` |
| `bun run update` | check Cargo manifests for dependency upgrades, then refresh `Cargo.lock` (requires `cargo-edit`) |
| `bun run release` | request a release: dispatches the Release workflow for the pushed default branch (needs `gh`) |
| `bun run icon` | redraw `AppIcon.icns` and `DevIcon.icns` from `apps/vekro/macos/icon.svg` (needs Google Chrome) |
| `bun run commit` | `git add -A && oco --yes` (needs [OpenCommit](https://github.com/di-sukharev/opencommit)) |

`bun run update` runs `cargo upgrade --incompatible` followed by `cargo update`. If the manifest
requirements already cover the available versions, only `Cargo.lock` may change. Bun runs the
project commands, and this workspace has no JavaScript package dependencies.

## Scripts

Automation uses only shell and Rust; there is no TypeScript or other runtime in scripts or workflows.

| Path | Purpose |
| --- | --- |
| `scripts/app.sh` | build, bundle, sign, launch, check (`--dev`, `[debug\|release] --build`, `--check`, `--icon`) |
| `scripts/release.sh` | request a release; in CI: `prepare` (next version, stamping), `notes`, `package`, `finalize` (assets, checksums) |
| `scripts/notarize-app.sh` | sign with Developer ID and notarize `dist/Vekro.app` (macOS tools, release secrets) |
| `scripts/tests/*.sh` | shell tests, run by `scripts/tests/run.sh` (version, notes, packaging, workflow guarantees) |

## Releases

`bun run release` (or running the Release workflow by hand) releases the pushed default branch. The version comes
from Conventional Commits since the latest `v*` tag: a breaking change (`!:` or `BREAKING CHANGE:`) is major,
`feat` is minor, everything else is patch. The workflow then:

1. **Validate**: Apple secrets are present, shell tests, `cargo fmt`, clippy and tests of `vekro-terminal`, `vekro-git`, `vekro-storage`.
2. **Prepare**: choose the version, stamp `Cargo.toml`, `Cargo.lock`, `package.json` and `Info.plist` in the CI checkout, write `RELEASE_NOTES.md` (commit titles grouped as New Features, Bug Fixes, Documentation, Chores, with a changelog link).
3. **Build** (one macOS job): release build, Developer ID signature, notarization, `Vekro-<tag>-macos-arm64.zip`, then workspace clippy and tests on the same runner.
4. **Verify**: exactly the expected assets, none empty, `SHA256SUMS.txt`.
5. **Publish**: checksums verified, draft GitHub release, then published as latest.

Required Actions secrets: `APPLE_CERTIFICATE_BASE64`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID`,
`APPLE_APP_SPECIFIC_PASSWORD`. The version in the repository is stamped only in CI; the tag is the source of truth.
`CI` (`.github/workflows/ci.yml`) runs the shell tests, formatting, and clippy and tests of the three plain-Rust crates on Linux.

## Packages

| Package | Kind | Purpose |
| --- | --- | --- |
| `vekro` | app | executable entry point (binary `Vekro`) |
| `vekro-app` | lib | startup, window, key bindings |
| `vekro-ui` | lib | views built with GPUI Kit |
| `vekro-terminal` | lib | PTY, emulation, key and mouse encoding |
| `vekro-git` | lib | repository summary, file diffs, creating and cloning projects |
| `vekro-storage` | lib | data directory, project list, preferences |
| `vekro-opencommit` | lib | commit message generation, `~/.opencommit` settings |
| `vekro-hosting` | lib | GitHub, GitLab and Azure DevOps accounts, repositories and pull requests |
| `vekro-version` | lib | version comparison for update detection |
| `vekro-updater` | lib | self-update from GitHub releases |
| `vekro-visual-check` | tool | inspect screenshot pixels and map them to window coordinates |

Code map and data flow: [architecture](architecture.md).

## Tests

Tests live in `crates/<crate>/tests/`, never inline. Terminal session tests run real programs
(`/bin/sh -c …`) through a PTY. Git tests create repositories in temporary directories. Tests that touch
the data directory use temporary directories; `VEKRO_DATA_DIR` overrides the location.

Shell tests for the scripts and workflows live in `scripts/tests/`.

For routine changes, run `cargo check --locked -p <package>` and `cargo test --locked -p <package>` for
the affected package, then `bun run check` for the workspace. These checks run without bundling or opening
Vekro. Use `bun run dev` when visual inspection is requested.

## Visual inspection with a computer-use agent

When explicitly requested, run `bun run dev` with a scratch `VEKRO_DATA_DIR`. Use the agent's computer-use tool to select the development app by bundle ID
`com.cagatayldzz.vekro.dev`, inspect its accessibility tree, and take a fresh window screenshot. Prefer accessible controls; use
coordinates from the current screenshot when a control is not exposed. After each action, inspect the new state before continuing.
The operating system may require Screen Recording and Accessibility permissions for the computer-use host.

For precise color or coordinate inspection of a saved PNG, run:

```sh
cargo run -p vekro-visual-check -- /tmp/vekro-window.png 200 100 800 600
```

This prints the image dimensions, the RGBA value at image pixel `(200, 100)`, and its center in an 800 × 600 logical-point
window. The conversion assumes the PNG contains exactly the window, without a shadow or cropping. It returns coordinates relative
to that window; add the window's screen origin if the computer-use tool expects screen coordinates. Window movement, display
scaling, and screenshots that include decorations or shadows change the mapping: capture and measure again after those changes.
The tool does not capture the screen or send input; use the computer-use tool for those actions. Screenshots can contain private
paths or terminal history, so keep them in a scratch location and do not commit them.

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
