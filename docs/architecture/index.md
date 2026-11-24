# Architecture overview

gCode is a Cargo workspace of small libraries and one thin executable. The rule that shapes everything:

> **Dependencies flow one way, from the interface down. Nothing below `ui` ever knows the interface exists.**

```text
 apps/desktop          the executable: calls ws_app::run(), nothing else
     │
     ▼
 crates/app          startup · key bindings · quit · Dock menu · first window
     │
     ▼
 crates/ui           every view, built with GPUI Kit  (the only crate that draws)
     │
     ├──► terminal   PTY, terminal emulation, key & mouse encoding
     ├──► git        repository summary, diffs, history, branches, merge, sync
     ├──► storage    data directory, projects, preferences, profiles, window
     ├──► index      project file index, fuzzy ranking, line-based highlighting
     ├──► hosting    GitHub / GitLab / Azure DevOps accounts, repositories, PRs
     ├──► commitgen   commit message generation, commit.json
     ├──► updater ──► version    release lookup, verified self-update
     └──► product    the product's name (generated from product.toml); the lower crates use it too

 crates/visual-check   a standalone development tool; the app does not depend on it
```

`terminal`, `git`, `storage`, `index`, `hosting`, `commitgen`, `updater` and `version` **never depend on the interface**. They contain
no GPUI types, so they are tested without a window and (for the plain-Rust ones) on Linux in CI.

## Design principles

| Principle | What it means in the code |
| --- | --- |
| **Thin entry point** | `apps/desktop/src/main.rs` calls `ws_app::run`. Application logic lives in `crates/*`. |
| **Speed is a feature** | Opening a file, switching projects and typing must feel instant. Slow work (Git, search, indexing, network, Keychain) runs on background tasks; the interface thread only applies results. Measure before optimizing. |
| **Safe by default** | Nothing changes a user's repository (commit, checkout, reset…) without an explicit action. A failed multi-step operation is rolled back: merges, reverts and cherry-picks abort instead of leaving half-done state. |
| **Atomic persistence** | Files gCode owns are written to a temporary file and swapped in; the previous version is kept as `.bak`. |
| **One command, one action** | A command is a single `Action` shared by buttons, menus and keys. Key bindings are made in `crates/app`. |
| **Kit first** | Interface parts come from GPUI Kit (`Dock`, `Editor`, `Diff`, `Empty`, `StatusBar`, `Settings`, `Command`…) instead of hand-built elements. |
| **No `unsafe`** | The workspace sets `unsafe_code = "deny"`. |
| **One name, one place** | The product's name is written in `product.toml` only. Crates, types, files and tests are named without it, so renaming the product is a one-file change (see [Persistence → The product's name](./persistence.md#the-product-s-name)). |
| **Private by default** | Tokens live in the Keychain and travel through environment variables, never through command lines, files or logs. |

## How a user action flows

Take "switch branch" as an example — every feature follows the same shape:

```text
 click on a branch row (render_menus.rs)
   │  workspace.update(cx, |this, cx| this.switch_branch(name, window, cx))
   ▼
 Workspace::switch_branch (workspace/branches.rs)       ← interface thread: decide, maybe ask
   │  changes exist?  → dialog: Leave / Bring / Cancel
   ▼
 cx.background_executor().spawn(async { ws_git::switch_branch_with(...) })
   │                                                    ← background thread: the slow, blocking work
   ▼
 cx.spawn_in(window, async move |this, cx| { … this.update_in(cx, |this, window, cx| { … }) })
   │                                                    ← back on the interface thread
   ▼
 refresh_project()  +  notification  +  cx.notify()      ← state applied, view redrawn
```

Rendering is a pure function of state: **no state changes and no focus requests happen inside `render`**.

## Where to read next

- [Code map](./code-map.md) — what lives where, file by file
- [Window layout](./window-layout.md) — how the window is composed
- [Workspace & state](./workspace.md) — projects, switching, the refresh loop
- [Terminals](./terminal.md) · [Git](./git.md) · [Editor & performance](./editor.md)
- [Hosting & accounts](./hosting.md) · [Commit messages](./commit-messages.md) · [Updates](./updates.md)
- [Persistence](./persistence.md) — every file gCode writes
