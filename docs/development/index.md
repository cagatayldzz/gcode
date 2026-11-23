# Setup & commands

## Platform

Supported and tested: **macOS 15+ on Apple Silicon**. Intel, Linux and Windows are unsupported (the plain-Rust crates are also built and
tested on Linux in CI). The minimum system version is `LSMinimumSystemVersion` in `apps/vekro/macos/Info.plist`; `scripts/app.sh` links
against it.

Requirements:

| Tool | Why |
| --- | --- |
| Command Line Tools (not Xcode) | compiler and linker |
| Rust 1.92+ (stable) | the workspace's `rust-version` |
| Bun | scripts and the documentation site |
| `git` on the `PATH` | cloning and network Git operations run it |

```sh
git clone https://github.com/cagatayldzz/vekro.git
cd vekro
bun install
bun run dev          # debug build, launches dist/Vekro.app
```

## Commands

| Command | Does |
| --- | --- |
| `bun run dev` | debug build, launch `dist/Vekro.app` |
| `bun run build` | release build → `dist/Vekro.app` |
| `bun run check` | script syntax and shell tests, `cargo fmt --check`, clippy with warnings as errors, tests — **run before finishing a change** |
| `bun run test` | shell tests, then `cargo test --locked --workspace` |
| `bun run benchmark` | the performance suite (see [Performance](./performance.md)) |
| `bun run update` | `cargo upgrade --incompatible` then `cargo update` (needs `cargo-edit`) |
| `bun run release` | request a release: dispatches the Release workflow for the pushed default branch (needs `gh`) |
| `bun run icon` | redraw `AppIcon.icns` and `DevIcon.icns` from `apps/vekro/macos/icon.svg` (needs Google Chrome) |
| `bun run docs:dev` / `docs:build` / `docs:preview` | the documentation site |
| `bun run commit` | `git add -A && oco --yes` (needs [OpenCommit](https://github.com/di-sukharev/opencommit)) |

### A routine change

```sh
cargo check --locked -p vekro-git          # the package you touched
cargo test  --locked -p vekro-git
cargo fmt --all
bun run check                              # the whole workspace, no app bundle, no window
```

These checks run **without bundling or opening Vekro**. Use `bun run dev` only when you need to look at the interface.

### Dependencies

`bun run update` runs `cargo upgrade --incompatible` followed by `cargo update`. Direct Cargo dependencies use full version requirements, so
compatible upgrades can also raise their minimum versions in the manifests. When only indirect dependencies have updates, only `Cargo.lock`
changes. The VitePress dependency is managed separately through Bun.

## Packages

| Package | Kind | Purpose |
| --- | --- | --- |
| `vekro` | app | executable entry point (binary `Vekro`) |
| `vekro-app` | lib | startup, window, key bindings |
| `vekro-ui` | lib | views built with GPUI Kit |
| `vekro-terminal` | lib | PTY, emulation, key and mouse encoding |
| `vekro-git` | lib | repository summary, diffs, history, branches, merge, sync, creating and cloning projects |
| `vekro-storage` | lib | data directory, projects, preferences, profiles |
| `vekro-opencommit` | lib | commit message generation, `~/.opencommit` settings |
| `vekro-hosting` | lib | GitHub, GitLab and Azure DevOps accounts, repositories and pull requests |
| `vekro-version` | lib | version comparison for update detection |
| `vekro-updater` | lib | self-update from GitHub releases |
| `vekro-index` | lib | project file index, fuzzy ranking and highlighting of `.env`, `.gitignore` and Dockerfiles |
| `vekro-visual-check` | tool | inspect screenshot pixels and map them to window coordinates |

Code map: [Architecture → Code map](../architecture/code-map.md).

## Scripts

App and release automation use shell and Rust. The documentation workflow uses Bun and VitePress.

| Path | Purpose |
| --- | --- |
| `scripts/app.sh` | build, bundle, sign, launch, check (`--dev`, `[debug\|release] --build`, `--check`, `--icon`) |
| `scripts/release.sh` | request a release; in CI: `prepare` (next version, stamping), `notes`, `package`, `finalize` (assets, checksums) |
| `scripts/notarize-app.sh` | sign with Developer ID and notarize `dist/Vekro.app` (macOS tools, release secrets) |
| `scripts/profile-vekro.sh` | sample a running Vekro's CPU and memory ([Performance](./performance.md)) |
| `scripts/tests/*.sh` | shell tests, run by `scripts/tests/run.sh` (version, notes, packaging, workflow guarantees) |

## Running the app in isolation

Use a **scratch data directory** so your own project list stays out:

```sh
VEKRO_DATA_DIR=/tmp/vekro-data ./target/debug/Vekro
```

Seed `VEKRO_DATA_DIR/projects.json` to start with chosen projects (a plain list of paths also loads); see the format in
[Persistence](../architecture/persistence.md#projects-json).

## Formatting

2 spaces, LF, final newline, `max_width = 150`. `rustfmt.toml` and `.editorconfig` hold the rules; run `cargo fmt --all`. In Rust: one blank line
after imports and between items.
