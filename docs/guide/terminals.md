# Terminals

The terminal button in the activity bar (or `⌘J`) makes the terminals fill the main area: everything right of the activity bar,
with no sidebar and no status bar. Choosing Explorer or Git, or pressing the button or `⌘J` again, gives the area back to the
editor. **The shells keep running while the editor is shown.**

Each project starts with one `zsh` (your login shell) in the project folder and keeps its **own** tabs and panes.

## Tabs and panes

```text
 ┌ zsh ──────┬ server ─┬ + ┐            ← tab strip (shown once there are 2+ tabs)
 ├───────────┴─────────┴───┤
 │ ❯ bun run dev     │ ❯ cargo test │
 │ …                 │ …            │   ← ⌘D splits side by side
 │                   ├──────────────┤
 │                   │ ❯ git status │   ← ⇧⌘D splits one above the other
 └───────────────────┴──────────────┘
```

| Action | Shortcut | Notes |
| --- | --- | --- |
| New tab | `⌘T` | Shows the terminals if they were hidden. |
| Split side by side | `⌘D` | The shell with the keyboard is split; the new one gets the keyboard. |
| Split one above the other | `⇧⌘D` | |
| Close pane | the pane's ✕ or its menu (`⌘W` does nothing in a terminal) | The sibling takes the whole place; a tab left without panes goes; when the last goes the editor returns. |
| Resize | drag the divider | |

A tab with several panes gives each a small header with **×**, the shell's title, and **⋯** (New Tab, Split Pane Vertically,
Split Pane Horizontally, Restart Session, Restart Codex Daemon, Close).

- **Restart Session** replaces a hung shell with a new one in the same place and folder.
- **Restart Codex Daemon** runs `codex app-server daemon restart`, for when Codex keeps showing "Reconnecting to server…" or sits at "Waiting for startup". If the restart does not finish within 20 seconds, the daemon's processes are killed (a stuck daemon ignores a normal stop) and the restart is tried once more. Codex sessions running in terminals are not touched. Start Codex again afterwards.
- A shell that exits (`exit`, `⌃D`) closes its pane the same way as ×.

Right-click in a terminal for New Tab, Split Pane Vertically/Horizontally, Copy, Paste, Clear Buffer and Close; each shows its shortcut, if it has one.

## Keyboard

| Shortcut | Action |
| --- | --- |
| `⌘C` / `⌘V` | Copy the selection / paste (an image on the clipboard is passed on to the program) |
| `⌘K` | Clear the scrollback |
| `⇧PageUp` / `⇧PageDown` | Scroll the history by a screen |
| `⇧Home` / `⇧End` | Jump to the oldest / latest output |
| `⌥←` / `⌥→` | Move by word |
| `⌘←` / `⌘→` | Move to the start / end of the line |
| `⇧←` `⇧→` `⇧↑` `⇧↓` | Move the end of the selection |
| `⌥` + text | Sends the typed text, so characters such as `@` work on non-US layouts |

## Mouse

- Drag to select; double-click selects a word or path, triple-click a line; middle-click pastes.
- Programs that follow the mouse (tmux, vim, htop) receive clicks, drags and the wheel; hold `⇧` to select text instead.
- Copying joins lines that only wrapped at the screen edge.
- Dropping files on a terminal pastes their shell-escaped paths, each followed by a space, without running anything.

The status bar stays under the terminals (branch, sync, problems). Its **keyboard** button on the right opens the shortcuts in a dialog, read from the keymap, so it always matches your bindings. The command palette has the same as **Keyboard Shortcuts**.
