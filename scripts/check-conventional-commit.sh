#!/usr/bin/env bash
# One source of truth for Conventional Commit subjects, used by CI for pull
# request titles and for every commit that lands on main.
set -euo pipefail

pattern='^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-zA-Z0-9._/-]+\))?!?: .+$'

if [[ $# -lt 1 ]]; then
  echo "usage: check-conventional-commit.sh <subject>..." >&2
  exit 2
fi

fail=0
for subject in "$@"; do
  if [[ ! "$subject" =~ $pattern ]]; then
    echo "not a conventional commit: ${subject}" >&2
    fail=1
  else
    echo "ok: ${subject}"
  fi
done

exit "$fail"
