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
use vector_field_explorer::ui::layout::{SheetSize, SheetState};
use vector_field_explorer::ui::pins::{self, PinAction, PinSnapshot, PinsState};
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

fn spin_button(nodes: &[WidgetNode]) -> Option<&WidgetNode> {
    nodes
        .iter()
        .find(|node| node.role == egui::Role::SpinButton)
}

/// Draw only the pin tray, returning its output and the action it emitted.
fn tray_frame(
    ctx: &egui::Context,
    state: &mut PinsState,
    snapshot: &PinSnapshot,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, Option<PinAction>) {
    let mut action = None;
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
                action = pins::show(ui, state, snapshot, true);
            });
        },
    );
    output.textures_delta.clear();
    (output, action)
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
fn pinned_radius_widget_action_applies_through_the_shared_setter() {
    let ctx = context();
    let mut app = compact_app();
    let before = app.maxwell.gauss.probe_radius;
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts.iter().any(|text| text.contains("Gauss flux")),
        "the pinned flux is visible in compact layout: {texts:?}"
    );
    let flux_before = texts
        .iter()
        .find(|text| text.contains("Phi ="))
        .cloned()
        .expect("the pinned flux shows a live report");

    // Drive the pinned radius widget itself, not the setter directly.
    let mut state = PinsState::new(PinUiRecord::default());
    state.record.pin(PinItem::GaussRadius);
    state.record.pin(PinItem::GaussFlux);
    let snapshot = PinSnapshot {
        gauss_radius: Some(before),
        gauss_flux: Some(flux_before.clone()),
        ..PinSnapshot::default()
    };
    let (out, _) = tray_frame(&ctx, &mut state, &snapshot, Vec::new());
    let rect = spin_button(&nodes(&out))
        .expect("the pinned radius is an editable spin button")
        .rect;
    let center = rect.center();
    let _ = tray_frame(
        &ctx,
        &mut state,
        &snapshot,
        vec![egui::Event::PointerMoved(center)],
    );
    let _ = tray_frame(
        &ctx,
        &mut state,
        &snapshot,
        vec![egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let mut action = None;
    for dx in [24.0, 48.0, 72.0, 96.0] {
        let (_, candidate) = tray_frame(
            &ctx,
            &mut state,
            &snapshot,
            vec![egui::Event::PointerMoved(center + egui::vec2(dx, 0.0))],
        );
        if candidate.is_some() {
            action = candidate;
            break;
        }
    }
    let (_, released) = tray_frame(
        &ctx,
        &mut state,
        &snapshot,
        vec![egui::Event::PointerButton {
            pos: center + egui::vec2(96.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let action = action
        .or(released)
        .expect("dragging the pinned radius emits a PinAction");
    let PinAction::SetGaussRadius(target) = action;
    assert!(
        target > before,
        "the drag increases the pinned radius: {before} -> {target}"
    );

    // Apply it through the ordinary app/model seam the tray uses.
    app.apply_pin_action(action);
    assert_eq!(
        app.maxwell.gauss.probe_radius, target,
        "the action reaches the shared Gauss state"
    );

    // The next frame formats the ordinary report from the changed state.
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts.iter().any(|text| text.contains("Gauss flux")),
        "the pinned flux stays visible after the widget edit"
    );
    let flux_after = texts
        .iter()
        .find(|text| text.contains("Phi ="))
        .cloned()
        .expect("the flux readout is still painted");
    assert_ne!(
        flux_before, flux_after,
        "the pinned readout follows the radius action applied through the app"
    );
}

#[test]
fn catalogue_has_stable_ids_and_one_editable_control() {
    let ids = PinItem::ALL.map(|item| item.id());
    assert_eq!(PinItem::ALL.len(), 7);
    for (index, item) in PinItem::ALL.into_iter().enumerate() {
        assert_eq!(PinItem::from_token(item.id()), Some(item));
        assert!(
            !ids[..index].contains(&item.id()),
            "every stored token is unique: {:?}",
            item.id()
        );
        assert!(!item.label().is_empty());
        assert!(!item.short_label().is_empty());
        assert_eq!(
            item.is_control(),
            item == PinItem::GaussRadius,
            "only the measuring radius is editable for now"
        );
    }
    assert_eq!(PinItem::from_token("not_a_pin"), None);
}

#[test]
fn readout_values_follow_shared_state_and_the_scale_lock_is_always_available() {
    let snapshot = PinSnapshot {
        gauss_radius: Some(1.6),
        gauss_shell_radius: Some(1.0),
        gauss_flux: Some("Phi = 1.0".to_owned()),
        scale_auto_fit: true,
        induction_frequency: Some(0.25),
        induction_time: Some(1.25),
        induction_orientation: Some(pins::induction_axis_text([0.0, 0.0, 1.0])),
        ..PinSnapshot::default()
    };
    assert_eq!(
        snapshot.value_text(PinItem::GaussShellRadius),
        Some("R = 1.000000".to_owned())
    );
    assert_eq!(
        snapshot.value_text(PinItem::GaussScaleLock),
        Some("Auto".to_owned())
    );
    assert_eq!(
        snapshot.value_text(PinItem::InductionTime),
        Some("t = 1.250 s".to_owned())
    );
    assert_eq!(
        snapshot.value_text(PinItem::InductionOrientation),
        Some("axis = (0.000, 0.000, 1.000)".to_owned())
    );
    assert!(snapshot.is_available(PinItem::GaussScaleLock));
    assert!(snapshot.is_available(PinItem::InductionTime));

    let locked = PinSnapshot {
        scale_auto_fit: false,
        ..snapshot
    };
    assert_eq!(
        locked.value_text(PinItem::GaussScaleLock),
        Some("Locked".to_owned())
    );
    assert!(
        locked.is_available(PinItem::GaussScaleLock),
        "the scale lock is available in every model"
    );
}

#[test]
fn gauss_shell_radius_readout_follows_the_committed_scene() {
    let ctx = context();
    let mut app = compact_app();
    app.pins.record.unpin(PinItem::GaussFlux);
    assert!(app.pins.record.pin(PinItem::GaussShellRadius));
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts.iter().any(|text| text.contains("R = 1.000000")),
        "the shell readout shows the committed radius: {texts:?}"
    );

    let mut scene = app.state.maxwell.clone();
    scene.radius = 1.2;
    assert!(app.state.set_maxwell_scene(scene));
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts.iter().any(|text| text.contains("R = 1.200000")),
        "editing the ordinary scene updates the pinned shell readout: {texts:?}"
    );
}

#[test]
fn pinned_readouts_show_the_active_model_and_hide_inactive_ones() {
    // Induction active: shared clock time and the committed axis are visible.
    let ctx = context();
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    app.clock.time = 1.25;
    app.clock.set_playing(false);
    app.state.set_time(1.25);
    app.state.auto_fit = false;
    for item in [
        PinItem::GaussScaleLock,
        PinItem::InductionFrequency,
        PinItem::InductionTime,
        PinItem::InductionOrientation,
    ] {
        assert!(app.pins.record.pin(item));
    }
    app.sheet = SheetState::Controls;
    app.sheet_expanded = SheetSize::Expanded;
    let out = frame(&ctx, &mut app, 600.0, Vec::new());
    let texts = painted_texts(&out);
    assert!(
        texts.iter().any(|text| text.contains("Locked")),
        "the scale readout follows State::auto_fit: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("f = 0.250000 Hz (normalized)")),
        "the induction frequency follows the committed scene: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.contains("t = 1.250 s")),
        "the induction time follows the shared clock: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("axis = (0.000, 0.000, 1.000)")),
        "the induction axis follows the committed scene: {texts:?}"
    );

    // Gauss model inactive: its readouts explain the limit instead of a stale value.
    let mut state = PinsState::new(PinUiRecord::default());
    for item in [PinItem::GaussShellRadius, PinItem::GaussFlux] {
        state.record.pin(item);
    }
    let snapshot = PinSnapshot {
        gauss_shell_radius: Some(9.9),
        gauss_flux: Some("Phi = 9.900000 stale".to_owned()),
        gauss_unavailable: Some(
            "The Gauss-shell radius and flux are available only in the Maxwell \
             hollow-sphere model."
                .to_owned(),
        ),
        ..PinSnapshot::default()
    };
    let (output, _) = tray_frame(&ctx, &mut state, &snapshot, Vec::new());
    let texts = painted_texts(&output);
    assert!(
        texts.iter().any(|text| text.contains("Gauss shell radius"))
            && texts
                .iter()
                .any(|text| text.contains("available only in the Maxwell")),
        "the shell readout explains its inactive model: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("9.900000")),
        "inactive readouts never show a stale value: {texts:?}"
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
