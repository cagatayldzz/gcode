# Extensions

The app lists these in its Extensions view and installs them on request. An extension is a package published here: a `manifest.json`,
listed in `index.json` with the SHA-256 of the manifest (`shasum -a 256 <id>/manifest.json`). **Change a manifest and its hash in the same
commit**, and raise `version`: the app refuses a download that does not match the index.

A package carries no code. It switches on a feature that is in the app and chooses among what that feature offers (for the AI Agent:
which agents to show). What a command runs is decided by the app, never by a manifest.
