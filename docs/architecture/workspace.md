# Workspace & state

The `Workspace` is the window's single source of truth. This page covers how it owns projects, how switching works, and how the window
stays in step with the disk.

## Ownership

```text
 Workspace  (one per window; one profile per window)
 ├─ projects: ProjectList            the list shown in the title bar; `active` decides everything below
 ├─ git: Option<Summary>             branch, changed files, Sync  (None outside a repository)
 ├─ rows: Vec<explorer::Row>         the Explorer tree
 ├─ editor_panel: Entity<EditorPanel>
 ├─ terminal_tabs: HashMap<PathBuf, Arrangement<Entity<Terminal>>>   every project's terminals, kept alive
 ├─ agent_tabs:    HashMap<PathBuf, AgentTabs<Entity<Terminal>>>
 ├─ settings: Entity<SettingsModel>  shared by windows of the same profile
 └─ views: HashMap<PathBuf, ProjectView>   per-project Explorer / Git selections, put away while another project is open
```

- `Workspace.projects` is the `ProjectList`; the active project decides what the sidebar, the editor tree, the Git summary and the
  terminals show.
- **`Workspace::switch_project` is the only way the open project changes.**

## Switching projects

```text
 switch_project(new)
   1. put the old project's Explorer and Git selections away      (Workspace.views)
   2. put its editor tabs away                                    (EditorPanel::switch_project, `saved`)
   3. apply the change
   4. restore the new project's views
   5. ensure_terminal(new)  – a project with no shell gets one
```

Editors other projects have open are **never evicted** from the cache, and each project's terminal entities stay alive in
`terminal_tabs`, so their shells keep running.

A shell that exits sends `TerminalExited`, and closing a pane does the same job (`close_pane`): the workspace takes the pane out (its
sibling takes its place, an empty tab goes) and, if the project has no shell left, shows the editor again.

## The refresh loop

`ws_git::summary` reads the branch and changed files with libgit2. `Workspace::refresh_in_background` runs it — together with reading
the folder tree — on a background thread:

```text
 window activated ─┐
                   ├─► refresh_in_background()
 every 2 s ────────┘     │  (REFRESH_INTERVAL)
                         ▼
                background thread: summary(root) + explorer::rows(root, expanded)
                         │
                         ▼
        apply ONLY IF  the project is unchanged
                  AND  the expanded folders are unchanged
                  AND  the data differs              ← no needless redraws
```

So edits made in another app show up by themselves, and an old answer never overwrites a newer state. After an action that changes the
repository (commit, switch, merge…) the workspace calls `refresh_project()` directly.

## Background work, concretely

| Work | Where it runs |
| --- | --- |
| Git reads and writes, network commands | `cx.background_executor().spawn(..)` |
| Project search & replace | background executor; a new query cancels the old |
| Go to File ranking | background; the 50 best per keystroke |
| Project index (open, rebuild) | background |
| Verifying a token, listing repositories, pull requests | background |
| Keychain access | background |
| Update check, download, verification | background threads; progress arrives through a channel |
| Terminal output | a thread feeding `alacritty_terminal`; events arrive through a channel |

A view keeps the `Task` of recurring work on its **owner** (never detached), subscriptions stay on their owner, and retained interface text
is a `SharedString`. Code never stores `&mut Window` or `App`; it stores handles (`Entity`, `WeakEntity`).

## Shared settings across windows

Windows of the same profile share one `SettingsModel` (via `SettingsRegistry` in `settings.rs`); windows of different profiles do not.
Changing a setting in one window updates the others at once.

## Notifications

Anything that can fail for the user shows an **actionable message in the interface** (a notification), not only in a log: Git errors, a
refused token, a failed save, a failed update.
