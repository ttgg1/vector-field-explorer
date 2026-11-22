//! Issue #274: Solid/Wireframe bodies in Materials and Maxwell.
//!
//! The pure constructors are pinned here first; the panel checks use the same
//! headless-accesskit helpers as `material_panel_test.rs` and
//! `maxwell_panel_test.rs`.

use eframe::egui;
use glam::Vec3;

use crate::support;

use vector_field_explorer::app::object_scene_for_state;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::material::{MaterialCase, MaterialScene};
use vector_field_explorer::maxwell::{ConductorOutline, MaxwellCase, MaxwellScene};
use vector_field_explorer::object_scene::{MeshObject, MeshPrimitive, MeshScene, ObjectStyle};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::material_panel::{self, MaterialPanelState};
use vector_field_explorer::ui::maxwell_panel::{self, MaxwellPanelState};
use vector_field_explorer::ui::sources_panel::{self, SourcesPanelState};
use vector_field_explorer::viewmodel::State;

fn frame_z() -> PlaneFrame {
    PlaneFrame {
        axis: PlaneAxis::Z,
        origin: Vec3::ZERO,
        right: Vec3::X,
        up: Vec3::Y,
        normal: Vec3::Z,
        half_u: 1.0,
        half_v: 1.0,
    }
}

fn translation(object: &MeshObject) -> Vec3 {
    Vec3::new(
        object.transform[3][0],
        object.transform[3][1],
        object.transform[3][2],
    )
}

fn unit_axis(object: &MeshObject) -> Vec3 {
    let value = Vec3::new(
        object.transform[1][0],
        object.transform[1][1],
        object.transform[1][2],
    );
    if value.length_squared() > 0.0 {
        value.normalize()
    } else {
        Vec3::ZERO
    }
}

fn column_length(object: &MeshObject, column: usize) -> f32 {
    let [x, y, z, _] = object.transform[column];
    (x * x + y * y + z * z).sqrt()
}

#[test]
fn material_sphere_fills_at_the_material_radius_and_carries_the_clip() {
    let domain = Domain::default();
    let scene = MaterialScene {
        radius: 0.75,
        ..MaterialScene::default()
    };

    let three_d = MeshScene::from_material(&scene, &domain, None);
    assert_eq!(three_d.objects().len(), 1);
    let sphere = three_d.objects()[0];
    assert_eq!(sphere.primitive, MeshPrimitive::Sphere);
    assert!(translation(&sphere).length() < 1e-6);
    assert!((column_length(&sphere, 0) - 0.75).abs() < 1e-5);
    assert!((column_length(&sphere, 1) - 0.75).abs() < 1e-5);
    assert_eq!(sphere.opacity, 1.0);
    assert_eq!(sphere.sign, 1);
    assert!((sphere.color[0] - 1.0).abs() < 1e-6);
    assert!(three_d.clip().is_none());

    let two_d = MeshScene::from_material(&scene, &domain, Some(&frame_z()));
    assert!(two_d.clip().is_some());
    assert_eq!(two_d.generation(), three_d.generation());
    assert!(!three_d.cache_matches(&two_d));
}

#[test]
fn material_boundary_is_a_thin_disc_on_the_interface_not_a_filled_half_space() {
    let domain = Domain::default();
    let scene = MaterialScene {
        case: MaterialCase::DielectricBoundary,
        interface_axis: PlaneAxis::Z,
        interface_offset: 0.5,
        ..MaterialScene::default()
    };

    let mesh = MeshScene::from_material(&scene, &domain, None);
    assert_eq!(mesh.objects().len(), 1);
    let disc = mesh.objects()[0];
    assert_eq!(disc.primitive, MeshPrimitive::Disc);
    assert!((translation(&disc).z - 0.5).abs() < 1e-5);
    // The local +Y normal follows the interface plane's Z normal.
    assert!(unit_axis(&disc).dot(Vec3::Z) > 0.999);
}

#[test]
fn maxwell_plates_are_two_thin_discs_at_the_plate_positions() {
    let domain = Domain::default();
    let scene = MaxwellScene::default(); // parallel plates, separation 1
    let mesh = MeshScene::from_maxwell(&scene, &domain, None);

    assert_eq!(mesh.objects().len(), 2);
    assert_eq!(mesh.objects()[0].primitive, MeshPrimitive::Disc);
    assert_eq!(mesh.objects()[1].primitive, MeshPrimitive::Disc);
    assert!((translation(&mesh.objects()[0]).z + 0.5).abs() < 1e-5);
    assert!((translation(&mesh.objects()[1]).z - 0.5).abs() < 1e-5);
}

#[test]
fn maxwell_coaxial_maps_the_inner_conductor_and_wire_solenoid_image_cases() {
    let domain = Domain::default();
    let color = |scene: &MaxwellScene| MeshScene::from_maxwell(scene, &domain, None);

    let mut coaxial = MaxwellScene {
        case: MaxwellCase::CoaxialElectric,
        radius: 0.5,
        outer_radius: 2.0,
        ..MaxwellScene::default()
    };
    coaxial.case = MaxwellCase::CoaxialElectric;
    let mesh = color(&coaxial);
    assert_eq!(mesh.objects().len(), 1);
    assert_eq!(mesh.objects()[0].primitive, MeshPrimitive::Cylinder);
    assert!(unit_axis(&mesh.objects()[0]).dot(Vec3::Z) > 0.999);
    assert!((column_length(&mesh.objects()[0], 0) - 1.0).abs() < 1e-5);

    let wire = MaxwellScene {
        case: MaxwellCase::InfiniteWire,
        ..MaxwellScene::default()
    };
    assert_eq!(color(&wire).objects()[0].primitive, MeshPrimitive::Cylinder);

    let solenoid = MaxwellScene {
        case: MaxwellCase::Solenoid,
        ..MaxwellScene::default()
    };
    assert_eq!(color(&solenoid).objects()[0].primitive, MeshPrimitive::Tube);

    let image_sphere = MaxwellScene {
        case: MaxwellCase::ImageSphere,
        radius: 1.5,
        ..MaxwellScene::default()
    };
    let mesh = color(&image_sphere);
    assert_eq!(mesh.objects().len(), 1);
    assert_eq!(mesh.objects()[0].primitive, MeshPrimitive::Sphere);
    assert!((column_length(&mesh.objects()[0], 0) - 1.5).abs() < 1e-5);

    let image_plane = MaxwellScene {
        case: MaxwellCase::ImagePlane,
        charge_height: 0.75,
        ..MaxwellScene::default()
    };
    let mesh = color(&image_plane);
    assert_eq!(mesh.objects().len(), 2);
    assert_eq!(mesh.objects()[0].primitive, MeshPrimitive::Sphere);
    assert!((translation(&mesh.objects()[0]).z - 0.75).abs() < 1e-5);
    assert_eq!(mesh.objects()[1].primitive, MeshPrimitive::Disc);
    assert!(translation(&mesh.objects()[1]).length() < 1e-6);
}

#[test]
fn hollow_sphere_and_square_conductor_stay_outline_only_with_a_reason() {
    let domain = Domain::default();

    let hollow = MaxwellScene {
        case: MaxwellCase::HollowSphere,
        ..MaxwellScene::default()
    };
    let mesh = MeshScene::from_maxwell(&hollow, &domain, None);
    assert!(mesh.objects().is_empty());
    assert_eq!(mesh.generation(), 0);
    let note = vector_field_explorer::object_scene::maxwell_style_note(&hollow, None)
        .expect("hollow sphere needs an explanation");
    assert!(note.contains("thin shell"), "{note}");

    let square = MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Square { half_extent: 1.0 },
        ..MaxwellScene::default()
    };
    let three_d = MeshScene::from_maxwell(&square, &domain, None);
    let two_d = MeshScene::from_maxwell(&square, &domain, Some(&frame_z()));
    assert!(three_d.objects().is_empty());
    assert!(two_d.objects().is_empty());
    let note = vector_field_explorer::object_scene::maxwell_style_note(&square, Some(&frame_z()))
        .expect("square conductor needs an explanation");
    assert!(note.contains("no faithful box primitive"), "{note}");
}

#[test]
fn conductor_circle_is_mapped_only_on_its_2d_slice() {
    let domain = Domain::default();
    let circle = MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Circle { radius: 1.0 },
        ..MaxwellScene::default()
    };

    let three_d = MeshScene::from_maxwell(&circle, &domain, None);
    assert!(three_d.objects().is_empty());
    assert!(
        vector_field_explorer::object_scene::maxwell_style_note(&circle, None)
            .is_some_and(|note| note.contains("2D slice"))
    );

    let two_d = MeshScene::from_maxwell(&circle, &domain, Some(&frame_z()));
    assert_eq!(two_d.objects().len(), 1);
    assert_eq!(two_d.objects()[0].primitive, MeshPrimitive::Cylinder);
    assert!(two_d.clip().is_some());
    assert!(
        vector_field_explorer::object_scene::maxwell_style_note(&circle, Some(&frame_z()))
            .is_none()
    );
}

#[test]
fn object_scene_for_state_maps_materials_and_maxwell_and_keeps_formula_ode_empty() {
    let mut state = State::new(true);

    assert!(state.set_field_mode(FieldMode::Materials));
    assert!(!object_scene_for_state(&state).objects().is_empty());

    assert!(state.set_field_mode(FieldMode::Maxwell));
    assert!(!object_scene_for_state(&state).objects().is_empty());

    let solid = object_scene_for_state(&state).generation();
    assert!(state.set_object_style(ObjectStyle::Wireframe));
    assert_eq!(object_scene_for_state(&state).generation(), solid);
    assert!(state.set_object_style(ObjectStyle::Solid));

    assert!(state.set_field_mode(FieldMode::Formula));
    let empty = object_scene_for_state(&state);
    assert!(empty.objects().is_empty());
    assert_eq!(empty.generation(), 0);

    assert!(state.set_field_mode(FieldMode::Ode));
    assert!(object_scene_for_state(&state).objects().is_empty());
}

struct WidgetNode {
    label: String,
    rect: egui::Rect,
    toggled: Option<bool>,
}

fn nodes(output: &egui::FullOutput) -> Vec<WidgetNode> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            let label = node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_owned();
            Some(WidgetNode {
                label,
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
                toggled: node.toggled().map(|value| format!("{value:?}") == "True"),
            })
        })
        .collect()
}

fn find<'a>(nodes: &'a [WidgetNode], label: &str) -> Option<&'a WidgetNode> {
    nodes.iter().find(|node| node.label == label)
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn run_frame(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    mut draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 1600.0),
            )),
            events,
            ..Default::default()
        },
        &mut draw,
    );
    output.textures_delta.clear();
    output
}

fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

fn material_frame(
    ctx: &egui::Context,
    state: &mut State,
    panel: &mut MaterialPanelState,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    run_frame(ctx, events, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let _ = material_panel::show(ui, panel, state, false);
        });
    })
}

fn maxwell_frame(
    ctx: &egui::Context,
    state: &mut State,
    panel: &mut MaxwellPanelState,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    run_frame(ctx, events, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let _ = maxwell_panel::show(ui, panel, state, false);
        });
    })
}

fn sources_frame(
    ctx: &egui::Context,
    state: &mut State,
    panel: &mut SourcesPanelState,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    run_frame(ctx, events, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            sources_panel::show(ui, state, panel, false);
        });
    })
}

fn click(
    ctx: &egui::Context,
    mut draw: impl FnMut(&egui::Context, Vec<egui::Event>) -> egui::FullOutput,
    label: &str,
) {
    let out = draw(ctx, Vec::new());
    let rect = find(&nodes(&out), label)
        .unwrap_or_else(|| panic!("missing {label}"))
        .rect;
    draw(ctx, pointer(rect.center(), true));
    draw(ctx, pointer(rect.center(), false));
}

#[test]
fn materials_and_maxwell_share_the_persisted_object_style() {
    let mut state = State::new(true);
    assert!(state.set_field_mode(FieldMode::Materials));
    let ctx = context();
    let mut material = MaterialPanelState::default();
    material.sync_from(&state);

    click(
        &ctx,
        |ctx, events| material_frame(ctx, &mut state, &mut material, events),
        "Wireframe",
    );
    assert_eq!(state.object_style(), ObjectStyle::Wireframe);

    // Sources keeps its own widget but reads the same committed state.
    assert!(state.set_field_mode(FieldMode::Sources));
    let mut sources = SourcesPanelState::default();
    let out = sources_frame(&ctx, &mut state, &mut sources, Vec::new());
    assert_eq!(
        find(&nodes(&out), "Wireframe").and_then(|node| node.toggled),
        Some(true)
    );
    assert_eq!(
        find(&nodes(&out), "Solid").and_then(|node| node.toggled),
        Some(false)
    );

    assert!(state.set_field_mode(FieldMode::Maxwell));
    let mut maxwell = MaxwellPanelState::default();
    maxwell.sync_from(&state);
    let out = maxwell_frame(&ctx, &mut state, &mut maxwell, Vec::new());
    let texts = painted_texts(&out);
    assert!(texts.iter().any(|text| text.contains("Object style")));
    assert!(find(&nodes(&out), "Object style").is_some());

    click(
        &ctx,
        |ctx, events| maxwell_frame(ctx, &mut state, &mut maxwell, events),
        "Solid",
    );
    assert_eq!(state.object_style(), ObjectStyle::Solid);
}

#[test]
fn maxwell_panel_explains_the_outline_only_square_conductor() {
    let ctx = context();
    let mut state = State::new(true);
    assert!(state.set_field_mode(FieldMode::Maxwell));
    assert!(state.set_maxwell_scene(MaxwellScene {
        case: MaxwellCase::Conductor2D,
        outline: ConductorOutline::Square { half_extent: 1.0 },
        ..MaxwellScene::default()
    }));
    let mut panel = MaxwellPanelState::default();
    panel.sync_from(&state);

    let out = maxwell_frame(&ctx, &mut state, &mut panel, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts
            .iter()
            .any(|text| text.contains("no faithful box primitive")),
        "{texts:?}"
    );
}
