#!/usr/bin/env bash
# Fail before a release build or publish unless the tag is exactly `v` plus
# the workspace package version, and that version is a semver. Extra
# characters, a refs/tags/ prefix, and any other tag are refused.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: check-release-tag.sh <tag>" >&2
  exit 2
fi

tag="$1"

root="$(cd "$(dirname "$0")/.." && pwd)"
version="$(
  awk '
    $0 ~ /^\[workspace\.package\]/ { in_pkg = 1; next }
    in_pkg && $0 ~ /^\[/ { exit }
    in_pkg && /^version[[:space:]]*=/ {
      line = $0
      sub(/^version[[:space:]]*=[[:space:]]*/, "", line)
      gsub(/"/, "", line)
      gsub(/[[:space:]]/, "", line)
      print line
      exit
    }
  ' "$root/Cargo.toml"
)"

if [[ -z "$version" ]]; then
  echo "refusing release: workspace package version not found in Cargo.toml" >&2
  exit 1
fi

semver='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
if [[ ! "$version" =~ $semver ]]; then
  echo "refusing release: Cargo.toml version ${version} is not a semver" >&2
  exit 1
fi

expected="v${version}"
if [[ "$tag" != "$expected" ]]; then
  echo "refusing release: tag ${tag} does not match Cargo.toml version ${version}" >&2
  exit 1
fi

echo "tag ${tag} matches Cargo.toml version ${version}"
