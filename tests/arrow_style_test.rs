//! Ticket 45: the arrow style families, their geometry parameters, and the
//! scene's 2D/3D arrow selection. Ticket 61 adds the independent 2D set;
//! ticket 64 renames the flat ribbon to Flat, removes Pyramid, and adds the
//! headless Line/Streamline family.
//!
//! There is no GPU in the test harness, so these tests pin the pure data model,
//! the draw-count helper the renderer uses, and the source constants the
//! shaders must keep in step with.

use serde::{Deserialize, Serialize};

use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D, MIN_SIDES};
use vector_field_explorer::renderer::{
    ARROW2D_VERTS, ARROW2D_VERTS_LINE, ARROW_VERTS_FLAT, ARROW_VERTS_LINE, ArrowDrawPlan, SceneGPU,
};
use vector_field_explorer::viewmodel::State;

const ARROW_SHADER: &str = include_str!("../src/shaders.arrow.wgsl");
const ARROW2D_SHADER: &str = include_str!("../src/shaders.arrow2d.wgsl");
const LINE_SHADER: &str = include_str!("../src/shaders.line.wgsl");
const DOT_SHADER: &str = include_str!("../src/shaders.dot.wgsl");

#[test]
fn styles_have_the_display_order_and_labels() {
    assert_eq!(
        ArrowStyle::ALL.map(ArrowStyle::label),
        ["Cone", "Cylinder", "Flat", "Line"],
        "the selector order is the enum order; Streamline is shown as Line",
    );
}

#[test]
fn the_2d_set_has_its_own_display_order_and_labels() {
    assert_eq!(
        ArrowStyle2D::ALL.map(ArrowStyle2D::label),
        ["Flat", "Line"],
        "the 2D selector order is the 2D enum order",
    );
    for style in ArrowStyle2D::ALL {
        assert!(!style.label().is_empty(), "{style:?} has a label");
        assert!(!style.tooltip().is_empty(), "{style:?} has a tooltip");
    }
}

#[test]
fn styles_have_stable_shader_ids() {
    assert_eq!(
        ArrowStyle::ALL.map(ArrowStyle::id),
        [0, 1, 2, 3],
        "Cone 0, Cylinder 1, Flat 2, Line 3",
    );
    assert_eq!(
        ArrowStyle2D::ALL.map(ArrowStyle2D::id),
        [0, 1],
        "2D Flat 0, Line 1",
    );
}

#[test]
fn vertex_counts_match_the_family_budgets() {
    for sides in [4, 5, 12, 16] {
        assert_eq!(ArrowStyle::Cone.vertex_count(sides), 12 * sides - 6);
        assert_eq!(ArrowStyle::Cylinder.vertex_count(sides), 9 * sides - 6);
        assert_eq!(ArrowStyle::Flat.vertex_count(sides), 9);
        assert_eq!(ArrowStyle::Streamline.vertex_count(sides), 6);
        for style in ArrowStyle::ALL {
            let count = style.vertex_count(sides);
            assert!(count > 0, "{style:?} emits vertices");
            assert_eq!(count % 3, 0, "{style:?} emits whole triangles");
        }
    }
    assert_eq!(ArrowStyle::Cone.vertex_count(16), 186, "the 16-side cone budget");
    assert_eq!(ArrowStyle::Cone.vertex_count(12), 138, "the default full-cap fan");
    assert_eq!(ArrowStyle::Cylinder.vertex_count(12), 102, "shaft plus far-end cap");
    assert_eq!(ArrowStyle2D::Flat.vertex_count(12), 9);
    assert_eq!(ArrowStyle2D::Streamline.vertex_count(12), 6);
}

#[test]
fn uses_sides_and_uses_head_match_the_families() {
    assert!(ArrowStyle::Cone.uses_sides());
    assert!(ArrowStyle::Cylinder.uses_sides());
    assert!(!ArrowStyle::Flat.uses_sides());
    assert!(!ArrowStyle::Streamline.uses_sides());

    assert!(ArrowStyle::Cone.uses_head());
    assert!(!ArrowStyle::Cylinder.uses_head());
    assert!(ArrowStyle::Flat.uses_head());
    assert!(!ArrowStyle::Streamline.uses_head());

    assert!(!ArrowStyle2D::Flat.uses_sides());
    assert!(!ArrowStyle2D::Streamline.uses_sides());
    assert!(ArrowStyle2D::Flat.uses_head());
    assert!(!ArrowStyle2D::Streamline.uses_head());
}

#[test]
fn defaults_are_the_ticket_values() {
    assert_eq!(ArrowStyle::default(), ArrowStyle::Cone, "3D starts on the cone");
    assert_eq!(ArrowStyle2D::default(), ArrowStyle2D::Flat, "2D starts on the flat ribbon");

    let params = ArrowParams::default();
    assert_eq!(params.head_length, 0.3);
    assert_eq!(params.head_width, 0.12);
    assert_eq!(params.shaft_radius, 0.055);
    assert_eq!(params.sides, 12);
}

#[test]
fn non_finite_params_fall_back_to_the_defaults_and_finite_ones_clamp() {
    let defaults = ArrowParams::default();
    let non_finite = ArrowParams {
        head_length: f32::NAN,
        head_width: f32::NEG_INFINITY,
        shaft_radius: f32::INFINITY,
        sides: 1,
    }
    .sanitized();
    assert_eq!(non_finite.head_length, defaults.head_length, "a NaN takes its default");
    assert_eq!(non_finite.head_width, defaults.head_width, "an infinite width takes its default");
    assert_eq!(non_finite.shaft_radius, defaults.shaft_radius, "an infinite radius takes its default");
    assert_eq!(non_finite.sides, MIN_SIDES, "the finite but tiny side count clamps");

    let clamped = ArrowParams {
        head_length: 0.0,
        head_width: 1.0,
        shaft_radius: -2.0,
        sides: 99,
    }
    .sanitized();
    assert_eq!(clamped.head_length, 0.1);
    assert_eq!(clamped.head_width, 0.3);
    assert_eq!(clamped.shaft_radius, 0.01);
    assert_eq!(clamped.sides, 16);
}

#[test]
fn scene_carries_the_style_params_and_mode() {
    let scene = SceneGPU::default();
    assert_eq!(scene.arrow_style, ArrowStyle::Cone, "the default keeps the old cone look");
    assert_eq!(scene.arrow_style_2d, ArrowStyle2D::Flat, "the 2D default is the flat ribbon");
    assert_eq!(scene.arrow_params, ArrowParams::default());
    assert!(scene.mode_3d, "the default keeps the old 3D pipeline");
    assert_eq!(
        scene.arrow_vertex_count(),
        138,
        "the default Cone is 12-sided with a full cap",
    );

    let mut scene = SceneGPU { mode_3d: true, ..SceneGPU::default() };
    scene.arrow_style = ArrowStyle::Flat;
    assert_eq!(scene.arrow_vertex_count(), 9, "a 3D flat ribbon draws its 9 vertices");

    scene.arrow_style = ArrowStyle::Streamline;
    assert_eq!(scene.arrow_vertex_count(), 6, "a 3D line draws its 6 vertices");

    scene.arrow_style = ArrowStyle::Cylinder;
    scene.arrow_params.sides = 8;
    assert_eq!(scene.arrow_vertex_count(), 9 * 8 - 6, "a 3D cylinder draws by its sides");
    assert!(!scene.uses_flat_arrow());

    scene.mode_3d = false;
    scene.arrow_style = ArrowStyle::Flat;
    scene.arrow_style_2d = ArrowStyle2D::Flat;
    scene.arrow_params.sides = 16;
    assert_eq!(
        scene.arrow_vertex_count(),
        9,
        "2D draws the flat arrow from its own set",
    );
    assert!(scene.uses_flat_arrow(), "2D ignores the selected 3D family");

    scene.arrow_style_2d = ArrowStyle2D::Streamline;
    assert_eq!(scene.arrow_vertex_count(), 6, "2D line draws its own 6-vertex budget");
    assert!(scene.uses_flat_arrow(), "2D line still uses the flat pipeline");
}

#[test]
fn arrow_draw_plan_selects_the_pipeline_and_count() {
    let mut scene = SceneGPU::default();
    assert_eq!(
        scene.arrow_draw_plan(),
        ArrowDrawPlan::Mesh3D { vertices: 138 },
        "the default Cone uses the 3D mesh at 12 sides",
    );

    scene.arrow_style = ArrowStyle::Flat;
    assert_eq!(scene.arrow_draw_plan(), ArrowDrawPlan::Mesh3D { vertices: 9 });

    scene.arrow_style = ArrowStyle::Streamline;
    assert_eq!(scene.arrow_draw_plan(), ArrowDrawPlan::Mesh3D { vertices: 6 });

    scene.arrow_style = ArrowStyle::Cylinder;
    scene.arrow_params.sides = 12;
    assert_eq!(scene.arrow_draw_plan(), ArrowDrawPlan::Mesh3D { vertices: 102 });

    scene.mode_3d = false;
    scene.arrow_style_2d = ArrowStyle2D::Flat;
    assert_eq!(
        scene.arrow_draw_plan(),
        ArrowDrawPlan::Flat2D { vertices: 9 },
        "2D selects the flat pipeline and its 2D family's count",
    );

    scene.arrow_style_2d = ArrowStyle2D::Streamline;
    assert_eq!(
        scene.arrow_draw_plan(),
        ArrowDrawPlan::Flat2D { vertices: 6 },
        "2D line keeps the flat pipeline at 6 vertices",
    );
}

#[test]
fn every_2d_family_has_a_draw_plan() {
    for style in ArrowStyle2D::ALL {
        let scene = SceneGPU { mode_3d: false, arrow_style_2d: style, ..SceneGPU::default() };
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Flat2D { vertices: style.vertex_count(12) },
            "{style:?} selects the flat pipeline at its own count",
        );
        assert_eq!(
            scene.arrow_vertex_count(),
            style.vertex_count(12),
            "{style:?} emits its own budget",
        );
        assert!(scene.uses_flat_arrow(), "{style:?} paints the flat arrow");
    }
}

#[test]
fn regression_2d_draw_plan_reads_only_the_2d_style() {
    // Ticket 61 regression: the 3D family must never leak into the 2D plan.
    let mut scene = SceneGPU { mode_3d: false, ..SceneGPU::default() };
    scene.arrow_style_2d = ArrowStyle2D::Flat;
    for style in ArrowStyle::ALL {
        scene.arrow_style = style;
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Flat2D { vertices: 9 },
            "the 3D {style:?} family does not affect the 2D plan",
        );
        assert!(scene.uses_flat_arrow(), "the 2D plan stays flat for {style:?}");
    }
    for style in ArrowStyle2D::ALL {
        scene.arrow_style_2d = style;
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Flat2D { vertices: style.vertex_count(12) },
            "the 2D {style:?} family drives the 2D plan",
        );
    }
}

#[test]
fn three_d_draw_plan_ignores_the_2d_style() {
    let mut scene = SceneGPU { mode_3d: true, ..SceneGPU::default() };
    scene.arrow_style = ArrowStyle::Flat;
    for style in ArrowStyle2D::ALL {
        scene.arrow_style_2d = style;
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Mesh3D { vertices: 9 },
            "the 2D {style:?} family does not affect the 3D plan",
        );
    }
}

#[test]
fn the_two_style_fields_never_alias() {
    let mut state = State::new(false);
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Flat, "2D starts on the flat ribbon");

    assert!(state.set_arrow_style(ArrowStyle::Streamline), "the 3D family changes");
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Flat, "the 2D family is untouched");

    // The 2D set now has two variants, so this loop exercises a real change.
    for style in ArrowStyle2D::ALL {
        let changed = state.arrow_style_2d != style;
        assert_eq!(
            state.set_arrow_style_2d(style),
            changed,
            "the 2D setter reports only a real change",
        );
        assert_eq!(
            state.arrow_style,
            ArrowStyle::Streamline,
            "the 3D family is untouched",
        );
    }
}

#[test]
fn snapshot_carries_the_2d_style_into_the_draw_plan() {
    let mut state = State::new(false);
    let scene = state.snapshot();
    assert_eq!(scene.arrow_style_2d, ArrowStyle2D::Flat, "the snapshot carries the 2D set");
    assert_eq!(
        scene.arrow_draw_plan(),
        ArrowDrawPlan::Flat2D { vertices: 9 },
        "2D draws the flat arrow",
    );

    state.set_arrow_style_2d(ArrowStyle2D::Streamline);
    let scene = state.snapshot();
    assert_eq!(
        scene.arrow_draw_plan(),
        ArrowDrawPlan::Flat2D { vertices: 6 },
        "the snapshot follows the 2D line family",
    );
}

#[test]
fn set_arrow_style_2d_follows_the_mark_dirty_contract() {
    let mut state = State::new(false);
    for style in ArrowStyle2D::ALL {
        let changed = state.arrow_style_2d != style;
        state.dirty = false;
        assert_eq!(
            state.set_arrow_style_2d(style),
            changed,
            "the setter reports only a real change",
        );
        assert_eq!(state.dirty, changed, "the setter dirties exactly when the family changes");
    }
}

#[test]
fn hiding_arrows_empties_only_the_arrow_instances() {
    let mut state = State::new(false);
    state.set_streamlines(true);
    let with_arrows = state.snapshot();
    assert!(!with_arrows.arrows.is_empty(), "the default scene has arrows");
    assert!(!with_arrows.lines.is_empty(), "the flow produces streamline segments");

    state.set_arrows(false);
    let hidden = state.snapshot();
    assert!(hidden.arrows.is_empty(), "Show arrows off empties the arrow list");
    assert_eq!(hidden.lines.len(), with_arrows.lines.len(), "streamlines are untouched");
    assert_eq!(hidden.dots.len(), with_arrows.dots.len(), "dots are untouched");
}

#[test]
fn old_style_names_decode_to_the_new_families_and_new_names_are_stable() {
    // Exact old spellings from saved records. `Line` meant the flat ribbon
    // before ticket 64; the new Line saves as `Streamline` so this cannot
    // become ambiguous.
    assert_eq!(serde_json::from_str::<ArrowStyle>("\"Pyramid\"").unwrap(), ArrowStyle::Cone);
    assert_eq!(serde_json::from_str::<ArrowStyle>("\"Line\"").unwrap(), ArrowStyle::Flat);
    assert_eq!(
        serde_json::from_str::<ArrowStyle2D>("\"Line\"").unwrap(),
        ArrowStyle2D::Flat,
    );

    #[derive(Debug, Deserialize)]
    struct OldRecord {
        arrow_style: ArrowStyle,
        arrow_style_2d: ArrowStyle2D,
    }
    let old: OldRecord = serde_json::from_str(
        r#"{"arrow_style":"Pyramid","arrow_style_2d":"Line"}"#,
    )
    .unwrap();
    assert_eq!(old.arrow_style, ArrowStyle::Cone, "old Pyramid becomes Cone");
    assert_eq!(old.arrow_style_2d, ArrowStyle2D::Flat, "old 2D Line becomes Flat");

    let old: OldRecord = serde_json::from_str(
        r#"{"arrow_style":"Line","arrow_style_2d":"Line"}"#,
    )
    .unwrap();
    assert_eq!(old.arrow_style, ArrowStyle::Flat, "old 3D Line becomes Flat");

    // The new persisted spellings are stable and round-trip.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Record {
        arrow_style: ArrowStyle,
        arrow_style_2d: ArrowStyle2D,
    }
    let record = Record {
        arrow_style: ArrowStyle::Streamline,
        arrow_style_2d: ArrowStyle2D::Streamline,
    };
    let json = serde_json::to_string(&record).unwrap();
    assert_eq!(json, r#"{"arrow_style":"Streamline","arrow_style_2d":"Streamline"}"#);
    assert_eq!(serde_json::from_str::<Record>(&json).unwrap(), record);

    assert_eq!(serde_json::to_string(&ArrowStyle::Flat).unwrap(), "\"Flat\"");
    assert_eq!(serde_json::to_string(&ArrowStyle2D::Flat).unwrap(), "\"Flat\"");
    for style in ArrowStyle::ALL {
        let json = serde_json::to_string(&style).unwrap();
        assert_eq!(serde_json::from_str::<ArrowStyle>(&json).unwrap(), style);
    }
    for style in ArrowStyle2D::ALL {
        let json = serde_json::to_string(&style).unwrap();
        assert_eq!(serde_json::from_str::<ArrowStyle2D>(&json).unwrap(), style);
    }
}

#[test]
fn every_shader_globals_struct_carries_the_arrow_uniforms() {
    for (name, source) in [
        ("arrow", ARROW_SHADER),
        ("arrow2d", ARROW2D_SHADER),
        ("line", LINE_SHADER),
        ("dot", DOT_SHADER),
    ] {
        assert!(source.contains("arrow_style: vec4<u32>"), "{name} declares arrow_style");
        assert!(source.contains("arrow_params: vec4<f32>"), "{name} declares arrow_params");
    }
    assert!(
        ARROW2D_SHADER.contains("x: 2D style id"),
        "the 2D shader documents that arrow_style.x is the 2D id in 2D",
    );
}

#[test]
fn shader_vertex_budgets_match_the_rust_constants() {
    assert_eq!(ARROW_VERTS_FLAT, 9, "the 3D flat ribbon");
    assert_eq!(ARROW_VERTS_LINE, 6, "the 3D headless line");
    assert_eq!(ARROW2D_VERTS, 9, "the flat 2D arrow");
    assert_eq!(ARROW2D_VERTS_LINE, 6, "the 2D headless line");
    assert!(ARROW_SHADER.contains("const ARROW_VERTS_FLAT: u32 = 9u;"));
    assert!(ARROW_SHADER.contains("const ARROW_VERTS_LINE: u32 = 6u;"));
    assert!(ARROW2D_SHADER.contains("const ARROW2D_VERTS: u32 = 9u;"));
    assert!(ARROW2D_SHADER.contains("const ARROW2D_VERTS_LINE: u32 = 6u;"));
    assert!(
        ARROW_SHADER.contains("return ARROW_VERTS_FLAT;"),
        "the Flat branch actually returns the 9-vertex budget",
    );
    assert!(
        ARROW_SHADER.contains("return ARROW_VERTS_LINE;"),
        "the Line branch actually returns the ribbon budget",
    );
    assert!(
        ARROW_SHADER.contains("return 12u * sides - 6u;"),
        "Cone includes the full 3*(sides-2) plate fan",
    );
    assert!(
        ARROW_SHADER.contains("return 9u * sides - 6u;"),
        "Cylinder is a shaft plus a far-end cap, with no head",
    );
}

#[test]
fn the_3d_shader_branches_on_all_four_styles() {
    for id in 0..4 {
        assert!(
            ARROW_SHADER.contains(&format!("style == {id}u")),
            "the 3D shader handles style id {id}",
        );
    }
    assert!(
        ARROW_SHADER.contains("if (style == 0u)"),
        "Cone has its own branch",
    );
    assert!(
        ARROW_SHADER.contains("if (style == 1u)"),
        "Cylinder has its own branch",
    );
    assert!(ARROW_SHADER.contains("arrow_style.y"), "the side count comes from the uniform");
    assert!(
        ARROW_SHADER.contains("plate_j / 3u"),
        "the cone plate branch walks the full fan, not a fixed two triangles",
    );
    assert!(
        ARROW_SHADER.contains("tri + 2u"),
        "the fan's last triangle reaches the final ring corner",
    );
    assert!(!ARROW_SHADER.contains("PYRAMID"), "the Pyramid branch and constants are gone");
    assert!(!ARROW_SHADER.contains("const ARROW_VERTS: u32 = 42u;"), "the 42-vertex budget is gone");
}

#[test]
fn the_3d_shader_derives_shaft_normals_from_the_taper() {
    assert!(ARROW_SHADER.contains("fn shaft_normal("), "the taper-aware normal helper exists");
    assert!(
        ARROW_SHADER.contains("bottom_radius - top_radius"),
        "the normal leans with the actual radius change",
    );
    assert!(
        ARROW_SHADER.contains("shaft_normal(t, shaft_radius, top_radius, shaft_len)"),
        "Cone shaft shading uses the taper-aware normal",
    );
}

#[test]
fn the_3d_line_is_a_headless_camera_facing_ribbon() {
    assert!(
        ARROW_SHADER.contains("cross(dir, to_cam)"),
        "the Line style builds a camera-facing side vector like shaders.line.wgsl",
    );
    assert!(
        ARROW_SHADER.contains("globals.camera_right.xyz"),
        "the Line style falls back to the camera right axis when parallel",
    );
    assert!(
        ARROW_SHADER.contains("inst.color, 1.0"),
        "the Line style is unlit",
    );
}

#[test]
fn the_2d_shader_branches_on_its_own_style_set() {
    assert!(
        ARROW2D_SHADER.contains("style == 1u"),
        "the 2D shader handles its Line id",
    );
    assert!(
        ARROW2D_SHADER.contains("ARROW2D_VERTS_LINE"),
        "the 2D shader uses the 6-vertex line budget",
    );
    assert!(
        !ARROW2D_SHADER.contains("const ARROW2D_VERTS: u32 = 6u;"),
        "Flat keeps the 9-vertex budget",
    );
}
