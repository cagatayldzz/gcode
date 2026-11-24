#!/bin/bash
# The workflows keep the guarantees the release process relies on.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/tests/lib.sh

ci="$(cat .github/workflows/ci.yml)"
release="$(cat .github/workflows/release.yml)"
action="$(cat .github/actions/build-release/action.yml)"

assert_contains "pull_request:" "$ci" "CI runs on pull requests"
assert_contains "bash scripts/tests/run.sh" "$ci" "CI runs the shell tests"
assert_contains "cargo test --locked -p ws-terminal -p ws-git -p ws-storage -p ws-commitgen -p ws-language -p ws-activity -p ws-hosting -p ws-version -p ws-updater -p ws-index" "$ci" "CI tests the core crates"
assert_contains "-D warnings" "$ci" "CI treats warnings as errors"

assert_contains "workflow_dispatch:" "$release" "release is started by hand"
assert_contains "github.ref_name == github.event.repository.default_branch" "$release" "release runs from the default branch only"
assert_contains "cancel-in-progress: false" "$release" "a running release is never cancelled"
assert_contains "needs: [prepare, verify]" "$release" "publish waits for verification"
assert_contains "needs: [validate, prepare, macos_arm64, macos_checks]" "$release" "verification waits for validation, the build and the checks"
assert_contains "cargo test --locked --workspace" "$release" "the workspace is tested while the app builds"
assert_contains "--draft" "$release" "the release is created as a draft first"
assert_contains "sha256sum --check SHA256SUMS.txt" "$release" "checksums are verified before publishing"
assert_contains "bash scripts/release.sh finalize" "$release" "assets are finalized by the release script"

cache_line="$(grep -n 'rust-cache' <<<"$action" | head -n 1 | cut -d: -f1)"
inputs_line="$(grep -n 'release-inputs' <<<"$action" | head -n 1 | cut -d: -f1)"
[[ "$cache_line" -lt "$inputs_line" ]] || { echo "FAIL: the Rust cache is restored before the stamped version files are downloaded" >&2; failures=$((failures + 1)); }

assert_contains "bash scripts/notarize-app.sh" "$action" "the build notarizes the app"
assert_contains "bash scripts/release.sh package" "$action" "the build packages with the release script"

for variable in APPLE_CERTIFICATE_BASE64 APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_TEAM_ID APPLE_APP_SPECIFIC_PASSWORD; do
  assert_contains "secrets.$variable" "$release" "release reads $variable"
done

# App and release automation use shell and Rust. VitePress uses Bun in the documentation workflow.
for file in .github/workflows/*.yml .github/actions/*/action.yml scripts/*.sh; do
  [[ "$file" == .github/workflows/docs.yml ]] && continue
  if grep -Eq '(^|[^a-z])(bun|npm|node|python3?|ruby)( |$)' "$file"; then
    echo "FAIL: $file uses a runtime other than shell and Rust" >&2
    failures=$((failures + 1))
  fi
done

finish
