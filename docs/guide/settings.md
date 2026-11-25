# Settings

`⌘,` (or the gear at the bottom of the activity bar) opens **Settings**, with a page list and a search field on the left. Changes are
saved as soon as you make them and apply at once.

| Page | Contains |
| --- | --- |
| **General** | **Appearance → Theme**: System (follows macOS), Light or Dark. Applies in every window. |
| **Editor** | **Font**: GitHub (the system font and Menlo), Geist (the default) or Inter (with Geist Mono for code); it applies to the interface, the editor and the terminals. **Code → Font size** (9–24 px, 13 by default) for the editor and file changes; **Explorer → Font size** (14 px by default). The terminal keeps its own size. |
| **Activity bar** | **Buttons**: reorder the activity bar's buttons and hide the ones you do not use. |
| **Projects** | **Folder**: where new and cloned projects are created (the *Local Path* of the Add project dialog starts here). **Change…** chooses it; **Reset** returns to the default — the folder containing the open project, else your home folder. **Add all** adds every Git repository directly inside it. |
| **Project list** | Every project and category, with actions ([details](./projects.md#the-project-list-page)). |
| **Profiles** | Create, rename and delete [profiles](./projects.md#profiles). |
| **Accounts** | [GitHub, GitLab and Azure DevOps](./accounts.md). |
| **Updates** | See below. |
| **About** | The version and the data folder. |

## Commit messages

The sparkle button in the commit box (from the **AI Agent** extension) uses these settings; the settings button next to it opens them, they are no longer a page of Settings. They live in `commit.json` in the data folder (an earlier `~/.opencommit` file is imported once), and
the commit instructions are kept in `preferences.json`.

- **Provider and model** — any OpenAI-compatible chat API, or the optional **Apple Intelligence** provider (on-device, macOS 26+, Apple
  Silicon). Apple Intelligence uses a smaller diff budget.
- **Message style** — Conventional Commits by default, in the language you choose.
- **Instructions** — extra rules appended to the built-in ones. Empty text means none at all; *reset* returns to the built-in text.
- **Push after commit** — the commit button also pushes.

Details: [Commit messages](../architecture/commit-messages.md).

## Updates

When a newer release is published on GitHub, gCode tells you **once** with a notification.

| Control | Does |
| --- | --- |
| **Check for updates** | Looks again. |
| **Download & Install** | Downloads the new version and verifies it: checksum, code signature, macOS Gatekeeper, the same developer as the installed app. |
| **Restart to Update** | Swaps the app in and reopens gCode. If the swap fails, the old version is restored. |
| **Check for updates on launch** | Off disables the check at startup. |

gCode only asks GitHub for the latest release and sends nothing about you. Updates work in release builds that run from `gCode.app`,
not in development builds, and only on Apple Silicon.

## Menu bar

Only what is needed:

| Menu | Items |
| --- | --- |
| **gCode** | Settings…, Check for Updates…, Quit gCode |
| **File** | Add Project…, Go to File…, Save, Close Tab |
| **View** | Command Palette…, Toggle Terminal, Toggle AI Agents, New Codex Session, New Claude Session |
| **Repository** | **View on GitHub / GitLab / Azure DevOps** (the host's name for any other; disabled while the project has no remote) |
| **Profiles** | Every profile (checked: the one this window shows), Edit…, Add Profile… |

## Where things are stored

`~/Library/Application Support/gCode` — override with the `GCODE_DATA_DIR` environment variable. The files and when they are written are
listed in [Persistence](../architecture/persistence.md).
