#!/usr/bin/env python3
"""Documentation-drift checker for Vector Field Explorer.

Standard library only. Run from anywhere inside (or outside) the repo; paths
are resolved relative to this script's location, not the current working
directory.

Usage: python3 scripts/check-docs.py
Exit 0 (prints "docs OK") when clean; non-zero with a grouped violation
list otherwise.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent

CODEBASE = ROOT / "docs" / "CODEBASE.md"
WIKI_DIR = ROOT / "docs" / "wiki"
INDEX_MD = WIKI_DIR / "index.md"

# Inline code spans only (no newlines), so fenced code blocks and stray
# multi-line spans cannot create phantom paths.
INLINE_CODE_RE = re.compile(r"`([^`\n]+)`")
MD_LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
# A backticked token that is exactly a repo-relative source/test path.
PATH_RE = re.compile(r"^(src|tests)/[A-Za-z0-9_./-]+\.rs$")


def _rel(p: Path) -> str:
    return p.relative_to(ROOT).as_posix()


def src_files() -> list[str]:
    base = ROOT / "src"
    out = []
    if base.is_dir():
        for p in base.rglob("*.rs"):
            out.append(_rel(p))
    return sorted(out)


def check_source_coverage() -> tuple[str, list[str]]:
    title = "Source coverage: src/ files missing from docs/CODEBASE.md"
    content = CODEBASE.read_text(encoding="utf-8") if CODEBASE.is_file() else ""
    violations = [rel for rel in src_files() if rel not in content]
    return title, sorted(violations)


def check_phantom_paths() -> tuple[str, list[str]]:
    title = "No phantom paths: backticked src/ or tests/ paths that do not exist"
    if not CODEBASE.is_file():
        return title, []
    text = CODEBASE.read_text(encoding="utf-8")
    tokens = INLINE_CODE_RE.findall(text)
    candidates = {t for t in tokens if PATH_RE.match(t)}
    violations = sorted(p for p in candidates if not (ROOT / p).is_file())
    return title, violations


def _wiki_md_files() -> list[Path]:
    out = []
    if WIKI_DIR.is_dir():
        for p in WIKI_DIR.rglob("*.md"):
            out.append(p)
    return sorted(out)


def check_wiki_index_coverage() -> tuple[str, list[str]]:
    title = "Wiki index coverage: docs/wiki pages not linked from index.md"
    if not INDEX_MD.is_file():
        return title, [_rel(p) for p in _wiki_md_files() if p != INDEX_MD]
    text = INDEX_MD.read_text(encoding="utf-8")
    linked: set[str] = set()
    for target in MD_LINK_RE.findall(text):
        target = target.strip()
        if not target:
            continue
        # Strip fragment / query.
        clean = target.split("#", 1)[0].split("?", 1)[0].strip()
        if not clean or "://" in clean or clean.startswith("mailto:"):
            continue
        if clean.startswith("/"):
            cand = ROOT / clean.lstrip("/")
        else:
            cand = (INDEX_MD.parent / clean).resolve()
        try:
            cand = cand.resolve()
        except OSError:
            continue
        if cand.is_file():
            try:
                linked.add(cand.relative_to(ROOT).as_posix())
            except ValueError:
                pass
    violations = []
    for p in _wiki_md_files():
        if p == INDEX_MD:
            continue
        rel = _rel(p)
        if rel not in linked:
            violations.append(rel)
    return title, sorted(violations)


def _is_relative_link(target: str) -> bool:
    clean = target.strip()
    if not clean or clean.startswith("#"):
        return False
    clean = clean.split("#", 1)[0].split("?", 1)[0].strip()
    if not clean or clean.startswith("#"):
        return False
    # Anchors, absolute URLs, mailto links and absolute paths are not local
    # files.
    if "://" in clean or clean.startswith("mailto:") or clean.startswith("/"):
        return False
    return True


def check_wiki_links() -> tuple[str, list[str]]:
    title = "Wiki links: relative links that do not resolve"
    violations: list[str] = []
    for p in _wiki_md_files():
        text = p.read_text(encoding="utf-8")
        for target in MD_LINK_RE.findall(text):
            target = target.strip()
            if not _is_relative_link(target):
                continue
            clean = target.split("#", 1)[0].split("?", 1)[0].strip()
            cand = (p.parent / clean).resolve()
            if not cand.is_file():
                violations.append(f"{_rel(p)}: {target}")
    return title, sorted(set(violations))


def main() -> int:
    checks = [
        check_source_coverage(),
        check_phantom_paths(),
        check_wiki_index_coverage(),
        check_wiki_links(),
    ]

    all_violations = []
    for title, items in checks:
        if items:
            all_violations.append((title, items))

    if not all_violations:
        print("docs OK")
        return 0

    total = sum(len(items) for _, items in all_violations)
    print(f"docs check: {total} violation(s)")
    print()
    for idx, (title, items) in enumerate(all_violations, start=1):
        print(f"[{idx}] {title}")
        for item in items:
            print(f"  - {item}")
        print()
    return 1


if __name__ == "__main__":
    sys.exit(main())
