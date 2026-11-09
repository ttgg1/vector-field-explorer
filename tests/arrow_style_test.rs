//! Ticket 45: the arrow style families, their geometry parameters, and the
//! scene's 2D/3D arrow selection. Ticket 61 adds the independent 2D set.
//!
//! There is no GPU in the test harness, so these tests pin the pure data model,
//! the draw-count helper the renderer uses, and the source constants the
//! shaders must keep in step with.

use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D, MIN_SIDES};
use vector_field_explorer::renderer::{
    ARROW2D_VERTS, ARROW_VERTS, ARROW_VERTS_LINE, ArrowDrawPlan, SceneGPU,
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
        ["Cone", "Pyramid", "Cylinder", "Line"],
        "the selector order is the enum order",
    );
}

#[test]
fn the_2d_set_has_its_own_display_order_and_labels() {
    assert_eq!(
        ArrowStyle2D::ALL.map(ArrowStyle2D::label),
        ["Line"],
        "the 2D selector order is the 2D enum order",
    );
    for style in ArrowStyle2D::ALL {
        assert!(!style.label().is_empty(), "{style:?} has a label");
        assert!(!style.tooltip().is_empty(), "{style:?} has a tooltip");
    }
}

#[test]
fn vertex_counts_match_the_family_budgets() {
    for sides in [4, 5, 12, 16] {
        assert_eq!(ArrowStyle::Pyramid.vertex_count(sides), 42, "the 44 layout");
        assert_eq!(ArrowStyle::Cone.vertex_count(sides), 12 * sides - 6);
        assert_eq!(ArrowStyle::Cylinder.vertex_count(sides), 12 * sides - 6);
        assert_eq!(ArrowStyle::Line.vertex_count(sides), 9);
        for style in ArrowStyle::ALL {
            let count = style.vertex_count(sides);
            assert!(count > 0, "{style:?} emits vertices");
            assert_eq!(count % 3, 0, "{style:?} emits whole triangles");
        }
    }
    assert_eq!(ArrowStyle::Cone.vertex_count(4), 42, "four sides collapse to the 44 budget");
    assert_eq!(ArrowStyle::Cone.vertex_count(12), 138, "the default full-cap fan");
    assert_eq!(ArrowStyle::Cylinder.vertex_count(12), 138);
}

#[test]
fn defaults_are_the_ticket_values() {
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
    assert_eq!(scene.arrow_style_2d, ArrowStyle2D::Line, "the 2D default is the flat line");
    assert_eq!(scene.arrow_params, ArrowParams::default());
    assert!(scene.mode_3d, "the default keeps the old 3D pipeline");
    assert_eq!(
        scene.arrow_vertex_count(),
        138,
        "the default Cone is 12-sided with a full cap",
    );

    let mut scene = SceneGPU { mode_3d: true, ..SceneGPU::default() };
    scene.arrow_style = ArrowStyle::Pyramid;
    assert_eq!(scene.arrow_vertex_count(), 42, "a 3D pyramid draws its 42 vertices");

    scene.arrow_style = ArrowStyle::Cylinder;
    scene.arrow_params.sides = 8;
    assert_eq!(scene.arrow_vertex_count(), 12 * 8 - 6, "a 3D cylinder draws by its sides");
    assert!(!scene.uses_flat_arrow());

    scene.mode_3d = false;
    scene.arrow_style = ArrowStyle::Pyramid;
    scene.arrow_style_2d = ArrowStyle2D::Line;
    scene.arrow_params.sides = 16;
    assert_eq!(
        scene.arrow_vertex_count(),
        9,
        "2D draws the flat arrow from its own set",
    );
    assert!(scene.uses_flat_arrow(), "2D ignores the selected 3D family");
}

#[test]
fn arrow_draw_plan_selects_the_pipeline_and_count() {
    let mut scene = SceneGPU::default();
    assert_eq!(
        scene.arrow_draw_plan(),
        ArrowDrawPlan::Mesh3D { vertices: 138 },
        "the default Cone uses the 3D mesh at 12 sides",
    );

    scene.arrow_style = ArrowStyle::Pyramid;
    assert_eq!(scene.arrow_draw_plan(), ArrowDrawPlan::Mesh3D { vertices: 42 });

    scene.arrow_style = ArrowStyle::Line;
    assert_eq!(scene.arrow_draw_plan(), ArrowDrawPlan::Mesh3D { vertices: 9 });

    scene.mode_3d = false;
    assert_eq!(
        scene.arrow_draw_plan(),
        ArrowDrawPlan::Flat2D,
        "2D selects the flat pipeline from the 2D set",
    );
}

#[test]
fn every_2d_family_has_a_draw_plan() {
    for style in ArrowStyle2D::ALL {
        let scene = SceneGPU { mode_3d: false, arrow_style_2d: style, ..SceneGPU::default() };
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Flat2D,
            "{style:?} selects the flat pipeline",
        );
        assert_eq!(scene.arrow_vertex_count(), ARROW2D_VERTS, "{style:?} emits the flat budget");
        assert!(scene.uses_flat_arrow(), "{style:?} paints the flat arrow");
    }
}

#[test]
fn regression_2d_draw_plan_reads_only_the_2d_style() {
    // Ticket 61 regression: the 3D family must never leak into the 2D plan.
    let mut scene = SceneGPU { mode_3d: false, ..SceneGPU::default() };
    for style in ArrowStyle::ALL {
        scene.arrow_style = style;
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Flat2D,
            "the 3D {style:?} family does not affect the 2D plan",
        );
        assert!(scene.uses_flat_arrow(), "the 2D plan stays flat for {style:?}");
    }
    for style in ArrowStyle2D::ALL {
        scene.arrow_style_2d = style;
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Flat2D,
            "the 2D {style:?} family drives the 2D plan",
        );
    }
}

#[test]
fn three_d_draw_plan_ignores_the_2d_style() {
    let mut scene = SceneGPU { mode_3d: true, ..SceneGPU::default() };
    scene.arrow_style = ArrowStyle::Pyramid;
    for style in ArrowStyle2D::ALL {
        scene.arrow_style_2d = style;
        assert_eq!(
            scene.arrow_draw_plan(),
            ArrowDrawPlan::Mesh3D { vertices: 42 },
            "the 2D {style:?} family does not affect the 3D plan",
        );
    }
}

#[test]
fn the_two_style_fields_never_alias() {
    let mut state = State::new(false);
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Line, "2D starts on the line family");

    assert!(state.set_arrow_style(ArrowStyle::Pyramid), "the 3D family changes");
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Line, "the 2D family is untouched");

    for style in ArrowStyle2D::ALL {
        let changed = state.arrow_style_2d != style;
        assert_eq!(
            state.set_arrow_style_2d(style),
            changed,
            "the 2D setter reports only a real change",
        );
        assert_eq!(state.arrow_style, ArrowStyle::Pyramid, "the 3D family is untouched");
    }
}

#[test]
fn snapshot_carries_the_2d_style_into_the_draw_plan() {
    let state = State::new(false);
    let scene = state.snapshot();
    assert_eq!(scene.arrow_style_2d, ArrowStyle2D::Line, "the snapshot carries the 2D set");
    assert_eq!(scene.arrow_draw_plan(), ArrowDrawPlan::Flat2D, "2D draws the flat arrow");
}

#[test]
fn set_arrow_style_2d_follows_the_mark_dirty_contract() {
    // With one family today the change branch is empty, so this loops `ALL`:
    // a second family automatically starts asserting that it dirties the scene.
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
}

#[test]
fn shader_vertex_budgets_match_the_rust_constants() {
    assert_eq!(ARROW_VERTS, 42, "the Pyramid budget from ticket 44");
    assert_eq!(ARROW_VERTS_LINE, 9, "the 3D Line ribbon");
    assert_eq!(ARROW2D_VERTS, 9, "the flat 2D arrow");
    assert!(ARROW_SHADER.contains("const ARROW_VERTS: u32 = 42u;"));
    assert!(ARROW_SHADER.contains("const ARROW_VERTS_LINE: u32 = 9u;"));
    assert!(ARROW2D_SHADER.contains("const ARROW2D_VERTS: u32 = 9u;"));
    assert!(
        ARROW_SHADER.contains("return ARROW_VERTS;"),
        "the Pyramid branch actually returns the 42-vertex budget",
    );
    assert!(
        ARROW_SHADER.contains("return ARROW_VERTS_LINE;"),
        "the Line branch actually returns the ribbon budget",
    );
    assert!(
        ARROW_SHADER.contains("return 12u * sides - 6u;"),
        "Cone/Cylinder include the full 3*(sides-2) plate fan",
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
        ARROW_SHADER.contains("(style == 0u) || (style == 2u)"),
        "Cone and Cylinder share the n-sided shaft/head branch",
    );
    assert!(ARROW_SHADER.contains("PYRAMID_TIP_RATIO"), "Pyramid keeps the 44 shaft taper");
    assert!(ARROW_SHADER.contains("arrow_style.y"), "the side count comes from the uniform");
    assert!(
        ARROW_SHADER.contains("plate_j / 3u"),
        "the plate branch walks the full fan, not a fixed two triangles",
    );
    assert!(
        ARROW_SHADER.contains("tri + 2u"),
        "the fan's last triangle reaches the final ring corner",
    );
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
        "Cone/Cylinder shaft shading uses the taper-aware normal",
    );
    assert!(
        ARROW_SHADER.contains("shaft_normal(t, shaft_radius, shaft_radius * PYRAMID_TIP_RATIO, shaft_len)"),
        "Pyramid shaft shading uses its own taper",
    );
}
