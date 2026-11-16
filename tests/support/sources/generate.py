#!/usr/bin/env python3
"""Offline generator for the committed electric source fixtures.

This script is provenance, not a test or runtime dependency. It evaluates every
reference independently of the Rust production code at two mpmath decimal
precisions, checks the change against the committed uncertainty contract
``1e-12 + 1e-10 * |reference|`` and writes ``fixtures.csv`` with exact binary64
input bits plus correctly rounded binary64 references.

Methods:

* point    -- closed form ``Q s / (|s|^2 + a^2)^(3/2)``.
* line     -- the design's analytic endpoint primitive with rationalized
              same-sign differences.
* ring     -- complete elliptic integrals ``K``/``E`` for
              ``S0 = <(1 - delta cos phi)^(-3/2)>`` and
              ``S1 = <cos phi (1 - delta cos phi)^(-3/2)>``.
* disc     -- polar ring integration: a disc is a family of charged rings of
              radius ``rho`` with measure ``Q/R^2 dt``, ``t = rho^2``.
* sphere   -- slab integration: a solid sphere is a family of discs of radius
              ``sqrt(R^2 - zeta^2)`` with the uniform volume measure.
* loop     -- direct current-element angular integration plus an independent
              complete-elliptic off-axis/reference cross-check.
* solenoid -- loop-over-z integration: each axial slice is a loop of current
              ``N I dz/L`` using the elliptic loop field, split at the projected
              query location.
* wire     -- direct infinite-wire integral after ``z' = sqrt(r^2+a^2) tan``.

The ring/disc/sphere reductions use only the specified physical distributions;
none of them follows the production chord, surface-divergence or near-axis
series decompositions. The loop/solenoid references never call the production
angle quadrature or analytic axial integration.

True ``mp.pi`` is used throughout. Run from the repository root:

    python3 tests/support/sources/generate.py
"""

from __future__ import annotations

import struct
from pathlib import Path

import mpmath as mp

HERE = Path(__file__).resolve().parent
OUT = HERE / "fixtures.csv"
LOW_DPS = 40
HIGH_DPS = 70


def bits(value: float) -> str:
    """Exact binary64 bit pattern for a Python float, as 16 hex digits."""
    return "0x" + struct.pack(">d", float(value)).hex()


def point_components(r: float, z: float, charge: float, softening: float):
    s = mp.sqrt(r * r + z * z)
    if s == 0:
        return mp.mpf(0), mp.mpf(0)
    d = (s * s + softening * softening) ** (mp.mpf(3) / 2)
    return charge * r / d, charge * z / d


def line_components(r: float, z: float, length: float, charge: float, softening: float):
    c = r * r + softening * softening
    u = z + length / 2
    v = z - length / 2
    a = mp.sqrt(c + v * v)
    b = mp.sqrt(c + u * u)
    same_sign = (u > 0 and v > 0) or (u < 0 and v < 0)
    if same_sign:
        ez = charge / length * (2 * z * length / (a * b * (a + b)))
        er = (
            mp.mpf(0)
            if r == 0
            else charge * r * 2 * z / (a * b * (u * a + v * b))
        )
    else:
        ez = charge / length * (1 / a - 1 / b)
        er = mp.mpf(0) if r == 0 else charge * r / (length * c) * (u / b - v / a)
    return er, ez


def ring_components(r, z, radius, charge, softening):
    """Exact softened charged-ring field from complete elliptic integrals."""
    a = r * r + radius * radius + z * z + softening * softening
    delta = 2 * r * radius / a
    if delta == 0:
        s0 = mp.mpf(1)
        s1 = mp.mpf(0)
    else:
        k2 = 2 * delta / (1 + delta)
        elliptic_k = mp.ellipk(k2)
        elliptic_e = mp.ellipe(k2)
        root = mp.sqrt(1 + delta)
        s0 = 2 * elliptic_e / (mp.pi * root * (1 - delta))
        s1 = 2 / (mp.pi * delta * root) * (elliptic_e / (1 - delta) - elliptic_k)
    base = charge / a ** (mp.mpf(3) / 2)
    return base * (r * s0 - radius * s1), base * z * s0


def disc_components(r, z, radius, charge, softening):
    """Polar ring integration of a thin uniform disc."""
    if radius == 0:
        return point_components(r, z, charge, softening)
    scale = charge / (radius * radius)

    def integrand(t):
        rho = mp.sqrt(t)
        er, ez = ring_components(r, z, rho, mp.mpf(1), softening)
        return er * scale, ez * scale

    hi = radius * radius
    parts = [(mp.mpf(0), hi)]
    if 0 < r < radius:
        parts = [(mp.mpf(0), r * r), (r * r, hi)]
    er = sum(mp.quad(lambda t: integrand(t)[0], list(part)) for part in parts)
    ez = sum(mp.quad(lambda t: integrand(t)[1], list(part)) for part in parts)
    return er, ez


def sphere_components(r, z, radius, charge, softening):
    """Spherical-shell volume integration of a uniform solid sphere.

    This integrates the exact physical volume measure directly; it does not use
    the production divergence-theorem surface reduction. For one shell of radius
    ``rp`` the polar integral is evaluated analytically, leaving one radial
    integral.
    """
    s = mp.sqrt(r * r + z * z)
    if s == 0:
        return mp.mpf(0), mp.mpf(0)
    density = 3 * charge / (4 * mp.pi * radius**3)

    def shell(rp):
        if rp == 0:
            return mp.mpf(0)
        a = s * s + rp * rp + softening * softening
        b = 2 * s * rp
        d_plus = mp.sqrt(a + b)
        d_minus = mp.sqrt(a - b)
        polar = (2 / b) * (
            (rp / b) * (d_plus - d_minus)
            - (s - rp * a / b) * (1 / d_plus - 1 / d_minus)
        )
        return rp * rp * polar

    radial = density * 2 * mp.pi * sum(
        mp.quad(shell, [left, right])
        for left, right in _splits(0.0, radius, s)
    )
    return radial * r / s, radial * z / s


def _splits(lo, hi, at):
    """Intervals split at an interior projected support location."""
    cuts = [mp.mpf(lo), mp.mpf(hi)]
    if lo < at < hi:
        cuts.insert(1, mp.mpf(at))
    return [(cuts[i], cuts[i + 1]) for i in range(len(cuts) - 1)]


def loop_components_direct(r, z, radius, current, softening):
    """Direct current-element integration over the loop angle.

    ``B = I R integral e_phi x s / (|s|^2 + a^2)^(3/2) dphi`` in the local
    ``(radial, axial)`` plane. This does not use the ``A``/``delta`` series
    reduction.
    """
    a = r * r + radius * radius + z * z + softening * softening

    def radial_integrand(phi):
        cos = mp.cos(phi)
        denominator = a - 2 * r * radius * cos
        return current * radius * z * cos / denominator ** (mp.mpf(3) / 2)

    def axial_integrand(phi):
        cos = mp.cos(phi)
        denominator = a - 2 * r * radius * cos
        return current * radius * (radius - r * cos) / denominator ** (
            mp.mpf(3) / 2
        )

    br = mp.quad(radial_integrand, [-mp.pi, 0, mp.pi])
    bz = mp.quad(axial_integrand, [-mp.pi, 0, mp.pi])
    return br, bz


def loop_components_elliptic(r, z, radius, current, softening):
    """Exact softened loop field from complete elliptic ``K``/``E`` moments."""
    a = r * r + radius * radius + z * z + softening * softening
    delta = 2 * r * radius / a
    if delta == 0:
        s0 = mp.mpf(1)
        s1 = mp.mpf(0)
    else:
        k2 = 2 * delta / (1 + delta)
        elliptic_k = mp.ellipk(k2)
        elliptic_e = mp.ellipe(k2)
        root = mp.sqrt(1 + delta)
        s0 = 2 * elliptic_e / (mp.pi * root * (1 - delta))
        s1 = 2 / (mp.pi * delta * root) * (elliptic_e / (1 - delta) - elliptic_k)
    base = 2 * mp.pi * current * radius / a ** (mp.mpf(3) / 2)
    return base * z * s1, base * (radius * s0 - r * s1)


def solenoid_components(r, z, radius, length, current, turns, softening):
    """Loop-over-z integration of the continuous solenoid."""
    factor = turns * current / length

    def slice_components(slice_axial):
        return loop_components_elliptic(
            r, z - slice_axial, radius, mp.mpf(1), softening
        )

    br = sum(
        mp.quad(lambda zp: slice_components(zp)[0], [left, right])
        for left, right in _splits(-length / 2, length / 2, z)
    )
    bz = sum(
        mp.quad(lambda zp: slice_components(zp)[1], [left, right])
        for left, right in _splits(-length / 2, length / 2, z)
    )
    return factor * br, factor * bz


def wire_components(r, current, softening):
    """Direct infinite-wire integral ``I r integral dz'/(r^2+z'^2+a^2)^(3/2)``."""
    if r == 0:
        return mp.mpf(0)
    # z' = sqrt(r^2+a^2) tan(theta) removes the infinite range; the substitution
    # is evaluated symbolically for the measure and the cosine integral is
    # evaluated numerically, not from the known closed form.
    scale = mp.sqrt(r * r + softening * softening)
    integral = mp.quad(
        lambda theta: mp.cos(theta) / (scale * scale),
        [-mp.pi / 2, 0, mp.pi / 2],
    )
    return current * r * integral


def ring_radius_for_delta(delta: float, radius: float, z: float, softening: float) -> float:
    """Small off-axis radius with ``delta = 2 r R / (R^2 + z^2 + a^2 + r^2)``."""
    c = radius * radius + z * z + softening * softening
    root = mp.sqrt((radius / delta) ** 2 - c)
    return float(radius / delta - root)


# id, kind, center, axis, charge, dimension, query, softening
CASES = [
    ("point_basic", "point", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 0.0,
     (0.4, 0.0, 0.1), 0.1),
    ("point_negative_nonunit", "point", (0.3, -0.2, 0.5), (0.0, 0.0, 0.0),
     -1.5, 0.0, (0.9, 0.0, 0.7), 0.001),
    ("line_radial", "line", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 2.0,
     (0.4, 0.0, 0.0), 0.1),
    ("line_offaxis_end", "line", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), -2.0, 2.0,
     (0.3, 0.0, 0.9), 0.1),
    ("line_end", "line", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 2.0,
     (0.3, 0.0, 1.0), 0.1),
    ("line_narrow_far", "line", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 2.0,
     (0.5, 0.0, 0.25), 1e-6),
    ("ring_axis_0p7", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.0, 0.0, 0.7), 0.1),
    ("ring_offaxis", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.4, 0.0, 0.7), 0.1),
    ("ring_near_axis", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (1e-10, 0.0, 0.7), 0.1),
    ("ring_near_support", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (1.01, 0.0, 0.0), 0.1),
    ("ring_nonunit", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), -2.5, 0.3,
     (0.2, 0.0, 0.15), 0.001),
    ("ring_narrow_far", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.4, 0.0, 0.7), 1e-6),
    ("ring_delta_099", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.0, 0.0, 0.7), 0.1),
    ("ring_delta_100", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.0, 0.0, 0.7), 0.1),
    ("ring_delta_101", "ring", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.0, 0.0, 0.7), 0.1),
    ("disc_axis_0p7", "disc", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.0, 0.0, 0.7), 0.1),
    ("disc_offaxis_z0", "disc", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.4, 0.0, 0.0), 0.1),
    ("disc_offaxis_z0p7", "disc", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.4, 0.0, 0.7), 0.1),
    ("disc_near_support", "disc", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.97, 0.0, 0.02), 0.1),
    ("disc_nonunit", "disc", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), -2.0, 0.3,
     (0.2, 0.0, 0.1), 0.001),
    ("disc_narrow_far", "disc", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     (0.4, 0.0, 0.7), 1e-6),
    ("sphere_center", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 1.0, 1.0,
     (0.0, 0.0, 0.0), 0.1),
    ("sphere_near_center", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 1.0, 1.0,
     (1e-8, 0.0, 0.0), 0.1),
    ("sphere_axis", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 1.0, 1.0,
     (0.0, 0.0, 0.4), 0.1),
    ("sphere_outside", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 1.0, 1.0,
     (1.02, 0.0, 0.3), 0.1),
    ("sphere_interior", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 1.0, 1.0,
     (0.5, 0.0, 0.2), 0.1),
    ("sphere_near_surface", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 1.0, 1.0,
     (0.99, 0.0, 0.0), 0.1),
    # Near-surface with tiny softening: s = 1 + 1e-9, R = 1, a = 1e-6. The
    # softened distance is dominated by a, so the cancellation-prone A - B u
    # form loses the small (s - R) term; this row pins the stable reduction.
    ("sphere_near_surface_tiny", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0),
     1.0, 1.0, (1.0 + 1e-9, 0.0, 0.0), 1e-6),
    ("sphere_nonunit", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), -2.0, 0.3,
     (0.2, 0.0, 0.1), 0.001),
    ("sphere_narrow_far", "sphere", (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 1.0, 1.0,
     (2.0, 0.0, 0.0), 1e-6),
]

# Magnetic cases: (id, kind, center, axis, current, radius, length, turns,
# query, softening). Loop/solenoid fields use the local (radial, axial) frame;
# every committed row is centered at zero with axis +Z so the world components
# are (Br, 0, Bz).
MAGNETIC_CASES = [
    ("loop_axis_0p7", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0, 0.0,
     0, (0.0, 0.0, 0.7), 0.1),
    ("loop_offaxis", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0, 0.0,
     0, (0.4, 0.0, 0.7), 0.1),
    ("loop_offaxis_z0p2", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (0.4, 0.0, 0.2), 0.1),
    ("loop_near_axis_0p7", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (1e-10, 0.0, 0.7), 0.1),
    ("loop_near_support", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (1.01, 0.0, 0.02), 0.1),
    ("loop_on_winding", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (1.0, 0.0, 0.0), 0.1),
    ("loop_nonunit", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), -2.5, 0.3, 0.0,
     0, (0.2, 0.0, 0.15), 0.001),
    ("loop_narrow_far", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (0.4, 0.0, 0.7), 1e-6),
    ("loop_delta_099", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (0.0, 0.0, 0.7), 0.1),
    ("loop_delta_100", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (0.0, 0.0, 0.7), 0.1),
    ("loop_delta_101", "loop", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     0.0, 0, (0.0, 0.0, 0.7), 0.1),
    ("solenoid_axis_center", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0,
     1.0, 2.0, 1, (0.0, 0.0, 0.0), 0.1),
    ("solenoid_axis_end", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0,
     1.0, 2.0, 1, (0.0, 0.0, 1.0), 0.1),
    ("solenoid_axis_outside", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0),
     1.0, 1.0, 2.0, 1, (0.0, 0.0, 1.2), 0.1),
    ("solenoid_interior", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0,
     1.0, 2.0, 1, (0.5, 0.0, 0.2), 0.1),
    ("solenoid_on_winding", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0,
     1.0, 2.0, 1, (1.0, 0.0, 0.0), 0.1),
    ("solenoid_near_axis", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0,
     1.0, 2.0, 1, (1e-8, 0.0, 0.3), 0.1),
    ("solenoid_end_effect", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0,
     1.0, 2.0, 1, (0.6, 0.0, 0.9), 0.1),
    ("solenoid_nonunit", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), -2.0,
     0.3, 0.7, 3, (0.2, 0.0, 0.1), 0.001),
    ("solenoid_long", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, 1.0,
     20.0, 1, (0.7, 0.0, 0.3), 0.1),
    ("solenoid_narrow_far", "solenoid", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0,
     1.0, 2.0, 1, (0.4, 0.0, 3.0), 1e-6),
    ("wire_offaxis", "wire", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 2.0, 0.0, 0.0,
     0, (1.0, 0.0, 0.4), 0.1),
    ("wire_on_axis", "wire", (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), 2.0, 0.0, 0.0,
     0, (0.0, 0.0, 0.4), 0.1),
]


def evaluate(case, dps):
    with mp.workdps(dps):
        _, kind, center, _, charge, dimension, query, softening = case
        charge = mp.mpf(charge)
        softening = mp.mpf(softening)
        r = mp.mpf(query[0])
        z = mp.mpf(query[2])
        if kind == "point":
            dx = mp.mpf(query[0]) - mp.mpf(center[0])
            dy = mp.mpf(query[1]) - mp.mpf(center[1])
            dz = mp.mpf(query[2]) - mp.mpf(center[2])
            distance = mp.sqrt(dx * dx + dy * dy + dz * dz)
            if distance == 0:
                er, ez = mp.mpf(0), mp.mpf(0)
            else:
                denominator = (distance * distance + softening * softening) ** (
                    mp.mpf(3) / 2
                )
                return (charge * dx / denominator, charge * dy / denominator,
                        charge * dz / denominator)
        elif kind == "line":
            er, ez = line_components(
                r, z, mp.mpf(dimension), charge, softening
            )
        elif kind == "ring":
            er, ez = ring_components(r, z, mp.mpf(dimension), charge, softening)
        elif kind == "disc":
            er, ez = disc_components(r, z, mp.mpf(dimension), charge, softening)
        elif kind == "sphere":
            er, ez = sphere_components(r, z, mp.mpf(dimension), charge, softening)
        else:  # pragma: no cover - guarded by the table
            raise ValueError(kind)
        return er, mp.mpf(0), ez


def magnetic_components(case, dps, method=None):
    """World-frame loop/solenoid/wire reference components.

    Every magnetic row uses ``center = 0`` and ``axis = +Z``. A loop/solenoid
    returns ``(Br, 0, Bz)``; the infinite wire returns ``(0, Bphi, 0)``.
    """
    with mp.workdps(dps):
        case_id, kind, _, _, current, radius, length, turns, query, softening = case
        current = mp.mpf(current)
        softening = mp.mpf(softening)
        radius = mp.mpf(radius)
        length = mp.mpf(length)
        r = mp.mpf(query[0])
        z = mp.mpf(query[2])
        if kind == "loop":
            if method is None:
                method = (
                    "elliptic-k-e"
                    if case_id.startswith(("loop_near_axis", "loop_delta"))
                    else "current-element"
                )
            if method == "elliptic-k-e":
                br, bz = loop_components_elliptic(r, z, radius, current, softening)
            else:
                br, bz = loop_components_direct(r, z, radius, current, softening)
            return (br, mp.mpf(0), bz), method
        if kind == "solenoid":
            br, bz = solenoid_components(
                r, z, radius, length, current, turns, softening
            )
            return (br, mp.mpf(0), bz), "loop-over-z"
        if kind == "wire":
            return (mp.mpf(0), wire_components(r, current, softening), mp.mpf(0)), \
                "direct-wire-integral"
        raise ValueError(kind)


def _append_row(rows, case, low, high, method):
    case_id, kind, center, axis, charge, dimension, query, softening = case
    norm = mp.sqrt(sum(v * v for v in high))
    change = max(abs(a - b) for a, b in zip(low, high))
    allowed = mp.mpf("1e-12") + mp.mpf("1e-10") * norm
    if not (change <= allowed):
        raise SystemExit(
            f"{case_id}: precision change {mp.nstr(change, 8)} exceeds "
            f"{mp.nstr(allowed, 8)}"
        )
    uncertainty = max(change, mp.mpf("1e-300"))
    ref = [float(v) for v in high]
    rows.append(
        [
            case_id,
            kind,
            bits(center[0]), bits(center[1]), bits(center[2]),
            bits(axis[0]), bits(axis[1]), bits(axis[2]),
            bits(charge), bits(dimension),
            bits(query[0]), bits(query[1]), bits(query[2]),
            bits(softening),
            repr(ref[0]), repr(ref[1]), repr(ref[2]),
            repr(float(uncertainty)),
            bits(0.0), 0,
            method,
        ]
    )
    print(
        f"{case_id:26s} ref=({ref[0]: .17e},{ref[1]: .17e},{ref[2]: .17e}) "
        f"change={float(change):.3e}",
        flush=True,
    )


def main():
    rows = []
    for case in CASES:
        case_id, kind, center, axis, charge, dimension, query, softening = case
        if case_id.startswith("ring_delta_"):
            factor = {"ring_delta_099": 0.99, "ring_delta_100": 1.0,
                      "ring_delta_101": 1.01}[case_id]
            radius = ring_radius_for_delta(0.125 * factor, 1.0, 0.7, 0.1)
            query = (radius, 0.0, 0.7)
            case = (case_id, kind, center, axis, charge, dimension, query, softening)

        low = evaluate(case, LOW_DPS)
        high = evaluate(case, HIGH_DPS)
        _append_row(
            rows,
            case,
            low,
            high,
            {
                "point": "closed-form",
                "line": "endpoint-primitive",
                "ring": "elliptic-k-e",
                "disc": "polar-ring",
                "sphere": "shell-volume",
            }[case[1]],
        )

    for case in MAGNETIC_CASES:
        case_id, kind, center, axis, current, radius, length, turns, query, softening = case
        if case_id.startswith("loop_delta_"):
            factor = {"loop_delta_099": 0.99, "loop_delta_100": 1.0,
                      "loop_delta_101": 1.01}[case_id]
            query = (ring_radius_for_delta(0.125 * factor, 1.0, 0.7, 0.1), 0.0, 0.7)
            case = (case_id, kind, center, axis, current, radius, length, turns,
                    query, softening)

        low, method = magnetic_components(case, LOW_DPS)
        high, _ = magnetic_components(case, HIGH_DPS, method)
        if kind == "loop":
            direct, _ = magnetic_components(case, HIGH_DPS, "current-element")
            elliptic, _ = magnetic_components(case, HIGH_DPS, "elliptic-k-e")
            norm = mp.sqrt(sum(v * v for v in high))
            cross_change = max(abs(a - b) for a, b in zip(direct, elliptic))
            cross_allowed = mp.mpf("1e-12") + mp.mpf("1e-10") * norm
            if not (cross_change <= cross_allowed):
                raise SystemExit(
                    f"{case_id}: current-element/elliptic change "
                    f"{mp.nstr(cross_change, 8)} exceeds "
                    f"{mp.nstr(cross_allowed, 8)}"
                )
            method = f"{method}+elliptic" if method == "current-element" else method

        _, kind, center, axis, current, radius, length, turns, query, softening = case
        norm = mp.sqrt(sum(v * v for v in high))
        change = max(abs(a - b) for a, b in zip(low, high))
        allowed = mp.mpf("1e-12") + mp.mpf("1e-10") * norm
        if not (change <= allowed):
            raise SystemExit(
                f"{case_id}: precision change {mp.nstr(change, 8)} exceeds "
                f"{mp.nstr(allowed, 8)}"
            )
        uncertainty = max(change, mp.mpf("1e-300"))
        ref = [float(v) for v in high]
        rows.append(
            [
                case_id,
                kind,
                bits(center[0]), bits(center[1]), bits(center[2]),
                bits(axis[0]), bits(axis[1]), bits(axis[2]),
                bits(current), bits(radius),
                bits(query[0]), bits(query[1]), bits(query[2]),
                bits(softening),
                repr(ref[0]), repr(ref[1]), repr(ref[2]),
                repr(float(uncertainty)),
                bits(length), turns,
                method,
            ]
        )
        print(
            f"{case_id:26s} ref=({ref[0]: .17e},{ref[1]: .17e},{ref[2]: .17e}) "
            f"change={float(change):.3e}",
            flush=True,
        )

    header = [
        "id", "kind",
        "center_x_bits", "center_y_bits", "center_z_bits",
        "axis_x_bits", "axis_y_bits", "axis_z_bits",
        "charge_bits", "dimension_bits",
        "query_x_bits", "query_y_bits", "query_z_bits",
        "softening_bits", "ref_x", "ref_y", "ref_z", "uncertainty",
        "length_bits", "turns", "method",
    ]
    lines = [",".join(header)]
    lines.extend(",".join(str(value) for value in row) for row in rows)
    OUT.write_text("\n".join(lines) + "\n")
    print(f"\nwrote {len(rows)} fixtures to {OUT}")


if __name__ == "__main__":
    main()
