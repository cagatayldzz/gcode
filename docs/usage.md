# Usage

A Vekro window has four parts:

- the **project rail** on the far left: one avatar per project, the `+` button, and Settings at the bottom;
- the **sidebar**: the Git summary with the changed files, and the folder tree;
- the **editor area** in the middle: a file, or the diff of a changed file;
- the **terminal dock** at the bottom: one or more shells of the open project.

The editor and each terminal have a zoom button in their title bar (also **Zoom In** in the `…` menu) that
makes the panel fill the whole dock area; press it again to go back. Drag the edge of the sidebar to resize it. 
A thin **status bar** shows the branch and the number of changed files on the left, the project
folder in the middle and the open file with its language on the right.

## Projects

Press `+` in the project rail to open the **Add project** dialog:

| Tab | Does |
| --- | --- |
| New | Creates a folder with the given name in the chosen location and starts an empty Git repository on `main` there. Fails if the folder exists. |
| Clone | Runs `git clone` for a repository URL such as `https://github.com/user/repo.git` into the chosen location, in a folder named after the repository. It uses your own Git login (SSH key or credential helper) and never asks for input; a repository that needs a login you do not have fails with Git's message. |
| Import | Adds a folder that already exists on this Mac. |

The new or imported project opens at once. Click an avatar to switch; hover it to see the project's name.
Everything in the window belongs to the open project: the Git summary, the folder tree (with the folders you had
opened and the file you had selected), the file or diff in the editor and the terminals. Switching puts the
project you leave away and brings back what the other one showed. Its shells keep running, so a dev server in
project A is still running when you come back to it.

Projects are only listed in the rail; nothing Vekro does deletes a project's folder.

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

`⌘,` or the gear in the rail opens Settings, with a page list and a search field on the left:

- **General → Appearance → Theme:** System (follows macOS), Light or Dark. It applies at once in every window.
- **General → Projects → Projects folder:** where new and cloned projects are created (the Local Path of the Add
  project dialog starts here). **Change…** chooses it; **Reset** goes back to the default, the folder that contains
  the open project, else your home folder.
- **General → Projects → Add every project in the folder:** **Add all** adds every Git repository directly inside
  the projects folder to the project bar at once (projects already listed are skipped, the open project stays open).
- **About:** the version and the data folder.

Settings are saved as soon as you change them.

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
