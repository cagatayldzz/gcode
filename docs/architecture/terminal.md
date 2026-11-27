# Terminals

The terminal stack has two halves with a clean seam between them: **`ws-terminal`** (no interface) runs the shell and models the
screen; **`Terminal`** in `crates/ui/src/terminal.rs` draws it and turns input into bytes.

```text
 ┌──────────── crates/ui ───────────┐      ┌────────── crates/terminal ──────────┐
 │ Terminal (a GPUI view)           │      │ Session                              │
 │   keys ──► encode_key ───────────┼─────►│   PTY  ◄──►  login shell             │
 │   mouse ─► encode_mouse ─────────┼─────►│   alacritty_terminal  (the screen)   │
 │   paste ─► paste_bytes           │      │                                      │
 │                                  │◄─────┼── SessionEvent ◄── reader thread     │
 │   render ─► Session::snapshot() ◄┼──────│   Snapshot { spans, cursor, … }      │
 └──────────────────────────────────┘      └──────────────────────────────────────┘
```

## Data flow

1. `Terminal::start` opens a PTY and the login shell through `Session::spawn`; a thread feeds the shell's output to `alacritty_terminal`.
2. The thread reports `SessionEvent` — `Wakeup`, `Title`, `Directory`, `ClipboardStore`, `Exit` — by callback into a channel. `Terminal`
   receives them in a task (`_events_task`), **coalescing bursts of wakeups**, and calls `cx.notify()`.
3. Each render fits the PTY to the panel (`Session::resize`) and takes a `Snapshot`: styled spans, resolved RGB colors and the cursor.
   `Session::modes` tells the view whether a program wants mouse, alternate-screen or focus reports.
4. Keys go through `encode_key` to the PTY. `⌘C`, `⌘V` and `⌘K` are handled by the view itself.
5. On Unix, each login shell's activity task checks the PTY's **foreground process group** off the interface thread. The workspace updates
   the project tab and the Terminal button when a foreground job takes or returns control of the PTY.

## Why it is split this way

| Choice | Reason |
| --- | --- |
| Emulation in a library with no GPUI | The tests run **real programs** (`/bin/sh -c …`) through a PTY without opening a window. |
| `Snapshot` instead of sharing the grid | The view reads a small immutable value per frame; the reader thread never waits on drawing. |
| Coalesced wakeups | A program printing thousands of lines produces one redraw per frame, not one per chunk. |
| Key/mouse encoding as pure functions | `encode_key(key, text, mods, app_cursor)` and `encode_mouse(event, sgr)` are table-testable. |

## Selection and mouse

Selection math (words, paths, lines, extending with the arrow keys) is plain data in `crates/ui/src/selection.rs`, tested without a
window. Programs that follow the mouse receive clicks, drags and the wheel; holding `⇧` selects text instead. Copying joins lines that only
wrapped at the screen edge.

## Agents are terminals

An AI agent session is simply a `Terminal` whose shell line is `$SHELL -l -i -c "exec codex"` (or `claude`), so the user's PATH finds the
agent. See [Window layout → The AI view](./window-layout.md#the-ai-view).


## Project scripts

`ws_terminal::project_scripts` reads root manifests and builds literal argument lists. The workspace starts these commands only
on an explicit script action and puts each run in its own tab of the existing project terminal arrangement. The PTY forwards
the exit status after draining output, so exited script tabs can retain their last screen. Run state belongs to the project
in the current window. A restart waits for the previous child to exit before launching a replacement.
