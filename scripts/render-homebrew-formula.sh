#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 5 ]]; then
  echo "usage: $0 VERSION SHA_MACOS_ARM64 SHA_MACOS_X86_64 SHA_LINUX_ARM64 SHA_LINUX_X86_64" >&2
  exit 2
fi

version="$1"
sha_macos_arm64="$2"
sha_macos_x86_64="$3"
sha_linux_arm64="$4"
sha_linux_x86_64="$5"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
  echo "invalid semantic version: $version" >&2
  exit 2
fi

for checksum in "$sha_macos_arm64" "$sha_macos_x86_64" "$sha_linux_arm64" "$sha_linux_x86_64"; do
  if [[ ! "$checksum" =~ ^[0-9a-f]{64}$ ]]; then
    echo "invalid SHA-256 checksum: $checksum" >&2
    exit 2
  fi
done

template="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/packaging/homebrew/memento.rb.in"
sed \
  -e "s/@VERSION@/$version/g" \
  -e "s/@SHA_MACOS_ARM64@/$sha_macos_arm64/g" \
  -e "s/@SHA_MACOS_X86_64@/$sha_macos_x86_64/g" \
  -e "s/@SHA_LINUX_ARM64@/$sha_linux_arm64/g" \
  -e "s/@SHA_LINUX_X86_64@/$sha_linux_x86_64/g" \
  "$template"
