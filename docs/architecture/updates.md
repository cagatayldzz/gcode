# Updates

```text
 launch ─► ws_updater::check(current)  ──► Release?  ──► notification (once per tag)
                                                  │
                  Settings → About   ◄──────────┘
                    "Download & Install"  ► prepare(release, current_app)   verified, staged next to the app
                    "Restart to Update"   ► install()  ► SWAP_SCRIPT
```

## Check

`ws_updater::check` asks the GitHub API for the latest release of the repository in `[workspace.package]`, and
`ws_version::is_newer` decides whether it is newer than the running version (`CARGO_PKG_VERSION`, stamped by the release workflow).

The release's assets must be `gCode-<tag>-macos-arm64.zip` and `SHA256SUMS.txt` **under that repository's own download URLs**, so a tampered
answer cannot point the updater elsewhere.

## Download and verify

Only on request (`SettingsModel::download_update`) is the archive downloaded, next to the running app, and verified:

| Check | Tool |
| --- | --- |
| SHA-256 matches the checksum list | `SHA256SUMS.txt` |
| Valid signature | `codesign --verify` |
| Accepted by Gatekeeper | `spctl` |
| Same developer | the same Team ID as the installed app |
| Same app | the same bundle identifier, and the release's version |

## Install

**Restart to Update** (`restart_to_update`) quits and runs `SWAP_SCRIPT`, which waits for the app to exit, swaps `gCode.app` and reopens it,
**restoring the old app if the swap fails**.

## Rules

- All of it runs on background threads; progress reaches the model through a channel.
- Development builds (`debug_assertions`) never check or install.
- The first window checks at launch unless `check_updates` is off in `preferences.json`, and announces an available release once.
- Only Apple Silicon is supported; the update state lives in the shared settings model (`UpdateState`, `AppUpdate`).
