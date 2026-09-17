#!/bin/bash
set -euo pipefail

version=2.10.0
sha256=c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cache="$project_dir/target/sparkle/$version"
if [[ ! -f "$cache/.verified-$sha256" || ! -x "$cache/bin/generate_appcast" ]]; then
    mkdir -p "$(dirname "$cache")"
    staging=$(mktemp -d "$(dirname "$cache")/.download.XXXXXX")
    trap 'rm -rf -- "$staging"' EXIT
    curl --fail --location --retry 3 --silent --show-error \
        "https://github.com/sparkle-project/Sparkle/releases/download/$version/Sparkle-$version.tar.xz" \
        -o "$staging/archive.tar.xz"
    printf '%s  %s\n' "$sha256" "$staging/archive.tar.xz" | shasum -a 256 -c - >&2
    mkdir "$staging/distribution"
    tar -xf "$staging/archive.tar.xz" -C "$staging/distribution"
    touch "$staging/distribution/.verified-$sha256"
    rm -rf -- "$cache"
    mv "$staging/distribution" "$cache"
fi
printf '%s\n' "$cache"
