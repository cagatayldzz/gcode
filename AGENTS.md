# Vekro

One window for projects, a code editor, a terminal and Git. Rust, GPUI Kit (`gpui-kit` with `gpui-fast`).

# Language

English for all project-owned identifiers, comments, UI text, logs, tests and docs. Conversation may use the user's language.

# Layout

`apps/vekro` (thin entry point) → `crates/app` → `crates/ui` → `crates/terminal`, `crates/storage`. `terminal` and `storage` never depend on the interface. No `unsafe`.

# Interface

Kit components, size tokens, colors from `cx.theme()`, stable ids from identity, one `Action` per command, no state changes in `render`. Reference: <https://gpui-kit.com/llms-full.txt>.

# Formatting and tests

2 spaces, LF, `cargo fmt --all`. Tests live in the owning crate's `tests/`, never inline. Run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` before finishing.
