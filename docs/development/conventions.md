# Conventions

The rules below are the contributor's contract. `AGENTS.md` in the repository root is the authoritative short version and is read by AI
agents; this page explains the *why*.

## Language and style

- **English** for all project-owned identifiers, comments, interface text, logs, tests, docs and scripts.
- 2 spaces, LF, final newline (`.editorconfig` and `rustfmt.toml` match). Run `cargo fmt --all` after Rust changes.
- **No `unsafe`** (`unsafe_code = "deny"`).
- A public project: do **not** name other applications in comments, tests or docs. Describe the behavior itself.
- **Never write the product's name in code**: not in identifiers, crate names, file names, strings or tests. It lives in `product.toml`; Rust reads it through `ws-product` (`ws_product::NAME`, `env_name("DATA_DIR")`) and scripts through `scripts/product.sh`. Comments say "the app".

## Layout rules

```text
 apps/desktop ─► app ─► ui ─► terminal, git, storage, index, hosting, opencommit, updater
```

- Keep `apps/desktop` a thin entry point; put application logic in `crates/*`.
- `terminal`, `git`, `storage`, `index` (and the other library crates) **never depend on the interface**.
- Update the documentation when the architecture changes: crate boundaries, the window layout, the files in the data directory
  (`docs/architecture/`).

## Interface rules (GPUI Kit)

Use GPUI Kit (`gpui-kit`) and its default icons. Prefer the Kit's components, in particular `Dock`, `Editor`, `Diff`, `Empty`, `StatusBar`
and `ScrollableElement`, over hand-built ones. Read the relevant part of the Kit's coding guides before interface work.

| Do | Don't |
| --- | --- |
| Kit components; one shared component per repeated pattern | Hand-built elements, copy-pasted rows |
| Size tokens: `text_sm`, `rounded_md`, `size_5`, `gap_2`, `small()` | Inline `px(...)` |
| Pixels only for terminal geometry, window size/placement, platform boundaries, and **named constants** (`FONT_SIZE`, `LEFT_DOCK_WIDTH`) | Magic numbers |
| Colors from `cx.theme()` or `colors(cx)` in `theme.rs` | Raw `rgb` / `hsla` (the terminal palette and `theme.rs` excepted) |
| Stable ids from identity (paths, panel ids) | Ids from list indexes |
| Keep tasks and subscriptions on their owner | Detached recurring tasks |
| Retained text as `SharedString`; store handles | Storing `&mut Window` / `App` |
| State changes and focus requests in handlers, `defer`, `defer_in` | State changes or focus requests inside `render` |
| One command = one `Action` shared by buttons, menus and keys; bind keys in `crates/app` | Parallel code paths per entry point |
| Show actionable failures in the interface (a notification) | Only logging errors |
| Never change the theme's `font_size` (the Kit sizes icons and dock parts in `rem`) — set gcode's own text size with `FONT_SIZE` | Rescaling the theme |

### Speed is a feature

Opening a file, switching projects and typing must feel **instant**. Measure before optimizing (time the step, then the frames after it),
never block the interface thread on slow work, and keep dependencies optimized in the dev profile.

## Git rules

- `git2` for reading state and creating repositories; the `git` program for `clone` and everything that needs the user's credentials.
- **Never run a Git command that changes the user's repository (commit, checkout, reset…) from the interface without an explicit user
  action.** An operation that can fail halfway must roll back (abort the merge, revert, cherry-pick) and say that nothing changed.

## Privacy

Do not commit screenshots or logs that show a user's name, machine name, home path or terminal history. Use a scratch `GCODE_DATA_DIR` for
any run you intend to show. Tokens are never written to files or logs.

## Tests

- Rust tests live in the owning package's `tests/` with **descriptive names** (`pull_merges_when_both_sides_have_new_commits…`). No inline
  test modules in production files.
- Tests that create files or repositories use **temporary directories**.
- See [Testing](./testing.md).

## Adding a feature — a checklist

```text
 1. Logic that can be tested without a window?   → put it in a library crate, with a test in its tests/
 2. A user-visible command?                      → one Action; bind the key in crates/app; add it to menus.rs if it belongs there
 3. Slow or blocking work?                       → background executor; apply the result with update_in on the interface thread
 4. Can it fail?                                 → a notification with a sentence the user can act on
 5. Can it change the user's data?               → only on an explicit action; roll back on failure
 6. New view or list?                            → Kit component; virtualize long lists
 7. Touched crate boundaries / layout / files?   → update docs/architecture/
 8. Done?                                        → cargo fmt --all, then bun run check
```

## Commits

Commits use **Conventional Commits** (`feat:`, `fix:`, `docs:`, `chore:`…). The release workflow derives the next version from them:
breaking change (`!:` or `BREAKING CHANGE:`) is major, `feat` is minor, everything else is patch. Commit or push only when you mean to.
