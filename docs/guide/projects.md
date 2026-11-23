# Projects

A **project** is a folder on your Mac. gcode lists it, remembers how you grouped it, and gives it its own Explorer state,
editor tabs and terminals. Nothing gcode does ever deletes a project's folder.

## Adding a project

Press `+` in the title bar (or **File → Add Project…**) to open the **Add project** dialog.

| Tab | What it does |
| --- | --- |
| **New** | Creates a folder with the given name in the chosen location and starts an empty Git repository on `main`. It fails if the folder already exists. |
| **Clone** | Runs `git clone` for a repository URL such as `https://github.com/user/repo.git`, into a folder named after the repository. A connected [account](./accounts.md) supplies its token for the host, otherwise your own Git login is used (SSH key or credential helper). It never asks for input: a repository that needs a login you do not have fails with Git's message. **Browse my repositories…** lists the connected account's repositories and fills in the address. |
| **Import** | Adds a folder that already exists on this Mac. |

The new project opens at once.

## Switching projects

Choose a project from the **project menu** at the left of the title bar, or click one of the favorite projects shown as tabs.

```text
 project A                       project B
 ┌───────────────┐   switch      ┌───────────────┐
 │ Explorer tree │ ───────────►  │ Explorer tree │
 │ editor tabs   │   A is put    │ editor tabs   │
 │ Git summary   │   away, B is  │ Git summary   │
 │ shells  ●●●   │   brought     │ shells  ●     │
 └───────────────┘   back        └───────────────┘
      shells of A keep running while B is open
```

Everything in the window belongs to the open project: the Git summary, the folder tree (with the folders you had opened and the
file you had selected), the file in the editor and the terminals. A dev server running in project A is still running when you come back to it.

If a project's folder disappears (deleted in Finder, by a command, by another app), gcode removes it from the list on its own, at the
next start, when the window comes to the front, and every few seconds, and tells you which one it removed. A folder that cannot be
read (no permission, an unmounted disk) is **not** counted as deleted.

## What is waiting in a project

Beside each project in the menu, two marks can appear:

| Mark | Meaning |
| --- | --- |
| ↑ arrow | Commits are waiting to be pushed: the branch is ahead of its remote, or was never published while the repository has a remote. |
| ● blue dot | Files have uncommitted changes. |

They are read when the menu opens and kept up to date while it is open.

## Favorites and categories

- **Favorites** — press the star at the left of a project in the menu (it appears while the pointer is on the row). Favorites are listed
  first, under **Favorites**, are not repeated under their category, and also become tabs in the title bar.
- **Categories** — group projects under headings in the menu. Projects without one are listed under **No category**. A category that
  loses its last project goes away, unless you created it yourself: a category you named stays until you delete it.

The sort button beside **Add** in the menu opens **Settings → Project list**, where you manage both.

### The Project list page

| You can | How |
| --- | --- |
| Add a category | **New category** (top right) asks for a name. |
| Move a project | Drag a row by its grip: onto a category title to put it in, onto **No category** to take it out, onto another project to put it right before that one. |
| Per project | **Name, icon and color**, the category menu, **Move up / down**, and **Remove from gcode** (asks first; its terminals stop, the folder and files stay). |
| Per category | **Rename**, **Move up / down**, **Fold / unfold** in the menu, and **Delete** (the projects stay). |

**Settings → Projects** holds the projects folder (where new and cloned projects go) and **Add all**, which adds every Git repository
directly inside that folder at once.

## Project settings

The folder-gear button in the activity bar opens the settings of the open project. They fill the area like the terminals do; click the
button again, or Explorer, Git or the terminal button, to leave.

| Page | Does |
| --- | --- |
| **General** | **Name** replaces the folder's name in the menu and Explorer (the folder is not renamed). Clearing it goes back to the folder's name. |
| **Appearance** | **Color** tints the avatar and fills it while the project is open; **Icon** shows one of 20 icons instead of the first letter. **Default** and **Aa** go back to the theme's blue and the first letter. |
| **Git** | Changes the address of the remote. |
| **Index** | Shows the saved list of the project's files that searching starts from: status (ready, out of date, not made yet), file count, size, when it was made and where it is kept. **Rebuild index** walks the project again; **Delete index** removes the saved one. |
| **Remove project** | Takes the project out of gcode. The folder stays on disk. |

**Save** is available once something changed; **Discard** leaves without saving. Enter in the name field also saves.

## Profiles

A **profile** is a separate set of projects, settings and connected accounts, like a browser profile. Each window shows one profile;
windows of the same profile share their settings, windows of different profiles do not.

- The **Profiles** menu lists every profile with a check beside the one the window in front shows. Choosing one brings its window
  forward or opens one. A profile can have only one window.
- The Dock menu lists the profiles too.
- **Add Profile…** and **Settings → Profiles** create, rename and delete profiles. A profile with an open window cannot be deleted.
- Tokens of a non-default profile live in their own Keychain entries, so accounts are never shared between profiles.

The default profile keeps its files directly in the data directory; others live in `profiles/<id>/`
(see [Persistence](../architecture/persistence.md)).
