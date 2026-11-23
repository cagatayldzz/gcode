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
| `⌘W` | Close the open editor tab; pressed again, the remaining tabs from the last to the first (never the window). In a terminal it closes the pane |
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
| `⌘K` `⌘C` | Add line comment to the selected lines |
| `⌘K` `⌘U` | Remove line comment |
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
