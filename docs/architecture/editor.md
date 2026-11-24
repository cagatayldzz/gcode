# Editor & performance

> **Opening a file must feel instant.** This page explains how gCode keeps that promise, and how highlighting and previews fit in.

## Opening files quickly

Measured in a development build, reading a file and building its editor take a few milliseconds. The real cost is **the first time the
highlighter draws a text** — up to ~0.1 s for a few tens of KB. So the work is moved out of the click:

```text
 pointer rests on a file in Explorer (120 ms)        click
        │  HOVER_PREPARE_DELAY                          │
        ▼                                               ▼
 EditorPanel::prefetch                            editor already built & highlighted
   reads the file, draws its editor
   out of sight for ½ s  ──────────────────────►  opens instantly
```

| Mechanism | Detail |
| --- | --- |
| **Editor cache** | `EditorPanel` keeps the editor of every file it has opened (`files`, up to 24). Files with unsaved changes are never dropped, and files in a tab of any project stay. Reopening is instant and keeps unsaved text, even after its tab was closed. |
| **Hover prefetch** | After 120 ms on a file (`HOVER_PREPARE_DELAY`), `prefetch` reads it and draws its editor out of sight for half a second. Files over **100 KB are skipped**: drawing them would stall the window, so they open on click and finish highlighting in the background. |
| **Language warm-up** | Shortly after launch the panel draws a few lines of each common language out of sight, one every 250 ms (`WARMUP_SAMPLES`), so the Kit's syntax definitions are loaded before the first real file. |
| **Optimized dependencies** | `Cargo.toml` sets `opt-level = 3` for dependencies in the dev profile; without it the highlighter is about ten times slower. |
| **`ActiveTab`** | The panel sends `ActiveTab` whenever the shown tab changes; the workspace selects the same file or change in the sidebar. |

## Highlighting

Two systems cover all languages:

```text
 language_for(path)  (editor.rs)
   │
   ├─ "rust" "toml" "json" "markdown" "typescript" "tsx" "javascript" "html" "css"
   │  "python" "bash" "yaml" "swift" "go" "kotlin"
   │      └─► tree-sitter grammar via GPUI Kit's LanguageRegistry
   │
   └─ "dotenv" "gitignore" "dockerfile"
          └─► ws_index::highlight::Highlights::compute(language, text)
              the colors of the whole text are worked out ONCE as runs that cover it
              (newlines included); drawing only slices the runs
```

- **Grammars** come from GPUI Kit's features (`tree-sitter-rust`, …). gCode adds two registrations in `crates/ui/src/syntax.rs`: the Swift
  highlight query (Kit registers the grammar with an empty one), and **Kotlin**, whose grammar crate (`tree-sitter-kotlin-ng`) ships no
  query, so `crates/ui/src/syntax/kotlin.scm` is written for gCode. In that grammar `true`, `false` and `null` are plain identifiers, so
  they are matched by text predicates.
- **Line-based languages** are classified by `ws-index` with no interface dependency, which is why they are unit-tested alone.
- A **language label** for the status bar comes from `language_label` (for example `"kotlin" → "Kotlin"`).

### Adding a language

1. Add the file extension to `language_for` in `crates/ui/src/editor.rs` and a label in `language_label`.
2. If the Kit has the grammar, enable its feature in the workspace `Cargo.toml`. Otherwise add the grammar crate to `crates/ui` and register it
   in `syntax.rs` with a highlight query.
3. Add a sample to `WARMUP_SAMPLES` (and update the array length).
4. Add a test in `crates/ui/tests/syntax_highlighting.rs` that asserts roles such as `keyword`, `function`, `type`, `comment`.

## Files and saving

- Only text files up to 2 MB open (images are previewed). A refused file shows a notification.
- The editor saves with `write_atomic` (a temporary file replaces the target in one step) and **refuses the first save when the file changed
  on disk since it was read**; the next save overwrites.
- Closing a tab keeps its text in the cache; only saving writes to disk.

## Minimap

`minimap.rs` and `minimap/` show a long file's shape beside the editor (80+ lines). It samples the buffer **in the background** and reuses
the editor's scroll position, so dragging it never blocks typing.

## Problems

`EditorPanel` tells a language server about JavaScript and TypeScript files (`ws-language`). `find_server` looks above the file for the
project's own TypeScript (`node_modules/@typescript/typescript-<platform>/lib/tsc --lsp --stdio`, then `node_modules/.bin/typescript-language-server`),
then for a `typescript-language-server` on the path; a file with no server just shows no problems. One server runs per project folder
(`languages`), as a child process with a reader thread, and the editor never waits for it:

- a file is sent when its editor is built, and again 300 ms after typing pauses (`SYNC_DELAY`);
- the server answers a `textDocument/diagnostic` request (or pushes `publishDiagnostics`) and the result arrives through a channel;
- errors and warnings become the editor's own diagnostics (underlines and hover text) and fill the counts in the status bar and the
  Problems panel, which the counts open. A row in the panel opens the file at the problem.

## Go to definition

⌘-click on a name underlines it and opens its definition, in the same file or another (imports, and the declaration files of libraries in
`node_modules`). Each JavaScript and TypeScript editor gets a `DefinitionProvider` (`editor/definitions.rs`) that sends
`textDocument/definition` to the project's language server. The same file is handled by the editor itself; another file is sent as an
`OpenLocation` event, which the workspace opens (`allow_large` lifts the 2 MB limit to 16 MB for declaration files) and selects the name in.

## Path above the text

Under the tabs a line shows the open file's path from the project's folder, one step per folder, with the file's icon at the end
(`EditorPanel::render_breadcrumb`).

## Explorer marks

The Explorer colors what the uncommitted changes touch: a changed file takes the color of its kind (new, modified, deleted, renamed,
conflicted) and shows its one-letter mark; a folder that holds a changed file is colored and has a dot at its right. They are worked out
once per frame from the repository summary (`ExplorerMarks`).

## Image previews

SVG and ICNS previews are decoded **off the interface thread**. For an SVG in Changes, Preview renders the last-commit and working-tree
versions side by side on demand (`ws_git::file_versions`, capped at 16 MB per image).

## Lists that stay smooth

Long lists use **virtualization**: only the rows in view are built, laid out and drawn.

| List | Mechanism |
| --- | --- |
| Explorer rows | `uniform_list` |
| Project search results | `uniform_list` |
| Changes list | Kit `v_virtual_list` with a fixed row height (`CHANGE_ROW_HEIGHT`) |
| Files of a commit | Kit `v_virtual_list` (`FILE_ROW_HEIGHT`) |
| All changes diff | the Kit `Diff` virtualizes itself |

A `v_virtual_list` needs every row's height before it draws and lays rows out at their content width, so these rows have a **fixed height
and an explicit width** taken from the list's last measured bounds. Row sizes are cached and rebuilt only when the row count changes.

## Measuring

Do not guess; measure the step, then the frames after it. See [Development → Performance](../development/performance.md) for the benchmark
suite and the profiling recipes.
