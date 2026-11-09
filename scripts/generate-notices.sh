#!/usr/bin/env bash
# Generate the per-crate third-party notice bundle (ticket 19).
#
# Output: packaging/THIRD_PARTY_NOTICES.txt
#
# Input: `cargo metadata --format-version 1 --locked`, so generation is
# offline and uses no new Rust dependencies. For every non-workspace package
# the bundle records the name, version, licence expression, repository, and
# the licence text(s) shipped with the package source. Byte-identical texts
# are deduplicated by SHA-256 while every package still names the text section
# it uses. Packages with no licence file keep their SPDX expression and get a
# `NO LICENCE FILE` marker; the generator never invents copyright text.
#
# Usage:
#   scripts/generate-notices.sh          regenerate the committed bundle
#   scripts/generate-notices.sh --check  fail if the committed bundle is stale
#
# The output is committed, so release builds and tests never need network
# access. CI runs `--check` after the build has populated the registry cache
# (CARGO_NET_OFFLINE=true); maintainers rerun the generator when Cargo.lock
# changes and commit the result.
#
# Requirements: bash, cargo, python3 (standard library only).

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

usage() {
  echo "usage: scripts/generate-notices.sh [--check]" >&2
}

check=0
case "$#:${1:-}" in
  0:) ;;
  1:--check) check=1 ;;
  *)
    usage
    exit 2
    ;;
esac

output="packaging/THIRD_PARTY_NOTICES.txt"
tmp="$(mktemp "${TMPDIR:-/tmp}/vfe-notices.XXXXXX")"
trap 'rm -f "$tmp"' EXIT

python3 - "$tmp" <<'PY'
"""Render the dependency graph's licence texts into one deterministic bundle."""

import hashlib
import json
import os
import re
import subprocess
import sys

target = sys.argv[1]
license_prefixes = ("LICENSE", "COPYING", "NOTICE")


def fail(message):
    sys.stderr.write(f"generate-notices: {message}\n")
    raise SystemExit(1)


def cargo_metadata():
    try:
        result = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except FileNotFoundError:
        fail("cargo is not installed")
    except subprocess.CalledProcessError as error:
        if error.stderr:
            sys.stderr.write(error.stderr)
        fail("cargo metadata --format-version 1 --locked failed")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        fail(f"cargo metadata returned invalid JSON: {error}")


def version_key(version):
    """Order numeric release components numerically and text components last."""
    parts = []
    for part in re.split(r"(\d+)", version):
        if not part:
            continue
        parts.append((0, int(part)) if part.isdigit() else (1, part))
    return tuple(parts)


def license_files(package):
    """License file paths for a package: metadata first, then a root scan."""
    package_root = os.path.dirname(package["manifest_path"])
    declared = package.get("license_file")
    if declared:
        path = declared if os.path.isabs(declared) else os.path.join(package_root, declared)
        if os.path.isfile(path):
            return [(os.path.basename(path), path)]
        sys.stderr.write(
            f"generate-notices: {package['name']} {package['version']} declares "
            f"license_file {declared}, which is missing; scanning the package root\n"
        )
    try:
        entries = os.listdir(package_root)
    except OSError as error:
        fail(f"cannot read {package['name']} {package['version']} source: {error}")
    names = sorted(
        (
            name
            for name in entries
            if name.upper().startswith(license_prefixes)
            and os.path.isfile(os.path.join(package_root, name))
        ),
        key=lambda name: (name.lower(), name),
    )
    return [(name, os.path.join(package_root, name)) for name in names]


def read_text(path):
    """Read one licence file as UTF-8 with LF endings and no trailing spaces."""
    with open(path, "rb") as handle:
        raw = handle.read()
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        text = raw.decode("utf-8", "replace")
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    # Trailing spaces are insignificant in the licence text but would make
    # `git diff --check` fail on the committed bundle.
    text = "\n".join(line.rstrip() for line in text.split("\n"))
    return text.rstrip("\n") + "\n"


metadata = cargo_metadata()
workspace = set(metadata["workspace_members"])

packages = []
texts = {}
for package in metadata["packages"]:
    if package["id"] in workspace:
        continue
    name = package["name"]
    version = package["version"]
    refs = []
    for filename, path in license_files(package):
        text = read_text(path)
        digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
        entry = texts.setdefault(
            digest,
            {"text": text, "packages": set(), "files": set()},
        )
        entry["packages"].add((name, version))
        entry["files"].add(filename)
        refs.append((filename, digest))
    packages.append(
        {
            "id": package["id"],
            "name": name,
            "version": version,
            "license": package.get("license") or "UNKNOWN",
            "repository": package.get("repository"),
            "refs": refs,
        }
    )

packages.sort(
    key=lambda package: (
        package["name"],
        version_key(package["version"]),
        package["id"],
    )
)

# Text sections follow the sorted package order so the output is stable even
# if cargo prints the graph in a different order.
ordered_texts = []
for package in packages:
    for _, digest in package["refs"]:
        if digest not in ordered_texts:
            ordered_texts.append(digest)

with_file = sum(1 for package in packages if package["refs"])
without_file = len(packages) - with_file
lines = [
    "Third-party notices for Vector Field Explorer",
    "=============================================",
    "",
    "Generated by scripts/generate-notices.sh from the dependency graph resolved",
    "by `cargo metadata --format-version 1 --locked`; do not edit this file by",
    "hand. The hand-written summary is THIRD_PARTY_LICENSES.md beside this file.",
    "",
    f"{len(packages)} non-workspace packages: {with_file} ship a licence file and",
    f"{without_file} do not. Licence texts are deduplicated by SHA-256 content hash;",
    "each package names the text section(s) it uses. A package that ships no",
    "licence file keeps its SPDX expression and a NO LICENCE FILE marker so a",
    "maintainer can review it; no copyright text is invented here.",
    "",
    "## Packages",
    "",
]

separator = "-" * 78

for package in packages:
    lines.append(f"### {package['name']} {package['version']}")
    lines.append(f"Licence: {package['license']}")
    if package["repository"]:
        lines.append(f"Repository: {package['repository']}")
    if package["refs"]:
        rendered = ", ".join(
            f"{filename} (text {digest[:16]})"
            for filename, digest in sorted(
                package["refs"], key=lambda ref: (ref[0].lower(), ref[0])
            )
        )
        lines.append(f"Licence files: {rendered}")
    else:
        lines.append("Licence files: none")
        lines.append(
            "NO LICENCE FILE: the package source ships no LICENSE*/COPYING*/NOTICE* "
            "file at its root; use the SPDX expression above and review the "
            "upstream package for the text."
        )
    lines.append("")

lines.append("## Licence texts")
lines.append("")

for digest in ordered_texts:
    entry = texts[digest]
    used = ", ".join(
        f"{name} {version}"
        for name, version in sorted(
            entry["packages"],
            key=lambda item: (item[0], version_key(item[1]), item[1]),
        )
    )
    filenames = ", ".join(
        sorted(entry["files"], key=lambda name: (name.lower(), name))
    )
    lines.append(f"### Text {digest[:16]}")
    lines.append(f"Used by {len(entry['packages'])} packages: {used}")
    lines.append(f"File names: {filenames}")
    lines.append(separator)
    lines.append(entry["text"].rstrip("\n"))
    lines.append(separator)
    lines.append("")

rendered = "\n".join(lines).rstrip("\n") + "\n"
with open(target, "w", encoding="utf-8", newline="\n") as handle:
    handle.write(rendered)
PY

if [ "$check" -eq 1 ]; then
  if ! cmp -s "$output" "$tmp"; then
    diff -u "$output" "$tmp" | head -n 80 >&2 || true
    echo "generate-notices: $output is stale; run scripts/generate-notices.sh and commit the result" >&2
    exit 1
  fi
  echo "generate-notices: $output is current"
else
  chmod 0644 "$tmp"
  mv "$tmp" "$output"
  echo "generate-notices: wrote $output"
fi
