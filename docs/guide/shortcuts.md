# Keyboard shortcuts

All shortcuts are bound in one place (`crates/app/src/lib.rs`), and the menus, buttons and the command palette use the same actions.
The terminal info bar and the command palette read the live keymap, so what they show is always accurate.

## Everywhere

| Shortcut | Action |
| --- | --- |
| `⌘P` | Go to File |
| `⇧⌘P` | Command palette |
| `⇧⌘F` | Search in project |
| `⌘S` | Save the open file |
| `⌘W` | Close the open editor tab; pressed again, the remaining tabs from the last to the first (never the window). In a terminal it does nothing, so a running terminal is never closed by accident |
| `⌘J` | Show or hide the terminals |
| `⇧⌘A` | Show or hide the AI agents |
| `⇧⌘M` | Mention projects (in the AI view and terminals) |
| `⇧⌘G` | Open the repository on its hosting service |
| `⌘T` | New terminal tab (shows the terminals) |
| `⌘D` / `⇧⌘D` | Split the terminal pane side by side / one above the other |
| `⌘,` | Settings |
| `⌘Q` | Quit |

## Editor

| Shortcut | Action |
| --- | --- |
| `⌘F` | Find in the file: a widget at the top right of the editor with the selected text as the query, **Aa** (match case), the match count (`3 of 8`), previous / next (`⇧↵` / `↵`) and close (`Esc`). The matches are highlighted in the text |
| `⌥⌘F` | Find and replace: the same widget with the replace row open (`↵` replaces one, **All** replaces every match). The arrow at its left opens or closes the row |
| `⇧⌘F` | Search in project, also while the editor has the focus |
| `⌘K` `⌘C` | Add line comment to the selected lines |
| `⌘K` `⌘U` | Remove line comment |
| `⌘D` | Add the next occurrence: selects the word at the cursor, then each press adds the next place the selected text occurs (wraps around the file). In a terminal, `⌘D` still splits the pane |
| `⌘/` | Toggle line comment: off when every line is already a comment, else on |

The marker follows the language: `//` (Rust, JavaScript, TypeScript, Go, Swift, Kotlin), `#` (Python, shell, YAML, TOML, `.env`, Dockerfile,
`.gitignore`), `<!-- -->` (HTML, Markdown) and `/* */` (CSS). A language without comments, such as JSON, says so in a notification.

## Search results

| Shortcut | Action |
| --- | --- |
| `⌘↵` | Open all results as a read-only editor tab |

## Terminal

| Shortcut | Action |
| --- | --- |
| `⌘C` / `⌘V` | Copy / paste |
| `⌘K` | Clear the scrollback |
| `⇧PageUp` / `⇧PageDown` | Scroll the history by a screen |
| `⇧Home` / `⇧End` | Jump to the oldest / latest output |
| `⌥←` / `⌥→` | Move by word |
| `⌘←` / `⌘→` | Move to the start / end of the line |
| `⇧←` `⇧→` `⇧↑` `⇧↓` | Move the end of the selection |

Everything without a shortcut is in the command palette.
