#!/usr/bin/env bash
# Fail before a release build or publish when a tag's version disagrees with
# the workspace package version. The tag may be `v0.1.0` or `refs/tags/v0.1.0`.
# The compared value is the tag with one leading `v` removed.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: check-release-tag.sh <tag>" >&2
  exit 2
fi

tag="$1"
tag="${tag#refs/tags/}"
if [[ "$tag" != v* ]]; then
  echo "refusing release: tag '${tag}' must start with v" >&2
  exit 1
fi
stripped="${tag#v}"

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

if [[ "$stripped" != "$version" ]]; then
  echo "refusing release: tag ${tag} (version ${stripped}) does not match Cargo.toml version ${version}" >&2
  exit 1
fi

echo "tag ${tag} matches Cargo.toml version ${version}"
