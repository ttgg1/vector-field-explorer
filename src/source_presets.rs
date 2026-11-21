//! Built-in source-scene presets shown in the Presets panel.
//!
//! Every preset is illustrative rather than physical: each source is built
//! from [`SourceKindTag::default_source`] with unit charge/current (dipoles
//! use `+1`/`-1`), the scene keeps the default `0.1` softening and leaves the
//! accuracy opt-in off, and loading a preset frames the canonical
//! `[-2, 2]^3` domain. Formula presets are never converted into sources; this
//! catalogue is a separate set of ordinary [`SourceScene`] values.

use glam::{DVec3, Vec3};

use crate::plot::PlotKind;
use crate::source_scene::{FieldMode, SourceEntry, SourceKindTag, SourceScene};
use crate::sources::{Source, SourceKind};
use crate::viewmodel::State;

/// One built-in source preset: a stable name, a one-line explanation and the
/// scene builder the panel calls.
pub struct SourcePreset {
    pub name: &'static str,
    pub summary: &'static str,
    pub build: fn() -> SourceScene,
}

/// The seventeen source presets in panel order. The last one is a gallery that
/// places one source of every kind.
static PRESETS: [SourcePreset; 17] = [
    SourcePreset {
        name: "Coulomb point",
        summary: "A single positive point charge at the origin",
        build: coulomb_point,
    },
    SourcePreset {
        name: "Electric dipole",
        summary: "Equal and opposite point charges separated along x",
        build: electric_dipole,
    },
    SourcePreset {
        name: "Charged line",
        summary: "A finite line charge along the z axis",
        build: charged_line,
    },
    SourcePreset {
        name: "Uniform ring",
        summary: "A uniformly charged ring in the xy plane",
        build: uniform_ring,
    },
    SourcePreset {
        name: "Thin disc",
        summary: "A uniformly charged thin disc",
        build: thin_disc,
    },
    SourcePreset {
        name: "Charged sphere",
        summary: "A uniformly charged solid sphere",
        build: charged_sphere,
    },
    SourcePreset {
        name: "Infinite wire",
        summary: "A straight current-carrying wire along z",
        build: infinite_wire,
    },
    SourcePreset {
        name: "Current loop",
        summary: "A circular current loop around the z axis",
        build: current_loop,
    },
    SourcePreset {
        name: "Finite solenoid",
        summary: "A finite current-carrying solenoid",
        build: finite_solenoid,
    },
    SourcePreset {
        name: "Unequal charges",
        summary: "Four alternating point charges in a quadrupole arrangement",
        build: unequal_charges,
    },
    SourcePreset {
        name: "Off-axis ring",
        summary: "A charged ring displaced from the origin",
        build: off_axis_ring,
    },
    SourcePreset {
        name: "Sphere inside and out",
        summary: "A solid charged sphere sampled inside and outside",
        build: sphere_inside_and_out,
    },
    SourcePreset {
        name: "Paired loops",
        summary: "Two widely separated coaxial current loops",
        build: paired_loops,
    },
    SourcePreset {
        name: "Helmholtz pair",
        summary: "Two coaxial loops at the classic one-radius separation",
        build: helmholtz_pair,
    },
    SourcePreset {
        name: "Opposing wires",
        summary: "Two parallel straight wires carrying opposite currents",
        build: opposing_wires,
    },
    SourcePreset {
        name: "Solenoid ends",
        summary: "A finite solenoid with visible end effects",
        build: solenoid_ends,
    },
    SourcePreset {
        name: "Source gallery",
        summary: "One illustrative example of each of the eight source kinds",
        build: source_gallery,
    },
];

/// The source-preset catalogue in panel order.
pub fn source_presets() -> &'static [SourcePreset] {
    &PRESETS
}

/// Look up one source preset by its stable name.
pub fn source_preset(name: &str) -> Option<&'static SourcePreset> {
    source_presets().iter().find(|preset| preset.name == name)
}

/// The signed charge or current carried by a source.
pub fn source_strength(source: &Source) -> f64 {
    match source.kind {
        SourceKind::PointCharge { charge }
        | SourceKind::FiniteLineCharge { charge, .. }
        | SourceKind::ChargedRing { charge, .. }
        | SourceKind::ChargedDisc { charge, .. }
        | SourceKind::SolidSphere { charge, .. } => charge,
        SourceKind::StraightWire { current }
        | SourceKind::CurrentLoop { current, .. }
        | SourceKind::Solenoid { current, .. } => current,
    }
}

/// Load one source preset through the ordinary scene setters. Returns false
/// without touching the state when the built scene fails validation.
pub fn load_source_preset(preset: &SourcePreset, state: &mut State) -> bool {
    let scene = (preset.build)();
    if let Err(error) = scene.validate() {
        log::warn!("source preset `{}` is invalid: {error:?}", preset.name);
        return false;
    }
    state.set_plot_kind(PlotKind::VectorField);
    state.set_field_mode(FieldMode::Sources);
    state.set_mode(true);
    state.set_domain(Vec3::splat(-2.0), Vec3::splat(2.0));
    state.set_source_scene(scene);
    true
}

fn scene(sources: Vec<Source>) -> SourceScene {
    SourceScene {
        entries: sources
            .into_iter()
            .map(|source| SourceEntry {
                enabled: true,
                source,
            })
            .collect(),
        ..SourceScene::default()
    }
}

fn at(tag: SourceKindTag, center: DVec3) -> Source {
    let mut source = tag.default_source();
    source.center = center;
    source
}

fn with_strength(mut source: Source, strength: f64) -> Source {
    match &mut source.kind {
        SourceKind::PointCharge { charge }
        | SourceKind::FiniteLineCharge { charge, .. }
        | SourceKind::ChargedRing { charge, .. }
        | SourceKind::ChargedDisc { charge, .. }
        | SourceKind::SolidSphere { charge, .. } => *charge = strength,
        SourceKind::StraightWire { current }
        | SourceKind::CurrentLoop { current, .. }
        | SourceKind::Solenoid { current, .. } => *current = strength,
    }
    source
}

fn coulomb_point() -> SourceScene {
    scene(vec![SourceKindTag::PointCharge.default_source()])
}

fn electric_dipole() -> SourceScene {
    scene(vec![
        with_strength(
            at(SourceKindTag::PointCharge, DVec3::new(0.5, 0.0, 0.0)),
            1.0,
        ),
        with_strength(
            at(SourceKindTag::PointCharge, DVec3::new(-0.5, 0.0, 0.0)),
            -1.0,
        ),
    ])
}

fn charged_line() -> SourceScene {
    scene(vec![SourceKindTag::FiniteLineCharge.default_source()])
}

fn uniform_ring() -> SourceScene {
    scene(vec![SourceKindTag::ChargedRing.default_source()])
}

fn thin_disc() -> SourceScene {
    scene(vec![SourceKindTag::ChargedDisc.default_source()])
}

fn charged_sphere() -> SourceScene {
    scene(vec![SourceKindTag::SolidSphere.default_source()])
}

fn infinite_wire() -> SourceScene {
    scene(vec![SourceKindTag::StraightWire.default_source()])
}

fn current_loop() -> SourceScene {
    scene(vec![SourceKindTag::CurrentLoop.default_source()])
}

fn finite_solenoid() -> SourceScene {
    scene(vec![SourceKindTag::Solenoid.default_source()])
}

fn unequal_charges() -> SourceScene {
    // Alternating signs at the corners of a square make a quadrupole-like
    // arrangement rather than two neutral pairs.
    scene(vec![
        with_strength(
            at(SourceKindTag::PointCharge, DVec3::new(-0.5, -0.5, 0.0)),
            1.0,
        ),
        with_strength(
            at(SourceKindTag::PointCharge, DVec3::new(0.5, -0.5, 0.0)),
            -1.0,
        ),
        with_strength(
            at(SourceKindTag::PointCharge, DVec3::new(-0.5, 0.5, 0.0)),
            -1.0,
        ),
        with_strength(
            at(SourceKindTag::PointCharge, DVec3::new(0.5, 0.5, 0.0)),
            1.0,
        ),
    ])
}

fn off_axis_ring() -> SourceScene {
    // The ring keeps its unit radius and axis but sits off the canonical axis.
    scene(vec![at(
        SourceKindTag::ChargedRing,
        DVec3::new(0.6, 0.35, 0.0),
    )])
}

fn sphere_inside_and_out() -> SourceScene {
    // The canonical [-2, 2]^3 domain shows both the interior and exterior.
    scene(vec![SourceKindTag::SolidSphere.default_source()])
}

fn paired_loops() -> SourceScene {
    scene(vec![
        at(SourceKindTag::CurrentLoop, DVec3::new(0.0, 0.0, -1.5)),
        at(SourceKindTag::CurrentLoop, DVec3::new(0.0, 0.0, 1.5)),
    ])
}

fn helmholtz_pair() -> SourceScene {
    // Classic Helmholtz spacing equals the loop radius (the default 1.0).
    scene(vec![
        at(SourceKindTag::CurrentLoop, DVec3::new(0.0, 0.0, -0.5)),
        at(SourceKindTag::CurrentLoop, DVec3::new(0.0, 0.0, 0.5)),
    ])
}

fn opposing_wires() -> SourceScene {
    scene(vec![
        with_strength(
            at(SourceKindTag::StraightWire, DVec3::new(-0.5, 0.0, 0.0)),
            1.0,
        ),
        with_strength(
            at(SourceKindTag::StraightWire, DVec3::new(0.5, 0.0, 0.0)),
            -1.0,
        ),
    ])
}

fn solenoid_ends() -> SourceScene {
    // The default length-2 solenoid spans z in [-1, 1], so the canonical
    // [-2, 2]^3 domain extends past both ends.
    scene(vec![SourceKindTag::Solenoid.default_source()])
}

fn source_gallery() -> SourceScene {
    let centers = [
        DVec3::new(-1.2, -1.2, 0.0),
        DVec3::new(0.0, -1.2, 0.0),
        DVec3::new(1.2, -1.2, 0.0),
        DVec3::new(-1.2, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.2, 0.0, 0.0),
        DVec3::new(-1.2, 1.2, 0.0),
        DVec3::new(0.0, 1.2, 0.0),
    ];
    let sources = SourceKindTag::ALL
        .iter()
        .zip(centers)
        .map(|(tag, center)| at(*tag, center))
        .collect();
    scene(sources)
}
