# Search & navigation

Three tools help you move around quickly. All of them work on the **open project**.

## Go to File — `⌘P`

Type a few letters of a file name or path and press Enter to open it.

```text
 ┌──────────────────────────────────────┐
 │ ⌕  wsr                                │
 ├──────────────────────────────────────┤
 │ crates/ui/src/workspace/render_git.rs │   ← letters match in order,
 │ crates/ui/src/workspace.rs            │     not necessarily next to each other
 │ crates/storage/src/window.rs          │
 └──────────────────────────────────────┘
```

- The project's file list comes from its saved **index** (every file the ignore rules let through), read in the background.
- Each keystroke is ranked off the interface thread, so typing never stalls; the 50 best matches are shown.
- Also in **File → Go to File…**.

## Command palette — `⇧⌘P`

A searchable list of everything gcode can do. It has two groups:

- **Commands** — the actions of the menus, each with its shortcut read from the live keymap.
- **Projects** — choose one to switch to it.

Choosing a command runs it exactly as the menu item or shortcut would; the palette adds no commands of its own. Find it under
**View → Command Palette…**.

## Search in project — `⇧⌘F`

The magnifier in the activity bar opens the project search. It searches file *contents*.

| Option | Meaning |
| --- | --- |
| Case sensitive | Match letter case exactly. |
| Whole word | Match only whole words. |
| Regex | Treat the query as a regular expression. Replacements can use capture groups. |
| Include / Exclude | Glob patterns that narrow the files searched, such as `crates/**/*.rs`. |

- Results are grouped by file with the matching lines; click one to open the file there.
- `⌘↵` opens all results as a **read-only editor tab**.
- **Replace** plans the whole replacement first, then writes it; **preserve case** keeps the case pattern of each match
  (`foo` → `bar`, `Foo` → `Bar`, `FOO` → `BAR`). A replacement that would grow files past the limit is refused with
  *"The replacement is too large. Limit the search to fewer files."*
- Searching runs on a background thread and a new query cancels the old one, so you can type freely.

::: tip Ignored files
Search respects `.gitignore` and `.git/info/exclude`, so `target/` and `node_modules/` stay out of results. Hidden files such as `.github/` are searched.
:::
