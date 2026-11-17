# Usage

A Vekro window has these parts:

- the **title bar**: the current project (a menu of every project grouped by category, with a filter and Add) fixed at the left, the `+` button (Add project) right of it;
- the **activity bar**, a narrow strip at the far left: the Explorer and Git buttons (they choose the view of the sidebar; the Git button shows the number of changed files in a small blue circle), the terminal button under them, and Settings at the bottom;
- the **sidebar**, next to the activity bar: **Explorer** shows the folder tree of the open project, under a row with the project's name and four
  buttons: new file, new folder (created inside the selected folder, next to the selected file, or in the project folder; type the name and
  press Enter, `dir/name.rs` makes the subfolder too), refresh and collapse all folders. **Git** shows the branch
  and the changed files, each with a checkbox (click a file to see its diff in the main area; right-click for **Open in Editor** and
  **Discard changes**, which asks first, throws away every change to that file and deletes it if it is new). Below the list
  is the commit box: type a **Summary** (required) and, if you like, a **Description**, then **Commit N files to
  <branch>**. Only the ticked files go into the commit; the others (staged or not) stay as they are, so they show up
  again in the list. A commit needs `user.name` and `user.email` in your Git configuration.
- the **main area**: tabs of files, filling the rest of the window (in the Git tab it shows the diff of the chosen file or commit instead), (the terminals take it over while they are shown, see Terminals below).

The editor has a zoom button in its title bar (also **Zoom In** in the `…` menu). Drag the edge of the sidebar to resize it
(up to 960 px); it keeps that width when the window is resized (full screen, smaller), only the main area changes. A thin **status bar** under the editor (in the Explorer view; not in the Git view or under the terminals; the sidebar runs down to the bottom of the window) shows the branch and the number of changed files on the left, the project
folder in the middle and the open file's cursor position, indentation, encoding, line ending and language on the right (`Ln 12, Col 5`, `Spaces: 2`, `UTF-8`, `LF`, `Rust`).

## Projects

Press `+` in the title bar to open the **Add project** dialog:

| Tab | Does |
| --- | --- |
| New | Creates a folder with the given name in the chosen location and starts an empty Git repository on `main` there. Fails if the folder exists. |
| Clone | Runs `git clone` for a repository URL such as `https://github.com/user/repo.git` into the chosen location, in a folder named after the repository. It uses a connected account's token for its host (see Accounts below), else your own Git login (SSH key or credential helper), and never asks for input; a repository that needs a login you do not have fails with Git's message. With a connected account, **Browse my repositories…** lists the account's repositories (filter by typing) and fills in the address. |
| Import | Adds a folder that already exists on this Mac. |

The new or imported project opens at once. Click an avatar to switch; hover it to see the project's name.
Everything in the window belongs to the open project: the Git summary, the folder tree (with the folders you had
opened and the file you had selected), the file in the editor and the terminals. Switching puts the
project you leave away and brings back what the other one showed. Its shells keep running, so a dev server in
project A is still running when you come back to it.

Projects are only listed in the project menu; nothing Vekro does deletes a project's folder. The other way round: when a
project's folder is deleted (in Finder, by a command, by another app), Vekro takes it out of the list on its own, at
the next start, when the window comes back to the front and every few seconds, and says which it removed. A folder that
cannot be read (no permission, an unmounted disk) is not counted as deleted.

### What is waiting in a project

In the project menu, each project can show two marks beside its name: a small **arrow** when commits are waiting to be pushed (the
branch is ahead of its remote, or was never published while the repository has a remote), and a **blue dot** when files have
uncommitted changes. They are read when the menu opens and kept up to date while it is open.

### Favorites

Star the projects you use most: in the project menu of the title bar, press the star at the left of a project (it appears while the
pointer is on the row; a favorite's star stays, filled). Favorites are listed first, under **Favorites**, and are not repeated under
their category; pressing the star again takes them out.
Projects without a category are listed under **No category**. Favorites are kept in `projects.json`.

### Categories

Categories group the projects in the project menu of the title bar: each category is a heading, and projects without one
are under **No category**. The sort button (arrows) beside **Add** in the menu opens Settings → **Project list**, where they are
managed:

- Settings → **Project list** shows every project and category with buttons to rename, reorder, move and delete them (deleting a
  category keeps its projects, outside any category).

A category that loses its last project goes away. Projects without a
category come first, then the categories in the order they were created.

### Code size

**Settings → Editor → Font size** sets the size of code in the editor and in a file's changes, from 9 to 24 pixels
(13 by default). It applies at once. The terminal keeps its own size.

### Managing the project bar in Settings

`⌘,` opens Settings. The **Project bar** page lists every project and category in the order the bar shows them
(projects without a category first).

- **New category** (top right) asks for a name and adds an empty category, a place to drop projects on. A category you
  named stays until you delete it, even when it is empty.
- **Drag a row** by its grip: onto a category's title to put the project in it, onto **No category** to take it out,
  onto another project to put it right before that project, in that project's category.
- Per project: **Name, icon and color**, the **category menu** (it shows the current category; pick another, start a
  new one or choose none), **Move up / down** and **Remove from Vekro** (asks first: its terminals stop, the folder
  and its files stay).
- Per category: **Rename**, **Move up / down**, **Fold / unfold** in the bar and **Delete** (the projects stay).

The **Projects** page holds the projects folder and **Add all**.

### Project settings

Right-click a project and choose **Settings…** to change how it looks:

- **Name** replaces the folder's name in the project menu and the Explorer (the folder itself is not renamed);
- **Color** tints the avatar, and fills it while the project is open;
- **Icon** shows one of 20 icons instead of the first letter.

Press Save (or Enter in the name field). Clearing the name, choosing **Default** and **Aa** goes back to the folder's name, the theme's blue and the first letter.

## Terminals

The **terminal button** in the activity bar, under Git (also `⌘J`), makes the terminals fill the main area, the right part of the window,
like a terminal window: everything right of the activity bar, with no sidebar and no status bar. Choosing Explorer or Git (or pressing the terminal button or `⌘J` again) gives
the main area back to the editor. Each project starts with one `zsh` (your login shell) in the project folder. The shells keep
running while the editor is shown.

- **Tabs** (`⌘T`): each is a shell, or several. With more than one tab a strip appears on top, with the title of each tab, **×** to
  close it and **+** to open another.
- **Panes** (`⌘D` splits side by side, `⇧⌘D` one above the other): the shell that has the keyboard is split and the new shell gets
  the keyboard. A tab with several panes gives each a small header with **×** (close), the title of the shell and **⋯** (New Tab,
  Split Pane Vertically, Split Pane Horizontally, Close). Drag the divider between panes to resize them.
- **Right-click** in a terminal for New Window `⌘N`, New Tab `⌘T`, Split Pane Vertically `⌘D`, Split Pane Horizontally `⇧⌘D`,
  Copy `⌘C`, Paste `⌘V`, Clear Buffer `⌘K` and Close `⌘W`.
- **Closing** (`⌘W`, ×, Close): the pane's shell stops; its sibling takes the whole place; a tab left without panes goes; when the
  last one goes the editor comes back. A shell that exits (`exit`, `⌃D`) closes its pane the same way. Each project keeps its own
  tabs and panes.

| Shortcut | Action |
| --- | --- |
| `⌘T` | New terminal tab |
| `⌘D` / `⇧⌘D` | Split the pane side by side / one above the other |
| `⌘W` | Close the pane |
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

Click a file in the folder tree to open it in a tab. Text files up to 2 MB open; anything else is refused with
a notification. Open as many as you like: the tabs sit in a row on top of the editor area, the shown one is raised. Click
a tab to show its file, **×** on it closes it (a click on a file that is already open shows its tab). The Explorer selects
the file of the shown tab. `⌘S` writes the shown file; a `•` after the name means unsaved changes. Closing a tab keeps
its unsaved text in memory, and opening the file again brings it back, so nothing is lost silently; the text is only
written by saving. Each project keeps its own tabs.

## Git

The Git tab has two views, **Changes** and **History**. **History** lists the commits of the current branch, newest first, 50 at a time (scroll to the end of the list, or press **Load 50 more commits**, for the next 50; the list has a scroll bar): the subject, who made it and when, any tags, and an arrow while the remote does not have the commit yet. Choose one to see it
in the main area, in place of the editor: its message, the files it changed (with `+`/`-` totals) and the diff of the chosen file;
right-click a commit for **Undo Commit…** (only the last commit, and only while the remote does not have it: it goes back like `git reset --soft`, its changes return to Changes and its message to the commit box), **Create Tag…** (a lightweight tag on that commit, local until you push it) and **Copy SHA**; the first commit is shown when you open the view, and the editor tabs come back when you return to Changes or another sidebar tab.

Above the History list, **Select Branch to Compare…** lists the other branches (the default branch, the ones you used lately, then all
the others, local and remote), each with how many commits it is behind (↓) and ahead (↑) of the current branch; type to narrow the
list and press Enter for the first match. Choosing a branch shows two tabs: **Behind (n)**, the commits of that branch the current one
does not have, and **Ahead (n)**, the commits of the current branch that one does not have (each list shows up to 300). Click a commit
to see it in the main area. While the other branch is ahead, **Create a merge commit** merges it into the current branch like
`git merge`, only when you press it; with conflicts the merge is stopped and nothing is changed, and the notification names the
files. The cross in the field goes back to the plain History list.

**Changes** shows every changed file with a mark: **A** new, **M** modified,
**D** deleted, **R** renamed, **!** conflicted. Click a file to see its diff against the last commit
in the main area, in place of the editor, with the path on top (staged and unstaged changes together; a new file is shown as all
added). Choosing another file replaces it, no tab is added; it updates as you save, and the editor tabs come back when you leave the
Changes view.

A bar above the editor holds the **current branch** and a button that keeps the branch in step with its remote (**Publish branch**,
**Push origin**, **Pull origin** or **Fetch origin**, with the number of commits and the last fetch; while it works it shows git's
progress). The branch opens a menu with two tabs, like GitHub Desktop's. **Branches** lists the local branches with a filter and a
**New Branch** button; switching never overwrites changes, it stops with a message. **Pull Requests** (shown when a connected account
serves the repository's host) lists the open pull requests with a filter and a refresh button (click one to open it in the browser),
says "You're all set!" when there are none, and has **Create pull request…** for the current branch (title, description, target
branch and draft; the branch must be published first). The sparkle button beside the commit description writes the summary and
description with OpenCommit.

The summary refreshes when you return to the window and every 2 seconds while it is open, so changes made
in another app show up by themselves.

## Updates

When a newer release is published on GitHub, Vekro tells you once with a notification. **Settings → Updates** shows the status and
its release notes; **Check for updates** looks again, **Download & Install** downloads the new version and verifies it (checksum,
code signature, macOS Gatekeeper, same developer as the installed app), and **Restart to Update** swaps it in and reopens Vekro. If
the swap fails, the old version is restored. Vekro checks when it starts unless you turn off **Check for updates on launch**; it only
asks GitHub for the latest release and sends nothing about you. Updates work in release builds that run from `Vekro.app`, not in
development builds, and only for Apple Silicon Macs.

## Settings

`⌘,` or the gear at the bottom of the activity bar opens Settings, with a page list and a search field on the left:

- **General → Appearance → Theme:** System (follows macOS), Light or Dark. It applies at once in every window.
- **General → Projects → Projects folder:** where new and cloned projects are created (the Local Path of the Add
  project dialog starts here). **Change…** chooses it; **Reset** goes back to the default, the folder that contains
  the open project, else your home folder.
- **General → Projects → Add every project in the folder:** **Add all** adds every Git repository directly inside
  the projects folder to the project bar at once (projects already listed are skipped, the open project stays open).
- **Accounts:** connect GitHub, GitLab or Azure DevOps with a personal access token (**Connect an account…**: choose the service,
  the host of your own server if you have one, or the Azure DevOps organization, and paste the token; the dialog lists the permissions the token needs, named as the service names them: **GitHub**
  Contents and Pull requests, Read and write (or the `repo` scope of a classic token); **GitLab** the `api` scope; **Azure DevOps** Code,
  Read & write, with the token's organization set to yours, not "All accessible organizations", which stops working on
  December 1, 2026). The token is checked with the service and kept in the macOS Keychain, never in a file. After connecting, Vekro reports
  what it could confirm: GitLab and classic GitHub tokens list their scopes, so a missing one is named; for a fine-grained GitHub token
  and for Azure DevOps only reading can be tried, so an information notice says that pushing and pull requests were not checked (a warning appears only for a permission that is certainly missing). A refused
  request later names the permission it needs. A connected
  account is used to browse and clone your repositories, to list and create pull requests, and to give Git the token for that host
  over HTTPS (clone, fetch, pull and push; SSH remotes keep using your keys; if the host refuses the token, Git's own login is tried before an error is shown). **Disconnect** removes the account and its token.
  When a host refuses the token (a push, pull or pull request is turned down, or connecting finds a permission missing), a small
  warning appears at the corner of the Settings gear at the bottom of the activity bar; it opens Settings → Accounts, where the account shows what
  is missing and which permissions to give the token, with a button that opens the service's token page in the browser (GitHub
  tokens: `github.com/settings/personal-access-tokens`; it lists your tokens, as GitHub does not tell Vekro which one is in use). The warning stays until it is fixed: after changing the token's permissions
  (or connecting again with a new token) press **Check again**, which asks the service what the token may do (it can start a push
  and open a pull request, without changing anything) in the open project's repository, else in the account's first repository.
- **Updates:** see Updates above.
- **OpenCommit:** how commit messages are generated.
- **About:** the version and the data folder.

Settings are saved as soon as you change them.

## Menu bar

Only what is needed: **Vekro** (Settings…, Quit), **File** (New Window, Add Project…, Save, Close Tab), **View** (Toggle Terminal)
and **Repository**, whose one item opens the open project's repository on the service that hosts it: **View on GitHub**, **View on
GitLab** or **View on Azure DevOps** (the name of the host for any other; disabled while the project has no remote). The service
is found from the remote's address, or from a connected account for a self-hosted GitHub or GitLab. Shortcuts show beside the items.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `⌘N` | New window |
| `⌘T` | New terminal tab (shows the terminals) |
| `⇧⌘G` | Open the repository on its hosting service (Repository → View on …) |
| `⌘J` | Show or hide the terminals (they fill the main area) |
| `⌘S` | Save the open file |
| `⌘W` | Close the open editor tab; pressed again, the remaining tabs from the last to the first (never the window). In a terminal it closes the pane |
| `⌘,` | Open Settings |
| `⌘Q` | Quit |

## Where things are stored

`~/Library/Application Support/Vekro` (override with `VEKRO_DATA_DIR`): `projects.json` holds the
project list, their categories and the one open last; `window.json` holds where the window was and how big, and Vekro opens there next time (on a screen that is still connected; else centered); `preferences.json` holds Settings (theme, projects folder, code font size, commit instructions);
`accounts.json` lists the connected accounts (service, host, user name; never a token: those are in the macOS Keychain, service "Vekro").
