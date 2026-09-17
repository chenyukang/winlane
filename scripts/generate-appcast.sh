#!/bin/bash
set -euo pipefail

if [[ $# != 2 || ! "$2" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    printf 'Usage: %s PACKAGE_DIR VERSION\n' "${0##*/}" >&2
    exit 2
fi
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output=$(cd -- "$1" && pwd)
version=$2
sparkle_dir=$("$project_dir/scripts/fetch-sparkle.sh")
staging=$(mktemp -d "${TMPDIR:-/tmp}/winlane-appcast.XXXXXX")
trap 'rm -rf -- "$staging"' EXIT

sparkle() {
    local tool=$1
    shift
    if [[ -n ${SPARKLE_ED_KEY:-} ]]; then
        printf '%s' "$SPARKLE_ED_KEY" | "$sparkle_dir/bin/$tool" --ed-key-file - "$@"
    else
        "$sparkle_dir/bin/$tool" --account app.windowlane.desktop "$@"
    fi
}

count=0
for arch in arm64 x86_64; do
    filename="Winlane-$version-macos-$arch.zip"
    [[ -f "$output/$filename" ]] || continue
    directory="$staging/$arch"
    mkdir "$directory"
    cp "$output/$filename" "$directory/$filename"
    sparkle generate_appcast --maximum-deltas 0 --maximum-versions 1 \
        --download-url-prefix "https://github.com/chenyukang/winlane/releases/download/v$version/" \
        --link 'https://github.com/chenyukang/winlane' \
        --full-release-notes-url "https://github.com/chenyukang/winlane/releases/tag/v$version" \
        -o "$directory/appcast-$arch.xml" "$directory"
    sparkle sign_update --verify "$directory/appcast-$arch.xml"
    swift "$project_dir/scripts/verify-appcast.swift" "$directory/appcast-$arch.xml" \
        "$output/$filename" "$project_dir/resources/sparkle-public-key.txt" "$version" "$arch"
    count=$((count + 1))
done
if [[ $count == 0 ]]; then
    printf 'No update archives found for version %s.\n' "$version" >&2
    exit 1
fi
# Publish feeds only after every generated entry and signature passes validation.
for arch in arm64 x86_64; do
    if [[ -f "$staging/$arch/appcast-$arch.xml" ]]; then
        cp "$staging/$arch/appcast-$arch.xml" "$output/appcast-$arch.xml"
    fi
done
