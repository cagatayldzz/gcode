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
assert_contains "cargo test --locked -p vekro-terminal -p vekro-git -p vekro-storage" "$ci" "CI tests the core crates"
assert_contains "-D warnings" "$ci" "CI treats warnings as errors"

assert_contains "workflow_dispatch:" "$release" "release is started by hand"
assert_contains "github.ref_name == github.event.repository.default_branch" "$release" "release runs from the default branch only"
assert_contains "cancel-in-progress: false" "$release" "a running release is never cancelled"
assert_contains "needs: [prepare, verify]" "$release" "publish waits for verification"
assert_contains "needs: [prepare, macos_checks, macos_arm64]" "$release" "verification waits for checks and the build"
assert_contains "--draft" "$release" "the release is created as a draft first"
assert_contains "sha256sum --check SHA256SUMS.txt" "$release" "checksums are verified before publishing"
assert_contains "bash scripts/release.sh finalize" "$release" "assets are finalized by the release script"

assert_contains "bash scripts/notarize-app.sh" "$action" "the build notarizes the app"
assert_contains "bash scripts/release.sh package" "$action" "the build packages with the release script"

for variable in APPLE_CERTIFICATE_BASE64 APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_TEAM_ID APPLE_APP_SPECIFIC_PASSWORD; do
  assert_contains "secrets.$variable" "$release" "release reads $variable"
done

# Only shell and Rust: no other scripting runtimes in automation.
for file in .github/workflows/*.yml .github/actions/*/action.yml scripts/*.sh; do
  if grep -Eq '(^|[^a-z])(bun|npm|node|python3?|ruby)( |$)' "$file"; then
    echo "FAIL: $file uses a runtime other than shell and Rust" >&2
    failures=$((failures + 1))
  fi
done

finish
