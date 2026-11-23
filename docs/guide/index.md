# Getting started

Vekro is a native macOS window that keeps **projects, a code editor, terminals and Git** together. One window shows one
project at a time, and switching projects never stops the shells of the others.

::: tip Requirements
macOS 15 or later on Apple Silicon. Intel Macs, Linux and Windows are not supported.
:::

## Install

Download the latest `Vekro-<tag>-macos-arm64.zip` from [GitHub Releases](https://github.com/cagatayldzz/vekro/releases),
unzip it and move `Vekro.app` to Applications. Releases are signed and notarized, and Vekro can update itself
(see [Settings → Updates](./settings.md#updates)).

To run it from source instead, see [Development → Setup](../development/index.md):

```sh
bun install
bun run dev
```

## The window at a glance

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ ● ● ●  [📁▾]  │ vekro ●  │ wfhub │ appforge │ +              title bar     │
├────┬───────────────────┬─────────────────────────────────────────────────┤
│ ▤  │  SIDEBAR          │  MAIN AREA                                      │
│ 🔍 │  ───────────────  │  ┌ main.rs ─┬ lib.rs ─┬ …           editor tabs  │
│ ⑂  │  Explorer         │  │                                              │
│ ›_ │    src/           │  │   fn main() {                                │
│ 📁 │      main.rs      │  │       …                                      │
│    │  Search           │  │   }                                          │
│    │  Git (changes /   │  │                                              │
│    │       history)    │  ├──────────────────────────────────────────────┤
│ D  │                   │  │ main · 3 changed     ~/dev/vekro   Ln 12, Col 5│
└────┴───────────────────┴──┴──────────────────────────────────────────────┘
 activity bar   sidebar          main area                   status bar
```

| Part | What it does |
| --- | --- |
| **Title bar** | The project switcher (a menu of every project, grouped by category), your favorite projects as tabs, and the `+` button to add a project. |
| **Activity bar** | A narrow strip with one button per view: Explorer, Search, Git (with a badge counting changed files), Terminals, AI agents, and project settings. Your profile's avatar sits at the bottom. |
| **Sidebar** | The view the activity bar chose. It is plain workspace content, 200–960 px wide, and you can drag its edge. |
| **Main area** | Editor tabs. In the Git view it shows the diff of the chosen file or commit instead. Terminals and AI agents take over the whole area right of the activity bar while they are shown. |
| **Status bar** | Branch and changed-file count on the left, the project folder in the middle, the cursor position, indentation, encoding, line ending and language on the right. |

## Your first five minutes

1. Press `+` in the title bar and **Import** a folder, **Clone** a repository, or create a **New** one
   ([Projects](./projects.md)).
2. Open a file from the Explorer. `⌘P` jumps to any file by typing a few letters
   ([Search & navigation](./search.md)).
3. Press `⌘J` for a terminal in the project folder ([Terminals](./terminals.md)).
4. Edit something and open the Git view: tick files, write a summary and commit
   ([Changes](./git-changes.md)).
5. Press `⇧⌘P` any time to search every command.

## Where to go next

- [Projects](./projects.md) — adding, grouping, favorites, profiles
- [Explorer & editor](./editor.md) — files, tabs, previews, minimap
- [Search & navigation](./search.md) — Go to File, command palette, project search and replace
- [Terminals](./terminals.md) and [AI agents](./agents.md)
- Git: [Changes & commits](./git-changes.md) · [History](./git-history.md) · [Branches, sync & merging](./git-branches.md)
- [Accounts](./accounts.md) — GitHub, GitLab, Azure DevOps
- [Settings](./settings.md) · [Keyboard shortcuts](./shortcuts.md)
