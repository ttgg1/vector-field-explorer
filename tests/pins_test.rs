//! Issue #232: pin/unpin/reorder controls and readouts, keep one shared
//! physical state, save named arrangements as a workspace preference, and keep
//! a pinned flux visible while editing its radius in compact layout.

use eframe::egui;

use crate::support;
use crate::support::storage::{MemoryStorage, from_ron, ron_string};

use vector_field_explorer::VfApp;
use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene};
use vector_field_explorer::persistence::{AppSettings, ViewRecord, load, save};
use vector_field_explorer::pin_layout::{
    MAX_PIN_ARRANGEMENTS, MAX_PIN_NAME_BYTES, PinItem, PinSet, PinUiRecord, sanitize_name,
};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::layout::SheetState;
use vector_field_explorer::ui::pins::{self, PinSnapshot, PinsState};
use vector_field_explorer::viewmodel::State;

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

struct WidgetNode {
    role: egui::Role,
    label: String,
    rect: egui::Rect,
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
                role: node.role(),
                label,
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn button<'a>(nodes: &'a [WidgetNode], label: &str) -> Option<&'a WidgetNode> {
    nodes
        .iter()
        .find(|node| node.role == egui::Role::Button && node.label == label)
}

fn frame(
    ctx: &egui::Context,
    app: &mut VfApp,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn settled(ctx: &egui::Context, app: &mut VfApp, width: f32) {
    for _ in 0..4 {
        let _ = frame(ctx, app, width, Vec::new());
    }
}

fn click_events(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
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

fn hollow() -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::HollowSphere,
        radius: 1.0,
        e0: 1.0,
        ..MaxwellScene::default()
    }
}

/// An app in the Maxwell hollow-sphere investigation with two pins active and
/// the compact Controls sheet open.
fn compact_app() -> VfApp {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Maxwell);
    assert!(app.state.set_maxwell_scene(hollow()));
    app.maxwell.sync_from(&app.state);
    app.maxwell.gauss.activate(&app.state.maxwell);
    app.maxwell.gauss.set_probe_radius(1.6);
    assert!(app.pins.record.pin(PinItem::GaussRadius));
    assert!(app.pins.record.pin(PinItem::GaussFlux));
    app.sheet = SheetState::Controls;
    app
}

#[test]
fn pin_unpin_and_reorder_keep_order_and_reject_duplicates() {
    let mut record = PinUiRecord::default();
    assert!(record.pin(PinItem::GaussFlux));
    assert!(record.pin(PinItem::GaussRadius));
    assert_eq!(
        record.active.items,
        vec![PinItem::GaussFlux, PinItem::GaussRadius]
    );
    assert!(!record.pin(PinItem::GaussFlux), "a duplicate is refused");
    assert!(record.move_up(1));
    assert_eq!(
        record.active.items,
        vec![PinItem::GaussRadius, PinItem::GaussFlux]
    );
    assert!(record.move_down(0));
    assert_eq!(
        record.active.items,
        vec![PinItem::GaussFlux, PinItem::GaussRadius]
    );
    assert!(!record.move_up(0), "the first item cannot move up");
    assert!(!record.move_down(1), "the last item cannot move down");
    assert!(record.unpin(PinItem::GaussFlux));
    assert_eq!(record.active.items, vec![PinItem::GaussRadius]);

    let repaired = PinSet {
        items: vec![PinItem::GaussFlux, PinItem::GaussFlux, PinItem::GaussRadius],
    }
    .sanitized();
    assert_eq!(
        repaired.items,
        vec![PinItem::GaussFlux, PinItem::GaussRadius],
        "sanitizing keeps the first occurrence in order"
    );
}

#[test]
fn named_arrangements_round_trip_and_bounded_names_are_repaired() {
    let mut record = PinUiRecord::default();
    record.pin(PinItem::GaussRadius);
    record.pin(PinItem::GaussFlux);
    assert_eq!(
        record.save_arrangement("  Lab flux check  "),
        Ok("Lab flux check".to_owned())
    );
    record.unpin(PinItem::GaussFlux);
    assert_eq!(record.active.items, vec![PinItem::GaussRadius]);
    assert!(record.restore_arrangement("Lab flux check"));
    assert_eq!(
        record.active.items,
        vec![PinItem::GaussRadius, PinItem::GaussFlux]
    );
    // Replacing the same name keeps one entry.
    assert_eq!(
        record.save_arrangement("Lab flux check"),
        Ok("Lab flux check".to_owned())
    );
    assert_eq!(record.saved.len(), 1);
    assert!(record.delete_arrangement("Lab flux check"));
    assert!(!record.restore_arrangement("Lab flux check"));

    assert_eq!(
        record.save_arrangement("   "),
        Err("Enter a name for the pin arrangement.".to_owned())
    );
    let long = "x".repeat(MAX_PIN_NAME_BYTES * 2);
    let sanitized = sanitize_name(&format!("  {long}  "));
    assert_eq!(sanitized.len(), MAX_PIN_NAME_BYTES);
    assert!(sanitized.is_char_boundary(sanitized.len()));

    let mut full = PinUiRecord::default();
    for index in 0..MAX_PIN_ARRANGEMENTS {
        full.save_arrangement(&format!("set {index}")).unwrap();
    }
    assert!(full.save_arrangement("one too many").is_err());
}

#[test]
fn invalid_and_missing_pin_entries_decode_without_corrupting_the_envelope() {
    let record: PinUiRecord = from_ron(
        "(active:(items:[\"gauss_flux\",\"not_a_pin\",\"gauss_flux\"]),\
         saved:[(name:\"  Dup  \",items:[\"gauss_radius\"]),\
         (name:\"Dup\",items:[\"gauss_flux\"]),(name:\"\",items:[\"gauss_flux\"])])",
    );
    let record = record.sanitized();
    assert_eq!(record.active.items, vec![PinItem::GaussFlux]);
    assert_eq!(record.saved.len(), 1);
    assert_eq!(record.saved[0].name, "Dup");
    assert_eq!(record.saved[0].items, vec![PinItem::GaussRadius]);

    let fallback: PinUiRecord = from_ron("(active:(items:\"nope\"),saved:7)");
    assert_eq!(fallback, PinUiRecord::default());

    // An older envelope without the whole record still loads defaults.
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &AppSettings::default());
    let raw = storage.values.get(eframe::APP_KEY).cloned().unwrap();
    let without_pins = raw.replace("pin_ui:", "ignored_pin_ui:");
    let mut old = MemoryStorage::default();
    old.values.insert(eframe::APP_KEY.to_owned(), without_pins);
    assert_eq!(load(Some(&old)).pin_ui, PinUiRecord::default());
}

#[test]
fn named_preference_round_trips_and_restores_the_same_model() {
    let mut state = State::new(false);
    state.set_field_mode(FieldMode::Maxwell);
    assert!(state.set_maxwell_scene(hollow()));
    let mut record = PinUiRecord::default();
    record.pin(PinItem::GaussRadius);
    record.pin(PinItem::GaussFlux);
    record.save_arrangement("Lab flux check").unwrap();

    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        pin_ui: record.clone(),
        ..Default::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(loaded.pin_ui, record, "the named arrangement round-trips");

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        app.state.maxwell, state.maxwell,
        "restoring pin preferences never replaces the physical model"
    );
    assert_eq!(app.pins.record, record);
    // The preference deliberately carries no model state.
    let raw = ron_string(&record);
    assert!(!raw.contains("MaxwellScene"));
    assert!(!raw.contains("outer_radius"));
    assert!(!raw.contains("probe_surfaces"));
}

#[test]
fn layout_switch_and_sheet_restore_preserve_the_model() {
    let ctx = context();
    let mut app = compact_app();
    let model = app.state.maxwell.clone();
    let passes = app.state.probe_surfaces().len();

    settled(&ctx, &mut app, 1280.0);
    assert_eq!(app.state.maxwell, model, "wide layout preserves the model");
    settled(&ctx, &mut app, 600.0);
    assert_eq!(
        app.state.maxwell, model,
        "compact layout preserves the model"
    );
    app.sheet = SheetState::Maxwell;
    settled(&ctx, &mut app, 600.0);
    assert_eq!(app.state.maxwell, model);
    assert_eq!(app.state.probe_surfaces().len(), passes);
    app.sheet = SheetState::Controls;
    settled(&ctx, &mut app, 600.0);
    assert_eq!(
        app.state.maxwell, model,
        "open sheet restore preserves the model"
    );
}

#[test]
fn compact_radius_edit_keeps_the_pinned_flux_visible_and_shared() {
    let ctx = context();
    let mut app = compact_app();
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts.iter().any(|text| text.contains("Gauss flux")),
        "the pinned flux is visible in compact layout: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.contains("Phi =")),
        "the pinned flux shows a live report: {texts:?}"
    );
    let before = app.maxwell.gauss.probe_radius;
    let flux_before = texts
        .iter()
        .find(|text| text.contains("Phi ="))
        .cloned()
        .unwrap();

    // The pinned control uses the same setter as the original control.
    assert!(app.maxwell.gauss.set_probe_radius(before + 0.4));
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts.iter().any(|text| text.contains("Gauss flux")),
        "the pinned flux stays visible after the radius edit"
    );
    let flux_after = texts
        .iter()
        .find(|text| text.contains("Phi ="))
        .cloned()
        .expect("the flux readout is still painted");
    assert_ne!(
        flux_before, flux_after,
        "the pinned readout follows the shared radius state"
    );
}

#[test]
fn pinned_items_explain_unavailability_and_never_show_stale_values() {
    let ctx = context();
    let mut state = PinsState::new(PinUiRecord::default());
    state.record.pin(PinItem::GaussFlux);
    let snapshot = PinSnapshot {
        gauss_flux: Some("Phi = 123.000000 stale".to_owned()),
        gauss_unavailable: Some(
            "The Gauss-shell radius and flux are available only in the Maxwell \
             hollow-sphere model."
                .to_owned(),
        ),
        ..PinSnapshot::default()
    };
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 600.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = pins::show(ui, &mut state, &snapshot, true);
            });
        },
    );
    output.textures_delta.clear();
    let texts = painted_texts(&output);
    assert!(
        texts
            .iter()
            .any(|text| text.contains("Unavailable in the active model")),
        "the tray explains the unavailable pin: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("available only in the Maxwell")),
        "the explanation names the active-model limit: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("123.000000")),
        "an unavailable pin never shows a stale value: {texts:?}"
    );
}

#[test]
fn tray_pointer_and_keyboard_paths_pin_the_same_items() {
    // Pointer: click the "Pin flux" button in the compact sheet.
    let ctx = context();
    let mut app = compact_app();
    app.pins.record.unpin(PinItem::GaussFlux);
    let _ = frame(&ctx, &mut app, 600.0, Vec::new());
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let nodes = nodes(&out);
    let target = button(&nodes, "Pin flux").unwrap_or_else(|| {
        panic!(
            "the pin button is painted: {nodes_labels:?}",
            nodes_labels = nodes.iter().map(|n| n.label.clone()).collect::<Vec<_>>()
        )
    });
    let _ = frame(&ctx, &mut app, 600.0, click_events(target.rect.center()));
    let _ = frame(&ctx, &mut app, 600.0, Vec::new());
    assert!(
        app.pins.record.active.contains(PinItem::GaussFlux),
        "the pointer path pins the flux"
    );

    // Keyboard: in the tray itself, Tab reaches the pin buttons and Enter
    // activates the focused one.
    let mut state = PinsState::new(PinUiRecord::default());
    state.record.pin(PinItem::GaussRadius);
    let snapshot = PinSnapshot {
        gauss_radius: Some(1.6),
        gauss_flux: Some("Phi = 12.5".to_owned()),
        ..PinSnapshot::default()
    };
    tray_pass(&ctx, &mut state, &snapshot, Vec::new());
    for _ in 0..2 {
        tray_pass(
            &ctx,
            &mut state,
            &snapshot,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    tray_pass(
        &ctx,
        &mut state,
        &snapshot,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(
        state.record.active.contains(PinItem::GaussFlux),
        "Tab then Enter activates the focused pin button"
    );
}

fn tray_pass(
    ctx: &egui::Context,
    state: &mut PinsState,
    snapshot: &PinSnapshot,
    events: Vec<egui::Event>,
) {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = pins::show(ui, state, snapshot, true);
            });
        },
    );
    output.textures_delta.clear();
}
