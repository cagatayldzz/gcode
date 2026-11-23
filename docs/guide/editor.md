# Explorer & editor

## Explorer

The **Explorer** is the sidebar's folder tree for the open project. A row at the top shows the project's name and four buttons:

| Button | Does |
| --- | --- |
| New file | Creates a file inside the selected folder, next to the selected file, or in the project folder. Type the name and press Enter; `dir/name.rs` creates the subfolder too. |
| New folder | The same, for a folder. |
| Refresh | Reads the tree again. |
| Collapse folders | Folds every open folder. |

Behavior worth knowing:

- Files Git ignores are dimmed. The tree shows folders and files from the **index** of the project, not a fresh walk on every click.
- **Single click** on a file *previews* it (the tab is replaced by the next preview); **double click** keeps it open.
- Hovering a file for a moment prepares its editor out of sight, so the click that follows opens instantly
  (see [Opening files quickly](../architecture/editor.md#opening-files-quickly)).
- Indent guides mark the folder you are pointing at.
- Right-click a file or folder for its menu: open as a preview or in a read-only side preview, **Open With** the system's default app, **Reveal in Finder**, a file **Timeline** (the commits that touched it), **Cut / Copy / Paste**, **Copy Path** / **Copy Relative Path**, **Rename** and **Delete** (the file moves to the Trash).
- Text size follows **Settings → Editor → Explorer font size** (14 px by default).

## The editor

Click a file to open it in a tab. The tabs sit in a row on top, and the shown one is raised.

- **Tabs** — click to show, **×** to close. A click on a file that is already open shows its tab. The Explorer selects the file of the
  shown tab.
- **Saving** — `⌘S` writes the shown file; a `•` after the name means unsaved changes. If the file changed on disk after gCode read
  it, the first save is refused and the next one overwrites.
- **Closing keeps your text** — closing a tab keeps its unsaved text in memory; opening the file again brings it back. Text is only
  written by saving. Each project keeps its own tabs.
- **Limits** — other than images, files must be text and at most 2 MB, or they are refused with a notification.
- `⌘W` closes the shown tab; pressed again, the remaining tabs from the last to the first. It never closes the window. The red close button minimizes the window instead, so terminals and agents keep running; `⌘Q` quits the app.

### Syntax highlighting

Highlighting comes from tree-sitter grammars through GPUI Kit, plus a few line-based languages gCode colors itself.

| Source | Languages |
| --- | --- |
| Grammars | Rust, TOML, JSON / JSONC, Markdown, TypeScript, TSX, JavaScript, HTML, CSS, Python, Bash / zsh, YAML, Swift, Go, **Kotlin** |
| Line-based | `.env`, `.gitignore`, Dockerfile |
| Anything else | Plain text |

The status bar shows the language of the open file.

### Images

SVG and macOS ICNS files show an image preview. For an SVG, use **Source** to edit it and **Preview** to see the result.

### Minimap

Files with at least 80 lines show a **minimap** at the right edge. Click anywhere in it to jump there, or drag the highlighted region
to scroll through the file.

### Zoom and font size

The editor has a zoom button in its title bar (also **Zoom In** in the `…` menu). **Settings → Editor → Font size** sets the size of
code in the editor and in a file's changes, from 9 to 24 px (13 by default). The terminal keeps its own size.

## Sidebar and status bar

Drag the sidebar's edge to resize it (200–960 px). It keeps that width when the window is resized; only the main area changes.

The thin **status bar** under the editor appears in the Explorer view only, not in the Git view or under the terminals:

```text
 main · 3 changed        ~/Developer/atlas          Ln 12, Col 5  Spaces: 2  UTF-8  LF  Rust
 └ branch & changes      └ project folder           └ cursor, indentation, encoding, line ending, language
```
