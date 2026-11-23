# Testing

```text
 crates/<crate>/tests/*.rs      Rust tests — never inline in production files
 scripts/tests/*.sh             shell tests for scripts and workflows
```

## Principles

- Tests live in `crates/<crate>/tests/` with descriptive names that read as a sentence.
- Tests that create files or repositories use **temporary directories** (and remove them).
- Tests that touch the data directory use temporary directories; `GCODE_DATA_DIR` overrides the location.
- Plain-Rust crates are tested **without a window**; that is why logic is kept out of `ui` whenever it can be.

## By crate

| Crate | Style | Examples |
| --- | --- | --- |
| `terminal` | Runs **real programs** (`/bin/sh -c …`) through a PTY | key/mouse encoding tables, scrollback search, session lifecycle |
| `git` | **Real repositories** in temporary directories, including a bare `origin` with two clones | pulls (fast-forward, diverged, conflicting), publish, stash Leave/Bring, revert, cherry-pick, amend |
| `storage` | Temporary data directories | atomic writes, `.bak`/`.corrupt` recovery, profile files |
| `index` | Generated trees | fuzzy ranking, `.env`/`.gitignore`/Dockerfile runs, saved index validity |
| `hosting`, `opencommit`, `updater`, `version` | Pure parsing and fakes (`MemoryStore`) | token scopes, diff splitting, release parsing, checksum matching |
| `ui` | **Headless**: plain-data modules and the Kit's `test-support` | window render, syntax roles, terminal layouts, explorer rows, project search |

### A Git test, as a pattern

```rust
// crates/git/tests/remote_workflow.rs
#[test]
fn pull_merges_when_both_sides_have_new_commits() {
  let remote = Remote::new("diverged");                  // bare origin + `mine` + `other` clones
  commit_file(&remote.other, "b.txt", "b", "teammate");
  git(&remote.other, &["push"]);
  commit_file(&remote.mine, "c.txt", "c", "mine");
  ws_git::fetch(&remote.mine, None, |_| {}).unwrap();

  ws_git::pull(&remote.mine, None, |_| {}).unwrap();

  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (2, 0));         // my commit + the merge commit
}
```

The test drives the library exactly as the interface does, then asserts both the **repository state** and the **user-visible numbers**.

## The window render test

`crates/ui/tests/window_render.rs` opens and draws the **real window headlessly** (GPUI Kit `test-support`). A panic while building or
drawing the interface — which would crash the app at launch — fails `bun run check`. **Add a case there when a change touches what the first
window draws.**

## Syntax tests

`crates/ui/tests/syntax_highlighting.rs` registers the languages and asserts that tokens use the theme's semantic colors in **both** light
and dark themes:

```rust
for (token, role) in [("fun", "keyword"), ("greet", "function"), ("String", "type"), ("// Says hi.", "comment")] {
  let offset = source.find(token).unwrap();
  let expected = theme.style(role).and_then(|style| style.color).unwrap();
  // the highlighted style at `offset` must carry `expected`
}
```

## Shell tests

`scripts/tests/` covers version stamping, release notes, packaging, and workflow guarantees. `scripts/tests/run.sh` runs them all; `bun run
check` and CI both do.

## What runs where

| Where | What |
| --- | --- |
| `bun run check` | shell tests, `cargo fmt --check`, clippy (warnings are errors), tests |
| CI `core` (Linux) | shell tests, format, clippy and tests of the plain-Rust crates |
| CI `interface` (macOS) | `cargo check` of `ui` and `app`, and the headless window render test |
| Release workflow | the whole workspace's clippy and tests on the release runner |

For routine changes: `cargo check --locked -p <package>` and `cargo test --locked -p <package>`, then `bun run check`. None of it bundles or
opens gcode.
