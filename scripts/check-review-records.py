#!/usr/bin/env python3
"""Require review records for ticket commits after the enforcement baseline."""
from __future__ import annotations
import re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Commits at/before this rule-introduction commit are grandfathered.
BASELINE_COMMIT = "6b52ba8149ce4d4d1b71abc18c2c665cd392388e"
TICKET_RE = re.compile(r"\(ticket (\d+)\)")

def main() -> int:
    try:
        commits = subprocess.run(["git", "-C", str(ROOT), "rev-list", "--reverse", f"{BASELINE_COMMIT}..HEAD"], check=True, capture_output=True, text=True).stdout.splitlines()
    except subprocess.CalledProcessError as exc:
        print(f"review records check: cannot read history: {exc}", file=sys.stderr)
        return 2
    records = sorted((ROOT / "docs/reviews").glob("*.md"))
    contents = [p.read_text(encoding="utf-8") for p in records]
    violations = []
    for commit in commits:
        subject = subprocess.run(["git", "-C", str(ROOT), "show", "-s", "--format=%s", commit], check=True, capture_output=True, text=True).stdout.strip()
        match = TICKET_RE.search(subject)
        if match:
            ticket = match.group(1)
            if not any(re.search(rf"^\*\*Ticket:\*\*\s*{ticket}\s*$", text, re.M) and commit in text for text in contents):
                violations.append(f"{commit[:12]} {subject} (missing docs/reviews record naming commit)")
    if violations:
        print("review records check: violation(s)\n" + "\n".join(f"  - {v}" for v in violations))
        return 1
    print("review records OK")
    return 0

if __name__ == "__main__":
    sys.exit(main())
