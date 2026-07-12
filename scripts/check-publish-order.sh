#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
publish_order=(src/domain src/app src/infra src/ui .)
metadata=$(cargo metadata --no-deps --format-version 1)

listed_package_file=$(mktemp)
trap 'rm -f "$listed_package_file"' EXIT

for crate in "${publish_order[@]}"; do
    if [[ "$crate" == "." ]]; then
        manifest="$repo_root/Cargo.toml"
    else
        manifest="$repo_root/$crate/Cargo.toml"
    fi
    if [[ ! -f "$manifest" ]]; then
        echo "missing manifest: $crate" >&2
        exit 1
    fi

    package_name=$(jq -r --arg manifest "$manifest" \
        '.packages[] | select(.manifest_path == $manifest) | .name' <<<"$metadata")
    if [[ -z "$package_name" ]]; then
        echo "manifest is not a workspace package: $crate" >&2
        exit 1
    fi
    printf '%s\n' "$package_name" >>"$listed_package_file"
done

listed_packages=$(LC_ALL=C sort -u "$listed_package_file")
publishable_packages=$(jq -r '.packages[] | select(.publish == null) | .name' <<<"$metadata" | LC_ALL=C sort -u)
if [[ "$listed_packages" != "$publishable_packages" ]]; then
    echo "publish order does not cover every publishable workspace package" >&2
    exit 1
fi

printf '%s\n' "${publish_order[@]}"
