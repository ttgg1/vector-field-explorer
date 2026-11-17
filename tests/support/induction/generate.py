#!/usr/bin/env python3
"""Regenerate the committed induction vector-potential fixtures.

Generation-time dependency: mpmath (not a Rust or runtime dependency).

The loop and finite-solenoid potentials are evaluated directly from the
softened axisymmetric integrals in the approved design, independently of the
Rust quadrature:

    A_phi = I R integral_0^{2pi} cos(phi) / D dphi              (loop)
    A_phi = (N I / L) R integral_0^{2pi} cos(phi) *
            [asinh((z + L/2)/D) - asinh((z - L/2)/D)] dphi     (solenoid)
    D = sqrt(r^2 + R^2 - 2 r R cos(phi) + z^2 + a^2)

Every reference is computed twice, at 40 and 70 decimal digits, with true
mpmath.pi and mpmath.quad. A row whose two results differ by more than
`1e-12 + 1e-10 * |reference|` is rejected rather than committed.

Usage:
    python3 tests/support/induction/generate.py
"""

from __future__ import annotations

import csv
from pathlib import Path

import mpmath as mp

ROOT = Path(__file__).resolve().parent

LOOP_RADIUS = mp.mpf("1.0")
SOLENOID_RADIUS = mp.mpf("0.5")
SOLENOID_LENGTH = mp.mpf("2.0")
SOLENOID_TURNS = 8

# (r, z, softening)
LOOP_POINTS = [
    ("0.0", "0.7", "0.1"),       # on axis
    ("0.3", "0.2", "0.1"),       # interior, off axis
    ("0.999", "0.05", "0.05"),   # near the conductor
    ("1.8", "0.4", "0.1"),       # exterior
    ("0.4", "-0.8", "0.1"),      # below the loop
    ("0.5", "0.1", "0.3"),       # large softening
]
SOLENOID_POINTS = [
    ("0.0", "0.0", "0.1"),       # on axis, mid-length
    ("0.2", "0.4", "0.1"),       # interior
    ("0.9", "0.3", "0.1"),       # exterior radial
    ("0.49", "0.1", "0.05"),     # near the winding
    ("0.3", "0.99", "0.1"),      # near the axial end
    ("0.4", "1.6", "0.1"),       # beyond the end
]


def loop_potential(r: mp.mpf, z: mp.mpf, a: mp.mpf) -> mp.mpf:
    def kernel(phi: mp.mpf) -> mp.mpf:
        d = mp.sqrt(r * r + LOOP_RADIUS * LOOP_RADIUS - 2 * r * LOOP_RADIUS * mp.cos(phi) + z * z + a * a)
        return mp.cos(phi) / d

    return LOOP_RADIUS * mp.quad(kernel, [0, mp.pi, 2 * mp.pi])


def solenoid_potential(r: mp.mpf, z: mp.mpf, a: mp.mpf) -> mp.mpf:
    half = SOLENOID_LENGTH / 2

    def kernel(phi: mp.mpf) -> mp.mpf:
        d = mp.sqrt(r * r + SOLENOID_RADIUS * SOLENOID_RADIUS - 2 * r * SOLENOID_RADIUS * mp.cos(phi) + a * a)
        return mp.cos(phi) * (mp.asinh((z + half) / d) - mp.asinh((z - half) / d))

    return (SOLENOID_TURNS / SOLENOID_LENGTH) * SOLENOID_RADIUS * mp.quad(
        kernel, [0, mp.pi, 2 * mp.pi]
    )


def evaluate(case: str, r: str, z: str, a: str) -> mp.mpf:
    function = loop_potential if case == "loop" else solenoid_potential
    results = []
    for digits in (40, 70):
        with mp.workdps(digits):
            results.append(function(mp.mpf(r), mp.mpf(z), mp.mpf(a)))
    low, high = results
    tolerance = mp.mpf("1e-12") + mp.mpf("1e-10") * abs(high)
    if abs(high - low) > tolerance:
        raise SystemExit(
            f"{case} r={r} z={z} a={a} did not converge: {low} vs {high}"
        )
    return high


def write(name: str, points: list[tuple[str, str, str]]) -> None:
    path = ROOT / f"{name}.csv"
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(["case", "r", "z", "softening", "reference"])
        for r, z, a in points:
            reference = evaluate(name, r, z, a)
            writer.writerow([name, r, z, a, mp.nstr(reference, 25)])
    print(f"wrote {path.relative_to(ROOT.parent.parent.parent)}")


def main() -> None:
    write("loop", LOOP_POINTS)
    write("solenoid", SOLENOID_POINTS)


if __name__ == "__main__":
    main()
