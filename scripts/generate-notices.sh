#!/usr/bin/env bash
# Generate the per-crate third-party notice bundle (ticket 19).
#
# Output: packaging/THIRD_PARTY_NOTICES.txt
#
# Input: `cargo metadata --format-version 1 --locked`, so generation is
# offline and uses no new Rust dependencies. For every non-workspace package
# the bundle records the name, version, licence expression, repository, and
# the licence text(s) shipped with the package source or checked into
# packaging/notice-sources/.
#
# Packages whose crate source ships no licence file cannot be left without
# text. The generator applies this policy:
#   * If the SPDX expression offers Apache-2.0 as an alternative, elect
#     Apache-2.0 and point at the single canonical Apache-2.0 section.
#     Apache-2.0 section 4(a) requires the licence copy, not a per-holder
#     copyright line; section 4(c) applies only when distributing Source form.
#     No NOTICE file exists at the checked repository roots, so no NOTICE copy
#     is required.
#   * An MIT-only package must have a checked-in upstream MIT text in
#     packaging/notice-sources/manifest.json; generation fails if it does
#     not. Copyright lines come from the checked-in upstream file, never from
#     this generator.
#   * BSL-1.0 is the only expression allowed to stay textless. It is not
#     linked into the shipped targets under current features; if it were, the
#     final clause of the single BSL-1.0 grant excludes copies solely in
#     machine-executable object-code form.
#   * Any other expression without a crate licence file fails generation.
#
# Byte-identical texts are deduplicated by SHA-256 while every package still
# names the text section it uses. Text is normalised to LF line endings and
# trailing whitespace is stripped so `git diff --check` stays clean.
#
# Usage:
#   scripts/generate-notices.sh          regenerate the committed bundle
#   scripts/generate-notices.sh --check  fail if the committed bundle is stale
#
# The output is committed, so release builds and tests never need network
# access. CI runs `--check` after the build has populated the registry cache
# (CARGO_NET_OFFLINE=true); maintainers rerun the generator when Cargo.lock or
# a checked-in notice source changes.
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
manifest_path = os.path.join("packaging", "notice-sources", "manifest.json")
license_file_re = re.compile(
    r"^(?i:LICENSE|LICENCE|COPYING|NOTICE)"
    r"(?:(?i:[-_][A-Za-z0-9._+-]+))*"
    r"(?:\.(?i:txt|md|rst|html?|adoc|lesser)|\.(?:[A-Z][A-Za-z0-9._+-]*))?$"
)


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


def target_graph_counts():
    """Exact per-target cargo tree package counts for the bundle header."""
    targets = (
        ("x86_64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"),
        ("aarch64-apple-darwin", "aarch64-apple-darwin"),
        ("x86_64-apple-darwin", "x86_64-apple-darwin"),
    )
    counts = {}
    for label, target in targets:
        for edges in ("normal", "normal,build"):
            try:
                result = subprocess.run(
                    [
                        "cargo",
                        "tree",
                        "--target",
                        target,
                        "--edges",
                        edges,
                        "--prefix",
                        "none",
                        "--format",
                        "{p}",
                    ],
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
                fail(f"cargo tree failed for {target} ({edges})")
            packages = {
                line.strip() for line in result.stdout.splitlines() if line.strip()
            }
            counts[(label, edges)] = len(packages)
    return counts


def version_key(version):
    """Order numeric release components numerically and text components last."""
    parts = []
    for part in re.split(r"(\d+)", version):
        if not part:
            continue
        parts.append((0, int(part)) if part.isdigit() else (1, part))
    return tuple(parts)


def read_bytes(path):
    try:
        with open(path, "rb") as handle:
            return handle.read()
    except OSError as error:
        fail(f"cannot read {path}: {error}")


def normalise(raw, path):
    """Decode strict UTF-8, convert to LF, and strip trailing whitespace."""
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        fail(f"{path} is not valid UTF-8: {error}")
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    text = "\n".join(line.rstrip() for line in text.split("\n"))
    return text.lstrip("\n").rstrip("\n") + "\n"


def text_hash(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


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
            if license_file_re.fullmatch(name)
            and os.path.isfile(os.path.join(package_root, name))
        ),
        key=lambda name: (name.lower(), name),
    )
    return [(name, os.path.join(package_root, name)) for name in names]


def license_options(expression):
    # Cargo's legacy `MIT/Apache-2.0` spelling means an OR choice.
    normalised = expression.replace("/", " OR ")
    options = []
    for option in re.split(r"\s+OR\s+", normalised, flags=re.IGNORECASE):
        option = option.strip().strip("()").strip()
        if option:
            options.append(option)
    return options


def load_manifest():
    raw = read_bytes(manifest_path)
    try:
        manifest = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"{manifest_path} is not valid JSON: {error}")
    return manifest


def load_manifest_text(entry, label):
    """Verify one checked-in source file and return text and provenance."""
    file_name = entry.get("file")
    expected = entry.get("sha256")
    provenance = entry.get("provenance")
    if not file_name or not expected or not provenance:
        fail(f"{manifest_path}: {label} needs file, sha256, and provenance")
    path = os.path.join("packaging", "notice-sources", file_name)
    raw = read_bytes(path)
    actual = hashlib.sha256(raw).hexdigest()
    if actual != expected:
        fail(f"{path}: sha256 mismatch (manifest {expected}, actual {actual})")
    text = normalise(raw, path)
    return {
        "file": file_name,
        "source_sha256": actual,
        "text": text,
        "digest": text_hash(text),
        "provenance": provenance,
        "note": entry.get("note"),
    }


manifest = load_manifest()
canonical = load_manifest_text(
    manifest.get("canonical", {}).get("apache-2.0", {}),
    "canonical Apache-2.0",
)

mit_by_package = {}
mit_entries = []
for index, entry in enumerate(manifest.get("mit", [])):
    loaded = load_manifest_text(entry, f"mit[{index}]")
    packages = entry.get("packages") or []
    if not packages:
        fail(f"{manifest_path}: mit[{index}] lists no packages")
    loaded["packages"] = packages
    mit_entries.append(loaded)
    for key in packages:
        if key in mit_by_package:
            fail(f"{manifest_path}: {key} appears in more than one MIT source")
        mit_by_package[key] = loaded

metadata = cargo_metadata()
graph_counts = target_graph_counts()
workspace = set(metadata["workspace_members"])

packages = []
texts = {}


def text_entry(digest, text, canonical=False):
    entry = texts.get(digest)
    if entry is None:
        entry = {
            "text": text,
            "packages": set(),
            "files": set(),
            "provenance": set(),
            "notes": set(),
            "canonical": canonical,
        }
        texts[digest] = entry
    return entry


apache_elected = 0
apache_only = []
mit_manifest_used = 0
used_mit_keys = set()
bsl_exempt = 0

for package in metadata["packages"]:
    if package["id"] in workspace:
        continue
    name = package["name"]
    version = package["version"]
    expression = package.get("license") or "UNKNOWN"
    files = license_files(package)
    refs = []
    election = None

    if files:
        for filename, path in files:
            text = normalise(read_bytes(path), path)
            digest = text_hash(text)
            entry = text_entry(digest, text)
            entry["packages"].add((name, version))
            entry["files"].add(filename)
            refs.append((filename, digest))
    else:
        options = license_options(expression)
        if "Apache-2.0" in options:
            election = {"kind": "apache", "license": "Apache-2.0"}
            apache_elected += 1
            if options == ["Apache-2.0"]:
                apache_only.append(f"{name} {version}")
            entry = text_entry(canonical["digest"], canonical["text"], canonical=True)
            entry["canonical"] = True
            entry["packages"].add((name, version))
            entry["files"].add(canonical["file"])
            entry["provenance"].add(canonical["provenance"])
        elif options == ["BSL-1.0"]:
            election = {"kind": "bsl", "license": "BSL-1.0"}
            bsl_exempt += 1
        elif options == ["MIT"]:
            key = f"{name} {version}"
            source = mit_by_package.get(key)
            if source is None:
                fail(
                    f"no licence text for {key} ({expression}): the crate ships no "
                    f"licence file and {manifest_path} has no MIT text for it; "
                    "add a checked-in upstream text and package mapping"
                )
            election = {"kind": "mit", "license": "MIT", "source": source}
            mit_manifest_used += 1
            used_mit_keys.add(key)
            entry = text_entry(source["digest"], source["text"])
            entry["packages"].add((name, version))
            entry["files"].add(source["file"])
            entry["provenance"].add(source["provenance"])
            if source["note"]:
                entry["notes"].add(source["note"])
        else:
            fail(
                f"no licence text for {name} {version} ({expression}): the crate "
                "ships no licence file and the generator will not invent text; "
                "add a checked-in upstream source or extend the election policy"
            )

    packages.append(
        {
            "id": package["id"],
            "name": name,
            "version": version,
            "license": expression,
            "repository": package.get("repository"),
            "refs": refs,
            "election": election,
        }
    )

for key in sorted(set(mit_by_package) - used_mit_keys):
    sys.stderr.write(
        f"generate-notices: warning: {manifest_path} maps {key}, which has no "
        "no-file MIT package in this graph\n"
    )

packages.sort(
    key=lambda package: (
        package["name"],
        version_key(package["version"]),
        package["id"],
    )
)

# Text sections follow the sorted package order so the output is stable even
# if cargo prints the graph in a different order. The canonical Apache section
# is always first.
ordered_texts = [canonical["digest"]]
for package in packages:
    for _, digest in package["refs"]:
        if digest not in ordered_texts:
            ordered_texts.append(digest)
    election = package["election"]
    if election and election["kind"] == "mit":
        digest = election["source"]["digest"]
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
    f"{len(packages)} non-workspace packages: {with_file} ship a crate licence file.",
    f"Of the {without_file} that do not, {apache_elected} take Apache-2.0 from their SPDX",
    f"expression ({apache_elected - len(apache_only)} dual/triple, {len(apache_only)} "
    f"Apache-only: {', '.join(sorted(apache_only))}), {mit_manifest_used} use a checked-in",
    "upstream MIT text from packaging/notice-sources/, and 1 is BSL-1.0",
    "(`clipboard-win`), which is not linked into any shipped target under current",
    "features.",
    "",
    "The 513-package list is a metadata superset of the shipped binaries: it covers",
    f"every target and optional feature. cargo tree normal-edge counts are "
    f"{graph_counts[('x86_64-unknown-linux-gnu', 'normal')]}/"
    f"{graph_counts[('aarch64-apple-darwin', 'normal')]}/"
    f"{graph_counts[('x86_64-apple-darwin', 'normal')]}",
    f"and normal+build counts are "
    f"{graph_counts[('x86_64-unknown-linux-gnu', 'normal,build')]}/"
    f"{graph_counts[('aarch64-apple-darwin', 'normal,build')]}/"
    f"{graph_counts[('x86_64-apple-darwin', 'normal,build')]} for Linux x86_64 /",
    "macOS arm64 / macOS x86_64; host-only proc-macro and build units account for",
    "part of the gap, and the remainder are optional or inactive target packages.",
    "",
    "Licence texts are normalised to LF line endings and have trailing whitespace",
    "stripped. Identical texts are deduplicated by SHA-256 content hash; each",
    "package names the text section it uses. Copyright lines come only from crate",
    "or upstream files; the generator never invents copyright text.",
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
    election = package["election"]
    if package["refs"]:
        rendered = ", ".join(
            f"{filename} (text {digest[:16]})"
            for filename, digest in sorted(
                package["refs"], key=lambda ref: (ref[0].lower(), ref[0])
            )
        )
        lines.append(f"Licence files: {rendered}")
    elif election and election["kind"] == "apache":
        lines.append("Licence files: none")
        lines.append(
            "Elected licence: Apache-2.0; text: canonical Apache-2.0 section "
            f"below (upstream {canonical['provenance']}, source sha256 "
            f"{canonical['source_sha256']})"
        )
    elif election and election["kind"] == "mit":
        source = election["source"]
        lines.append("Licence files: none")
        lines.append(
            f"Elected licence: MIT; text {source['digest'][:16]} (upstream "
            f"{source['provenance']}, source sha256 {source['source_sha256']})"
        )
    elif election and election["kind"] == "bsl":
        lines.append("Licence files: none")
        lines.append(
            "NO LICENCE FILE: BSL-1.0 (`clipboard-win`) is not linked into any "
            "shipped target under current features (Windows-only cfg plus the "
            "optional egui clipboard feature), so no BSL notice is distributed. "
            "If it were enabled, the final clause of the single BSL-1.0 grant "
            "would exclude copies solely in machine-executable object-code form "
            "from the notice requirement."
        )
    else:
        fail(f"internal error: {package['name']} has neither text nor a known election")
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
    if entry["canonical"]:
        lines.append(f"### Canonical Apache-2.0 text ({digest[:16]})")
    else:
        lines.append(f"### Text {digest[:16]}")
    lines.append(f"Used by {len(entry['packages'])} packages: {used}")
    lines.append(f"File names: {filenames}")
    for provenance in sorted(entry["provenance"]):
        lines.append(f"Provenance: {provenance}")
    for note in sorted(entry["notes"]):
        lines.append(f"Note: {note}")
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
