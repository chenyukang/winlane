#!/bin/bash
set -euo pipefail

usage() {
    printf 'Usage: %s [--debug] [--adhoc | --distribution] [--with-updater] [--target TRIPLE] [--output-dir DIR]\n' "${0##*/}"
    printf 'Build dist/Winlane.app using the Windowlane Development signing identity.\n'
    printf 'Override with WINLANE_SIGNING_IDENTITY (exact certificate name or SHA-1).\n'
    printf 'Use --adhoc only for disposable builds; permissions may reset after rebuilding.\n'
    printf 'Use --distribution with a Developer ID Application identity for notarization.\n'
    printf 'Use --with-updater for distributable builds with Sparkle (downloads a pinned framework).\n'
    printf 'The app is not installed or launched. See docs/development.md for one-time certificate setup.\n'
}

profile=release
build_args=(--locked --release)
adhoc=false
distribution=false
with_updater=false
target=$(rustc -vV | sed -n 's/^host: //p')
output_dir=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug) profile=debug; build_args=(--locked) ;;
        --adhoc) adhoc=true ;;
        --distribution) distribution=true ;;
        --with-updater) with_updater=true ;;
        --target|--output-dir)
            [[ $# -ge 2 && -n "$2" ]] || { usage >&2; exit 2; }
            if [[ "$1" == --target ]]; then target=$2; else output_dir=$2; fi
            shift ;;
        --help|-h) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
    shift
done
if [[ $(uname -s) != Darwin ]]; then
    printf 'Winlane.app must be built on macOS.\n' >&2
    exit 1
fi

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output_dir=${output_dir:-$project_dir/dist}
case "$target" in
    aarch64-apple-darwin|x86_64-apple-darwin) ;;
    *) printf 'Unsupported Rust target: %s\n' "$target" >&2; exit 2 ;;
esac
if [[ "$distribution" == true && ( "$adhoc" == true || "$profile" == debug ) ]]; then
    printf '%s\n' '--distribution requires a release build and a Developer ID identity.' >&2
    exit 2
fi
signing_identity=-
if [[ "$adhoc" == false ]]; then
    requested_identity=${WINLANE_SIGNING_IDENTITY:-${WINDOWLANE_SIGNING_IDENTITY:-Windowlane Development}}
    if [[ -n ${WINLANE_SIGNING_KEYCHAIN:-} ]]; then
        identities=$(/usr/bin/security find-identity -p codesigning "$WINLANE_SIGNING_KEYCHAIN")
    else
        identities=$(/usr/bin/security find-identity -p codesigning)
    fi
    signing_identity=$(printf '%s\n' "$identities" |
        sed -nE 's/^[[:space:]]*[0-9]+\) ([[:xdigit:]]{40}) "([^"]*)".*$/\1|\2/p' |
        awk -F '|' -v wanted="$requested_identity" \
            '$2 == wanted || toupper($1) == toupper(wanted) { print $1 }' |
        sort -u)
    if [[ ! "$signing_identity" =~ ^[[:xdigit:]]{40}$ ]]; then
        printf 'Expected one code-signing identity matching: %s\n' "$requested_identity" >&2
        printf 'Create Windowlane Development in Keychain Access (docs/development.md: development signing).\n' >&2
        printf 'For multiple matches, set WINLANE_SIGNING_IDENTITY to the certificate SHA-1.\n' >&2
        printf 'Refusing to silently fall back to ad hoc signing.\n' >&2
        exit 1
    fi
    if [[ "$distribution" == true ]] && ! printf '%s\n' "$identities" |
        grep -Eq "$signing_identity \"Developer ID Application: "; then
        printf 'Distribution requires a Developer ID Application certificate.\n' >&2
        exit 1
    fi
else
    printf 'Warning: ad hoc signing may require granting permissions again after code changes.\n' >&2
fi
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$project_dir/Cargo.toml" | head -n 1)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    printf 'Cargo.toml must contain a numeric major.minor.patch package version.\n' >&2
    exit 1
fi

printf 'Building Winlane %s (%s, %s)…\n' "$version" "$target" "$profile"
RUSTC_WRAPPER='' MACOSX_DEPLOYMENT_TARGET=14.0 CARGO_TARGET_DIR="$project_dir/target" \
    cargo build --manifest-path "$project_dir/Cargo.toml" \
    --target "$target" "${build_args[@]}"

mkdir -p "$output_dir"
staging_dir=$(mktemp -d "$output_dir/.winlane.XXXXXX")
trap 'rm -rf -- "$staging_dir"' EXIT
bundle="$staging_dir/Winlane.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$project_dir/target/$target/$profile/winlane" "$bundle/Contents/MacOS/winlane"
chmod 755 "$bundle/Contents/MacOS/winlane"

cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Winlane</string>
    <key>CFBundleDisplayName</key><string>Winlane</string>
    <key>CFBundleIdentifier</key><string>app.windowlane.desktop</string>
    <key>CFBundleExecutable</key><string>winlane</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleLocalizations</key><array><string>en</string><string>zh-Hans</string></array>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSAutoFillRequiresTextContentTypeForOneTimeCodeOnMac</key><true/>
</dict>
</plist>
PLIST

if [[ -f "$project_dir/resources/AppIcon.icns" ]]; then
    cp "$project_dir/resources/AppIcon.icns" "$bundle/Contents/Resources/AppIcon.icns"
    /usr/libexec/PlistBuddy -c 'Add :CFBundleIconFile string AppIcon.icns' "$bundle/Contents/Info.plist"
fi
/usr/bin/plutil -lint "$bundle/Contents/Info.plist"
if [[ "$with_updater" == true ]]; then
    sparkle_dir=$("$project_dir/scripts/fetch-sparkle.sh")
    framework="$bundle/Contents/Frameworks/Sparkle.framework"
    mkdir -p "$bundle/Contents/Frameworks"
    ditto "$sparkle_dir/Sparkle.framework" "$framework"
    # Winlane is not sandboxed, so neither Sparkle XPC service is needed.
    rm -rf "$framework/Versions/B/XPCServices" "$framework/XPCServices"
    cp "$sparkle_dir/LICENSE" "$bundle/Contents/Resources/Sparkle-LICENSE.txt"
    arch=arm64
    if [[ "$target" == x86_64-apple-darwin ]]; then arch=x86_64; fi
    python3 - "$bundle/Contents/Info.plist" "$project_dir/resources/sparkle-public-key.txt" "$arch" <<'PY'
import base64, pathlib, plistlib, sys
path, key_path, arch = sys.argv[1:]
key = pathlib.Path(key_path).read_text().strip()
if len(base64.b64decode(key, validate=True)) != 32:
    raise SystemExit('Sparkle public key must contain 32 bytes.')
with open(path, 'rb') as stream:
    info = plistlib.load(stream)
info.update({
    'SUFeedURL': f'https://github.com/chenyukang/winlane/releases/latest/download/appcast-{arch}.xml',
    'SUPublicEDKey': key,
    'SUEnableAutomaticChecks': True,
    'SUScheduledCheckInterval': 86400,
    'SUAutomaticallyUpdate': False,
    'SUAllowsAutomaticUpdates': False,
    'SUEnableSystemProfiling': False,
    'SUVerifyUpdateBeforeExtraction': True,
    'SURequireSignedFeed': True,
})
with open(path, 'wb') as stream:
    plistlib.dump(info, stream)
PY
fi
printf 'Signing identity: %s\n' "$signing_identity"
sign_args=(--timestamp=none)
if [[ "$distribution" == true ]]; then
    sign_args=(--timestamp --options runtime)
fi
if [[ -n ${WINLANE_SIGNING_KEYCHAIN:-} ]]; then
    sign_args+=(--keychain "$WINLANE_SIGNING_KEYCHAIN")
fi
if [[ "$with_updater" == true ]]; then
    # Sign nested code inside out. Hardened runtime is only used with Developer ID.
    for component in "$framework/Versions/B/Autoupdate" "$framework/Versions/B/Updater.app" "$framework"; do
        /usr/bin/codesign --force --sign "$signing_identity" "${sign_args[@]}" "$component"
    done
fi
/usr/bin/codesign --force --sign "$signing_identity" "${sign_args[@]}" --identifier app.windowlane.desktop "$bundle"
/usr/bin/codesign --verify --deep --strict "$bundle"
/usr/bin/codesign --display --requirements - "$bundle"

destination="$output_dir/Winlane.app"
if [[ -e "$destination" || -L "$destination" ]]; then
    rm -rf -- "$destination"
fi
mv -- "$bundle" "$destination"
printf 'Built: %s\nArchitecture: %s\nThe app has not been launched.\n' "$destination" "$target"
