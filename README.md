<div align="center">
  <img src="apps/desktop/macos/icon.png" width="88" alt="">
  <h1>gCode</h1>
  <p><strong>A native macOS workspace for code, terminals, Git, and AI agents.</strong></p>
  <p>Built with Rust and GPUI Kit · macOS 15+ · Apple Silicon</p>
  <p>
    <a href="https://github.com/cagatayldzz/gcode/releases/latest"><strong>Download for macOS</strong></a>
    &nbsp;·&nbsp;
    <a href="docs/guide/index.md">User guide</a>
    &nbsp;·&nbsp;
    <a href="#run-from-source">Run from source</a>
  </p>
</div>

![Light mode showing the project explorer and a Rust file open in the editor](docs/images/workspace-light.png)

## Your workspace

| Area | What you can do |
| --- | --- |
| **Editor** | Open files quickly, edit side by side, search and replace across a project, and use JavaScript and TypeScript completion and Quick Fix. [Editor guide](docs/guide/editor.md) |
| **Terminals** | Dock a terminal below the editor or switch to a full terminal workspace. Split panes and run project scripts from the Command Palette. [Terminal guide](docs/guide/terminals.md) |
| **Git** | Review diffs, stage individual hunks, and commit selected changes. Compare branches, rebase, and manage linked worktrees. [Git guide](docs/guide/git-changes.md) |
| **AI agents** | Run Codex and Claude in separate Git worktrees. Review each session’s changes against its starting commit and open its Git panel to commit. [Agent guide](docs/guide/agents.md) |
| **Projects** | Create, clone, or import projects. Group and star them, then switch between projects while their terminals keep running. [Project guide](docs/guide/projects.md) |
| **Accounts** | Connect GitHub, GitLab, or Azure DevOps to browse repositories and work with pull requests. [Accounts guide](docs/guide/accounts.md) |

![Dark mode showing changed files, commit controls, and an inline Git diff](docs/images/workspace-dark.png)

## Start with the keyboard

| Shortcut | Action |
| --- | --- |
| `⇧⌘P` | Open the Command Palette |
| `⌘P` | Open a project file |
| `⌘J` | Toggle the terminal |
| `⇧⌘A` | Toggle the AI view |

See the [full shortcut list](docs/guide/shortcuts.md).

## Run from source

On Apple Silicon with macOS 15 or later, install the Xcode Command Line Tools, stable Rust, Bun, and Git. [Development setup](docs/development/index.md)

```sh
git clone https://github.com/cagatayldzz/gcode.git
cd gcode
bun install
bun run dev
```

[Architecture](docs/architecture/index.md) · [Development](docs/development/index.md) · [Contributor conventions](AGENTS.md)
