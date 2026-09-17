#!/bin/bash
set -euo pipefail

if [[ $# != 1 ]]; then
    printf 'Usage: %s APP_BUNDLE\n' "${0##*/}" >&2
    exit 2
fi
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bundle=$(cd -- "$1" && pwd)
staging=$(mktemp -d "${TMPDIR:-/tmp}/winlane-updater-test.XXXXXX")
identifier="app.windowlane.updater-test.$(uuidgen)"
cleanup() {
    defaults delete "$identifier" >/dev/null 2>&1 || true
    rm -rf -- "$staging"
}
trap cleanup EXIT
RUSTC_WRAPPER='' cargo test --manifest-path "$project_dir/Cargo.toml" --locked \
    --test native_panels --no-run --message-format=json > "$staging/cargo.json"
binary=$(python3 - "$staging/cargo.json" <<'PY'
import json, sys
for line in open(sys.argv[1]):
    item = json.loads(line)
    if item.get('executable') and item.get('target', {}).get('name') == 'native_panels':
        print(item['executable'])
PY
)
test_app="$staging/Probe.app"
ditto "$bundle" "$test_app"
cp "$binary" "$test_app/Contents/MacOS/winlane"
plist="$test_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $identifier" "$plist"
/usr/libexec/PlistBuddy -c 'Set :SUEnableAutomaticChecks false' "$plist"
for expectation in enabled invalid disabled; do
    case "$expectation" in
        invalid) rm -rf "$test_app/Contents/Frameworks/Sparkle.framework" ;;
        disabled) /usr/libexec/PlistBuddy -c 'Delete :SUFeedURL' "$plist" ;;
    esac
    codesign --force --sign - --identifier "$identifier" "$test_app"
    WINLANE_UPDATER_SMOKE="$expectation" "$test_app/Contents/MacOS/winlane"
done
