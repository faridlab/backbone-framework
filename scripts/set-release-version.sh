#!/usr/bin/env bash
# Set the one release version everywhere it is written.
#
#   scripts/set-release-version.sh 2.7.36
#
# The framework is released as one version (ADR-0004), and each crate is
# published to crates.io under that version (metaphora ADR-0030). This writes
# it to `[workspace.metadata.release].version`, to every member crate's
# `version`, and to the version requirement on every dependency between member
# crates, so a release bump is one command and scripts/check-release-version.sh
# can hold the three in agreement.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ "$#" -ne 1 ] || ! printf '%s' "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
  echo "usage: scripts/set-release-version.sh <major.minor.patch>" >&2
  exit 2
fi

python3 - "$1" <<'EOF'
import json, re, subprocess, sys

version = sys.argv[1]
meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--no-deps"],
    capture_output=True, text=True, check=True).stdout)
members = {p["name"]: p["manifest_path"] for p in meta["packages"]}

root = meta["workspace_root"] + "/Cargo.toml"
text = open(root).read()
text, n = re.subn(r'(?m)^(\[workspace\.metadata\.release\]\nversion = )"[^"]*"',
                  rf'\g<1>"{version}"', text)
if n != 1:
    sys.exit("could not find [workspace.metadata.release].version in Cargo.toml")
open(root, "w").write(text)

member_dep = re.compile(
    r'(?m)^(?P<key>' + "|".join(map(re.escape, members)) + r')\s*=\s*\{(?P<body>[^}\n]*\bpath\s*=[^}\n]*)\}')

def with_version(match):
    body = match.group("body")
    if re.search(r'\bversion\s*=', body):
        body = re.sub(r'\bversion\s*=\s*"[^"]*"', f'version = "{version}"', body)
    else:
        body = re.sub(r'(\bpath\s*=\s*"[^"]*")', rf'\1, version = "{version}"', body, count=1)
    return f'{match.group("key")} = {{{body}}}'

for manifest in members.values():
    text = open(manifest).read()
    text, n = re.subn(r'(?ms)(^\[package\]\n.*?^version = )"[^"]*"', rf'\g<1>"{version}"', text, count=1)
    if n != 1:
        sys.exit(f"could not find the package version in {manifest}")
    text = member_dep.sub(with_version, text)
    open(manifest, "w").write(text)

print(f"release version, {len(members)} crate versions and member dependencies set to {version}")
EOF
