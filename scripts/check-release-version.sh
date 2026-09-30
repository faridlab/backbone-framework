#!/usr/bin/env bash
# Check that the workspace release version agrees with the release tags.
#
#   scripts/check-release-version.sh           # before tagging: field is not behind any tag
#   scripts/check-release-version.sh v2.7.35   # at release: field equals this tag
#
# `[workspace.metadata.release].version` in Cargo.toml is the one release
# version (ADR-0004). A tag whose version disagrees with it is refused by the
# release workflow, but that refusal only fires after the tag is pushed, and a
# pushed tag is already consumable as a git dependency. Running the no-argument
# form before tagging (and on every push/PR) catches a field that was not bumped
# while the release can still be fixed.
set -euo pipefail

cd "$(dirname "$0")/.."

field="$(cargo metadata --format-version 1 --no-deps \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["metadata"]["release"]["version"])')"

if [ "$#" -ge 1 ]; then
  tag_version="${1#v}"
  if [ "$tag_version" != "$field" ]; then
    echo "Tag '$1' (version '$tag_version') disagrees with" >&2
    echo "[workspace.metadata.release].version = '$field'." >&2
    echo "Bump the workspace version in Cargo.toml or retag." >&2
    exit 1
  fi
  echo "release version $field matches tag $1"
  exit 0
fi

latest_tag="$(git tag --list 'v[0-9]*' --sort=-v:refname | head -n 1)"
if [ -z "$latest_tag" ]; then
  echo "release version $field (no release tags yet)"
  exit 0
fi

# `sort -V` puts the lower version first; the field is behind when the latest
# tag sorts after it.
lowest="$(printf '%s\n%s\n' "$field" "${latest_tag#v}" | sort -V | head -n 1)"
if [ "$lowest" != "${latest_tag#v}" ]; then
  echo "[workspace.metadata.release].version = '$field' is behind the latest" >&2
  echo "release tag '$latest_tag'. Bump it to the next version before tagging," >&2
  echo "or the release workflow will refuse that tag." >&2
  exit 1
fi
echo "release version $field is not behind the latest tag $latest_tag"
