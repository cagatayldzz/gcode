# Releases & CI

## Releasing

`bun run release` (or running the **Release** workflow by hand) releases the **pushed default branch**.

The version comes from Conventional Commits since the latest `v*` tag:

| Commit | Bump |
| --- | --- |
| `feat!:` / `BREAKING CHANGE:` | **major** |
| `feat:` | **minor** |
| everything else | **patch** |

```text
 release workflow
   1 Validate   Apple secrets present · shell tests · cargo fmt · clippy+tests of terminal, git, storage
   2 Prepare    choose the version · stamp Cargo.toml, Cargo.lock, package.json, Info.plist (in CI only)
                · write RELEASE_NOTES.md (New Features / Bug Fixes / Documentation / Chores + changelog link)
   3 Build      ONE macOS job: release build · Developer ID signature · notarization
                · gCode-<tag>-macos-arm64.zip · then workspace clippy and tests on the same runner
   4 Verify     expected archive, SHA256SUMS.txt and install.sh
   5 Publish    checksums verified · draft GitHub release · published as latest
```

The version in the repository is **stamped only in CI**; the tag is the source of truth.

Required Actions secrets:

| Secret | For |
| --- | --- |
| `APPLE_CERTIFICATE_BASE64` | the Developer ID certificate |
| `APPLE_CERTIFICATE_PASSWORD` | its password |
| `APPLE_ID` | notarization |
| `APPLE_TEAM_ID` | signing and the updater's same-developer check |
| `APPLE_APP_SPECIFIC_PASSWORD` | notarization |

The updater's required assets are `gCode-<tag>-macos-arm64.zip` and `SHA256SUMS.txt`: the in-app updater only trusts those names under the
repository's own download URLs (see [Updates](../architecture/updates.md)).

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `main` and every pull request:

| Job | Runner | Does |
| --- | --- | --- |
| `core` | Linux | shell tests, `cargo fmt --check`, clippy (warnings are errors) and tests of the plain-Rust crates |
| `interface` | macOS | `cargo check` of `ws-ui` and `ws-app`, and the headless `window_render` test |

The GPUI crates need macOS, so the full workspace is linted and tested by the release workflow's build job.

## Documentation site

The site is [VitePress](https://vitepress.dev) in `docs/`.

```sh
bun install
bun run docs:dev        # edit locally with hot reload
bun run docs:build      # static site in docs-site/
bun run docs:preview    # serve that build
```

For GitHub Pages set **Settings → Pages → Build and deployment → Source** to **GitHub Actions**. The **Deploy documentation** workflow
(`.github/workflows/docs.yml`) builds and publishes the site when `docs/**` changes on `main`, and can be run by hand from **Actions**. The base
path comes from `DOCS_BASE` (default `/`; the workflow passes the repository's Pages path).

## Screenshots

The images in `docs/images` show a **staged demo project** ("atlas"), so that no real name, machine name, path or terminal history appears.
To redo them:

1. Make a scratch home with a few small Git repositories (one with uncommitted changes) in `~/Developer`.
2. Put a `.zshrc` in a scratch `ZDOTDIR` that clears the screen and sets a plain prompt such as `atlas main $ `, and a `projects.json` and a
   `preferences.json` (`{"theme":"dark"}` or `"light"`) in a scratch `GCODE_DATA_DIR`.
3. Run `HOME=<scratch home> ZDOTDIR=<scratch zsh> GCODE_DATA_DIR=<scratch data> ./target/debug/ws-desktop`, size the window to about 1040×660, open
   the file or diff to show, then capture only the window without its shadow (`screencapture -o -l <window id>`).

**Check every image for names, paths and dates before committing it** — the status bar shows the home folder as `~`.

## Visual inspection with a computer-use agent

Only when explicitly requested: run `bun run dev` with a scratch `GCODE_DATA_DIR`, select the development app by bundle ID
`com.cagatayldzz.gcode.dev`, inspect its accessibility tree and take a fresh window screenshot. Prefer accessible controls; use coordinates
from the current screenshot when a control is not exposed. After each action, inspect the new state before continuing. The operating system
may require Screen Recording and Accessibility permissions for the host.

For precise color or coordinate inspection of a saved PNG:

```sh
cargo run -p ws-visual-check -- /tmp/window.png 200 100 800 600
```

It prints the image size, the RGBA value at image pixel `(200, 100)`, and that pixel's center in an 800 × 600 logical-point window. It assumes
the PNG contains exactly the window, without shadow or cropping, and returns coordinates **relative to the window** — add the window's screen
origin when a tool expects screen coordinates. The tool does not capture the screen or send input. Screenshots can contain private paths or
terminal history: keep them in a scratch location and never commit them.


Each release also publishes a version-specific `install.sh` that reads the archive checksum from `SHA256SUMS.txt`.
Download the installer from the release's Assets and run `bash install.sh`. It checks the archive's SHA-256 and macOS signature
before installing into `~/Applications`. Existing installs in that folder use the in-app updater.
