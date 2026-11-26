# Extensions

The app lists these in its Extensions view and installs them on request. An extension is a package published here: a `manifest.json`,
listed in `index.json` with the SHA-256 of the manifest (`shasum -a 256 <id>/manifest.json`). **Change a manifest and its hash in the same
commit**, and raise `version`: the app refuses a download that does not match the index.

**Versions are semantic** (`major.minor.patch`, as the app's own releases) and an extension starts at `0.1.0`. Raise the patch for a fix, the minor
for a new capability (a new agent in the manifest, say) and the major for a change that breaks what the extension did before. The index and the
manifest must say the same version (a test checks it), and the app offers an *Update* only when the index's version is higher than the installed one.
A draft stays at `0.0.0` until it has a package.

A package carries no code. It switches on a feature that is in the app and chooses among what that feature offers (for the AI Agent:
which agents to show). What a command runs is decided by the app, never by a manifest.

An entry with `"status": "draft"` and no `package`/`sha256` is listed as *Coming soon* and cannot be installed (Spotify and YouTube Music are drafts).
