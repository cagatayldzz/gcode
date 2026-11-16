# Language

English for all project-owned identifiers, comments, UI text, logs, tests, docs and scripts. Conversation may use the user's language.

# Formatting

2 spaces, LF, final newline (EditorConfig and `rustfmt.toml` match). Run `cargo fmt --all` after Rust changes. In Rust, one blank line after imports and between items.

# Layout

Workspace: `apps/*` (executables), `crates/*` (libraries). Dependencies: `apps/vekro` → `app` → `ui` → `terminal`, `git`, `storage`. Keep `apps/vekro` a thin entry point; put application logic in `crates/*`. `terminal`, `git` and `storage` never depend on the interface. No `unsafe` (`unsafe_code = "deny"`). Update `docs/architecture.md` when architecture changes (crate boundaries, window layout, files in the data directory).

# Interface

Use GPUI Kit (`gpui-kit`) and its default icons. Reference: <https://gpui-kit.com/llms-full.txt> ("Coding Guides"); read the relevant part before interface work. It wins over this file for GPUI Kit usage. Prefer the Kit's components, in particular `Dock`, `Editor`, `Diff`, `Empty`, `StatusBar` and `ScrollableElement`, over hand-built ones.

- Kit components over hand-built elements; one shared component per repeated pattern.
- Size tokens (`text_sm`, `rounded_md`, `size_5`, `gap_2`, `small()`), not inline `px(...)`. Pixels only for terminal geometry, window size/placement, platform boundaries, and named constants (`FONT_SIZE`, `LEFT_DOCK_WIDTH`) for pixel-only component APIs. The Kit sizes icons and dock parts in `rem`, so never change the theme's `font_size`; set the text size of Vekro's own content with `FONT_SIZE` instead.
- Colors from `cx.theme()` or the palette in `theme.rs` (`colors(cx)`), not raw `rgb`/`hsla` (the terminal palette and `theme.rs` excepted).
- Stable ids from identity (paths, panel ids), never list indexes.
- Keep tasks and subscriptions on their owner; do not detach recurring ones. Retained UI text is `SharedString`. Never store `&mut Window`/`App`/`Context`; store handles.
- No state changes or focus requests in `render`; use handlers, `defer`, `defer_in`.
- One command = one `Action` shared by buttons, menus and keys. Bind keys in `crates/app`.
- Show actionable failures in the UI (a notification), not only logs.
- Speed is a feature: opening a file, switching projects and typing must feel instant. Measure before optimizing (time the step, then the frames after it), never block the interface thread on slow work, and keep dependencies optimized in the dev profile.
- The interface is verified by running it (`bun run dev`); do not claim a visual change works without looking at it.

# Git

`git2` for reading state and creating repositories; the `git` program for `clone`, so the user's own credentials apply. Never run a Git command that changes the user's repository (commit, checkout, reset) from the interface without an explicit user action.

# Privacy

Do not commit screenshots or logs that show the user's name, machine name, home path or terminal history. Use a scratch `VEKRO_DATA_DIR` for any run you intend to show.

# Tests

Rust tests live in the owning package's `tests/` with descriptive names. No inline test modules in production files. Tests that create files or repositories use temporary directories.

# Checks

Run `bun run check` before finishing. Commit or push only when the user asks.
