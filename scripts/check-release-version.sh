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

metadata="$(cargo metadata --format-version 1 --no-deps)"
field="$(printf '%s' "$metadata" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["metadata"]["release"]["version"])')"

# Every crate is published under the release version (metaphora ADR-0030), and
# a dependency between member crates must ask for that same version, or the
# published crates would point at a sibling release that was never cut.
# scripts/set-release-version.sh writes all three at once.
printf '%s' "$metadata" | python3 -c '
import json, sys
field = sys.argv[1]
meta = json.load(sys.stdin)
names = {p["name"] for p in meta["packages"]}
bad = []
for p in meta["packages"]:
    name, version = p["name"], p["version"]
    if version != field:
        bad.append(name + " is version " + version)
    for d in p["dependencies"]:
        if d["name"] in names and d.get("path") and d["req"] != "^" + field:
            bad.append(name + " asks for " + d["name"] + " " + d["req"])
if bad:
    print("Crates disagree with [workspace.metadata.release].version = " + repr(field) + ":", file=sys.stderr)
    for b in bad:
        print("  " + b, file=sys.stderr)
    print("Run scripts/set-release-version.sh " + field + " to align them.", file=sys.stderr)
    sys.exit(1)
' "$field"

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
