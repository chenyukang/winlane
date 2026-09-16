#!/bin/bash
set -euo pipefail

if [[ $# != 2 ]]; then
    printf 'Usage: %s PATH/Winlane.app OUTPUT_DIR\n' "${0##*/}" >&2
    exit 2
fi
bundle=$1
output_dir=$2
plist="$bundle/Contents/Info.plist"
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")
identifier=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "$identifier" == app.windowlane.desktop ]] || {
    printf 'Expected a versioned Winlane app bundle.\n' >&2; exit 1;
}
arch=$(/usr/bin/lipo -archs "$bundle/Contents/MacOS/winlane")
case "$arch" in
    arm64|x86_64) ;;
    *) printf 'Unsupported bundle architecture: %s\n' "$arch" >&2; exit 1 ;;
esac
/usr/bin/codesign --verify --strict "$bundle"
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
name="Winlane-$version-macos-$arch"
for extension in zip dmg; do
    [[ ! -e "$output_dir/$name.$extension" ]] || {
        printf 'Refusing to overwrite %s\n' "$output_dir/$name.$extension" >&2; exit 1;
    }
done
staging_dir=$(mktemp -d "$output_dir/.package.XXXXXX")
trap 'rm -rf -- "$staging_dir"' EXIT
mkdir "$staging_dir/image"
/usr/bin/ditto "$bundle" "$staging_dir/image/Winlane.app"
app="$staging_dir/image/Winlane.app"

notarize() {
    local artifact=$1
    local result="$staging_dir/notary-result.json"
    xcrun notarytool submit "$artifact" --keychain-profile "$WINLANE_NOTARY_PROFILE" \
        --keychain "$WINLANE_SIGNING_KEYCHAIN" --wait --timeout 30m --output-format json > "$result"
    python3 - "$result" <<'PY'
import json, sys
result = json.load(open(sys.argv[1]))
if result.get("status") != "Accepted":
    sys.exit(f"Notarization failed: {result}")
PY
}

if [[ -n ${WINLANE_NOTARY_PROFILE:-} ]]; then
    : "${WINLANE_SIGNING_IDENTITY:?Notarization requires a Developer ID identity}"
    : "${WINLANE_SIGNING_KEYCHAIN:?Notarization requires a keychain}"
    /usr/bin/ditto -c -k --sequesterRsrc --keepParent "$app" "$staging_dir/notarize.zip"
    notarize "$staging_dir/notarize.zip"
    xcrun stapler staple "$app"
    xcrun stapler validate "$app"
    /usr/sbin/spctl --assess --type execute --verbose "$app"
fi

# Archive the stapled app so both download formats can be checked offline.
/usr/bin/ditto -c -k --sequesterRsrc --keepParent "$app" "$staging_dir/$name.zip"
ln -s /Applications "$staging_dir/image/Applications"
/usr/bin/hdiutil create -quiet -volname "Winlane $version" -srcfolder "$staging_dir/image" \
    -format UDZO "$staging_dir/$name.dmg"
if [[ -n ${WINLANE_NOTARY_PROFILE:-} ]]; then
    /usr/bin/codesign --force --sign "$WINLANE_SIGNING_IDENTITY" \
        --keychain "$WINLANE_SIGNING_KEYCHAIN" --timestamp "$staging_dir/$name.dmg"
    notarize "$staging_dir/$name.dmg"
    xcrun stapler staple "$staging_dir/$name.dmg"
    xcrun stapler validate "$staging_dir/$name.dmg"
fi
/usr/bin/hdiutil verify -quiet "$staging_dir/$name.dmg"
/usr/bin/ditto -x -k "$staging_dir/$name.zip" "$staging_dir/unpacked"
/usr/bin/codesign --verify --strict "$staging_dir/unpacked/Winlane.app"
cmp "$app/Contents/MacOS/winlane" "$staging_dir/unpacked/Winlane.app/Contents/MacOS/winlane"
test -x "$staging_dir/unpacked/Winlane.app/Contents/MacOS/winlane"
mv "$staging_dir/$name.zip" "$staging_dir/$name.dmg" "$output_dir/"
printf 'Packaged: %s/%s.{zip,dmg}\n' "$output_dir" "$name"
