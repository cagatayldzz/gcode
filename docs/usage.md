# Usage

A Vekro window has four parts:

- the **project rail** on the far left: one avatar per project, the `+` button, and Settings at the bottom;
- the **sidebar**: the project list, the Git summary with the changed files, and the folder tree;
- the **editor area** in the middle: a file, or the diff of a changed file;
- the **terminal dock** at the bottom: one or more shells of the open project.

A thin **status bar** shows the branch and the number of changed files on the left, the project
folder in the middle and the open file with its language on the right.

## Projects

Press `+` (in the rail or beside **Projects**) to open the **Add project** dialog:

| Tab | Does |
| --- | --- |
| New | Creates a folder with the given name in the chosen location and starts an empty Git repository on `main` there. Fails if the folder exists. |
| Clone | Runs `git clone` for a repository URL such as `https://github.com/user/repo.git` into the chosen location, in a folder named after the repository. It uses your own Git login (SSH key or credential helper) and never asks for input; a repository that needs a login you do not have fails with Git's message. |
| Import | Adds a folder that already exists on this Mac. |

The new or imported project opens at once. Click an avatar, or a name in **Projects**, to switch.
The shells of the project you leave keep running and are back when you return.

Projects are only listed here; removing one never deletes its folder.

## Terminals

Each project starts with one `zsh` (your login shell) in the project folder.

- `⌘T`, or `+` in the terminal's title bar, opens another shell in the same project.
- Drag a terminal's tab to the side of the dock to put two shells next to each other, for example
  the running app on the left and an AI assistant on the right.
- A shell that exits (`exit`, `⌃D`) closes its terminal; a project is never left without one.

| Shortcut | Action |
| --- | --- |
| `⌘C` / `⌘V` | Copy the selection / paste (an image on the clipboard is passed on to the program) |
| `⌘K` | Clear the scrollback |
| `⇧PageUp` / `⇧PageDown` | Scroll the history by a screen |
| `⇧Home` / `⇧End` | Jump to the oldest / latest output |
| `⌥←` / `⌥→` | Move by word |
| `⌘←` / `⌘→` | Move to the start / end of the line |
| `⇧←` `⇧→` `⇧↑` `⇧↓` | Move the end of the selection |

Mouse: drag to select; double-click selects a word or path, triple-click a line; middle-click
pastes. Programs that follow the mouse (tmux, vim, htop) receive clicks, drags and the wheel;
hold `⇧` to select text instead. Copying joins lines that only wrapped at the screen edge.

## Editor

Click a file in the folder tree to open it. Text files up to 2 MB open; anything else is refused with
a notification. `⌘S` writes the file; a `•` after the name means unsaved changes. Opening another
file replaces the current one without asking, so save first.

## Git

The sidebar shows the branch and every changed file with a mark: **A** new, **M** modified,
**D** deleted, **R** renamed, **!** conflicted. Click a file to see its diff against the last commit
in the editor area (staged and unstaged changes together; a new file is shown as all added).

The summary refreshes when you return to the window and every 2 seconds while it is open, so changes made
in another app show up by themselves.

## Settings

`⌘,` or the gear in the rail:

- **Appearance:** System (follows macOS), Light or Dark.
- **New projects:** the folder new and cloned projects go into. By default, the folder that contains the
  open project, else your home folder.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `⌘N` | New window |
| `⌘T` | New terminal |
| `⌘S` | Save the open file |
| `⌘,` | Open Settings |
| `⌘Q` | Quit |

## Where things are stored

`~/Library/Application Support/Vekro` (override with `VEKRO_DATA_DIR`): `projects.json` holds the
project list and the one open last; `preferences.json` holds Settings.
