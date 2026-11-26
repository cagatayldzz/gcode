# Window layout

One `Workspace` entity renders one window. This page explains how the pieces are composed and which state decides what is shown.

```text
 ┌─────────────────────────────────────────────────────────────────────────┐
 │ title band   ● ● ●  repository switcher │ favorite tabs … │ + │ Update  │  TOOLBAR_HEIGHT
 ├──────┬─────────────────────┬──────────────────────────────────────────────┤
 │      │                     │ (Git view only) branch ▾ │ sync button │      │  render_git_toolbar
 │ ACT. │      SIDEBAR        ├──────────────────────────────────────────────┤
 │ BAR  │  Explorer / Search  │                                              │
 │ 56px │  / Git              │   DockArea  ── EditorPanel (tabs)            │
 │      │  h_resizable        │        or ChangeView / CommitView /          │
 │      │  200–960 px         │        AllChanges in the Git view            │
 │      │                     ├──────────────────────────────────────────────┤
 │ ava- │                     │ status bar (Explorer view only)              │
 │ tar  │                     │                                              │
 └──────┴─────────────────────┴──────────────────────────────────────────────┘
```

## The pieces

**Title band.** Right of the traffic lights: the **repository switcher** (a fixed first tab with a filterable, category-grouped menu of every
project and an Add button), then the favorite projects as tabs, then the Add button.

**Activity bar** (`render_activity_bar`). A 56 px strip of buttons — Explorer, Search, Git, terminals, AI agents, and project settings
below the tool group. The profile's avatar sits at the bottom and opens a menu with the profile list; the gear below it opens Settings. Which buttons show, and in what
order, follows `Preferences.activity_bar` (`ActivityBarLayout`: order and hidden ids, in `preferences.json`).

**Sidebar.** The selected view, in a Kit `h_resizable` pane. It is *plain workspace content, not a dock panel*, so it has no tab bar, and
it runs down to the bottom of the window.

**Main column.** Right of the sidebar, a column holds the Kit **`DockArea`** and the status bar below it. The dock holds the main
`EditorPanel`; Explorer can open a read-only preview beside it in a horizontal resizable split (it closes when the active project changes).
Long text files show a minimap, which samples the buffer in the background and reuses the editor's scroll position. The Kit draws the dock
chrome (the title, zoom button and `…` menu).

**Activity log panel** (`activity_panel.rs`, debug builds). The bug button in the title band opens a 280 px panel under everything else in the
window (activity bar included, takeover areas too) and closes it again; the panel's own close button does the same, and its top edge can be
dragged. It lists the lines of `ws_activity` oldest first in the Kit's read-only `Editor` (so text can be selected and scrolled), following
the newest line while it is at the bottom; new lines wait while text is selected. Filters, Copy (the selection, else everything) and Clear.

## Takeover areas

The terminals and the AI view are **not** dock panels. They *replace* everything right of the activity bar:

```text
 sidebar_tab: Explorer | Search | Git
 terminal_open: bool          ┐
 agent_open: bool             ├─ full_area_open  →  sidebar + dock are NOT drawn (the status bar stays under the terminals)
 project settings open        ┘
```

- `ToggleTerminal` (`⌘J`) sets `terminal_open`; `render_terminal_area` draws a tab strip when there are several tabs and the shown tab as a
  tree of panes, with the status bar below it. The status bar's keyboard button (`ShowShortcuts`) opens the shortcuts in a dialog.
- `ToggleAgents` (`⇧⌘A`) sets `agent_open`. The two never show together.
- The project settings view (`Workspace.project_settings`) fills the same area, and `full_area_open` counts it.
- Showing the editor again keeps every shell running: the entities stay alive in the workspace.

### Terminal panes as data

```rust
// crates/ui/src/terminal_layout.rs — tabs and split panes without any GPUI types
pub(crate) enum Direction { Columns /* side by side */, Rows /* one above the other */ }

pub(crate) enum Node<T> {
  Pane(T),
  Split { id: u64, direction: Direction, first: Box<Node<T>>, second: Box<Node<T>> },
}

pub(crate) struct Arrangement<T> { pub tabs: Vec<Node<T>>, pub active: usize, /* … */ }
```

Splits render as Kit `h_resizable` / `v_resizable` groups; a pane has a header when its tab has several. `⌘T`, `⌘D`, `⇧⌘D` and `⌘W` act on
the shell that has the keyboard, and a terminal's right-click menu uses the same actions.

## The AI view

Same area as the terminals, but organized by session. The header holds one tab per session (agent icon and name), a menu to start another
agent, and the project name — no splits. A session is a `Terminal` that runs `$SHELL -l -i -c "exec codex"` (or `claude`) in the project
folder. `NewCodex` and `NewClaude` start one. With no session the view shows the choice of agents (`render_agent_empty`); when the last
session ends it stays open and shows it again. Sessions live in `Workspace.agent_tabs` per project and keep running while another project
or view is shown.

The terminals and the AI view end in an info bar (`shortcut_bar`) that lists their shortcuts, read from the keymap with `Kbd`.

## Sizing rules

- Pixels appear only for terminal geometry, window size/placement, platform boundaries and named constants (`FONT_SIZE`,
  `LEFT_DOCK_WIDTH`, `CHANGE_ROW_HEIGHT`…). Everything else uses Kit size tokens (`gap_2`, `size_5`, `rounded_md`).
- The Kit sizes icons and dock parts in `rem`, so the theme's `font_size` is **never changed**; gCode's own content sets its text size with `FONT_SIZE`.
- Colors come from `cx.theme()` or the palette in `theme.rs` (`colors(cx)`), never from raw `rgb`/`hsla`.
