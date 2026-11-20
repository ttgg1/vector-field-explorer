//! Issue #84 slice A1: the pure layer-style model and its State seam.
//!
//! Neutral defaults must reproduce today's drawing exactly; an override may
//! set opacity, width and fixed colours independently, and resolution runs
//! global -> family -> layer per attribute. Validation rejects values rather
//! than clamping, while `sanitized` repairs a stored record. The settings
//! envelope carries the record without affecting any renderer path.

use serde::Serialize;

use crate::support::storage::MemoryStorage;
use vector_field_explorer::VfApp;
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::layer_style::{
    LayerId, LayerOverride, LayerStyles, MAX_WIDTH_MULTIPLIER, MIN_WIDTH_MULTIPLIER, PhysicsFamily,
};
use vector_field_explorer::parameters::ParameterSet;
use vector_field_explorer::persistence::{AppSettings, PresetUiRecord, ViewRecord, load, save};
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::style_panel::AppStyle;
use vector_field_explorer::viewmodel::State;
use vector_field_explorer::viewport_style::ViewportStyle;

fn layer_slot(layer: LayerId) -> usize {
    LayerId::ALL
        .iter()
        .position(|candidate| *candidate == layer)
        .expect("every layer is listed in LayerId::ALL")
}

fn family_slot(family: PhysicsFamily) -> usize {
    PhysicsFamily::ALL
        .iter()
        .position(|candidate| *candidate == family)
        .expect("every family is listed in PhysicsFamily::ALL")
}

#[test]
fn neutral_styles_resolve_to_drawable_defaults() {
    let styles = LayerStyles::default();
    assert!(styles.is_neutral());
    assert!(styles.layers.iter().all(LayerOverride::is_neutral));
    assert!(styles.families.iter().all(LayerOverride::is_neutral));

    for layer in LayerId::ALL {
        for family in [
            None,
            Some(PhysicsFamily::Electric),
            Some(PhysicsFamily::Magnetic),
        ] {
            let resolved = styles.resolve(layer, family);
            assert_eq!(resolved.opacity, 1.0, "{layer:?}/{family:?} opacity");
            assert_eq!(resolved.width, 1.0, "{layer:?}/{family:?} width");
            assert_eq!(resolved.color, None, "{layer:?}/{family:?} colour");
        }
    }
}

#[test]
fn layer_override_sets_only_the_fields_it_names() {
    let mut styles = LayerStyles::default();
    styles.layers[layer_slot(LayerId::Arrows)].opacity = Some(0.35);
    assert!(!styles.is_neutral());

    let resolved = styles.resolve(LayerId::Arrows, None);
    assert_eq!(resolved.opacity, 0.35);
    assert_eq!(resolved.width, 1.0, "an unset width stays neutral");
    assert_eq!(resolved.color, None, "an unset colour stays neutral");

    let flow = styles.resolve(LayerId::Flow, None);
    assert_eq!(flow.opacity, 1.0, "another layer is untouched");
    assert_eq!(flow.width, 1.0);
}

#[test]
fn family_override_applies_to_family_aware_layers() {
    let mut styles = LayerStyles::default();
    styles.families[family_slot(PhysicsFamily::Electric)].opacity = Some(0.4);

    for layer in [LayerId::Sources, LayerId::Induction, LayerId::TeachingScene] {
        assert_eq!(
            styles.resolve(layer, Some(PhysicsFamily::Electric)).opacity,
            0.4,
            "{layer:?} honours its family"
        );
        assert_eq!(
            styles.resolve(layer, None).opacity,
            1.0,
            "a caller with no family ignores the family record"
        );
    }
    assert_eq!(
        styles
            .resolve(LayerId::Sources, Some(PhysicsFamily::Magnetic))
            .opacity,
        1.0,
        "the other family is independent"
    );
}

#[test]
fn layer_beats_family_per_attribute() {
    let mut styles = LayerStyles::default();
    let electric = family_slot(PhysicsFamily::Electric);
    styles.families[electric].opacity = Some(0.25);
    styles.families[electric].width = Some(2.0);
    let sources = layer_slot(LayerId::Sources);
    styles.layers[sources].width = Some(0.75);
    styles.layers[sources].color = Some(Rgb([12, 34, 56]));

    let resolved = styles.resolve(LayerId::Sources, Some(PhysicsFamily::Electric));
    assert_eq!(resolved.opacity, 0.25, "opacity falls back to the family");
    assert_eq!(resolved.width, 0.75, "the layer width is more specific");
    assert_eq!(
        resolved.color,
        Some(Rgb([12, 34, 56])),
        "the layer colour is more specific"
    );
}

#[test]
fn validation_rejects_non_finite_and_out_of_range_values() {
    let arrows = layer_slot(LayerId::Arrows);
    for opacity in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.01, -0.01] {
        let mut styles = LayerStyles::default();
        styles.layers[arrows].opacity = Some(opacity);
        let error = styles.validate().expect_err("opacity must be rejected");
        assert!(
            error.contains(&format!("layers[{arrows}].opacity")),
            "the error names the opacity path: {error}"
        );
    }

    for width in [f32::NAN, f32::INFINITY, 0.04, 4.01] {
        let mut styles = LayerStyles::default();
        styles.layers[arrows].width = Some(width);
        let error = styles.validate().expect_err("width must be rejected");
        assert!(
            error.contains(&format!("layers[{arrows}].width")),
            "the error names the width path: {error}"
        );
    }

    let magnetic = family_slot(PhysicsFamily::Magnetic);
    let mut styles = LayerStyles::default();
    styles.families[magnetic].width = Some(0.0);
    let error = styles.validate().expect_err("family width is rejected");
    assert!(
        error.contains(&format!("families[{magnetic}].width")),
        "the error names the family width path: {error}"
    );

    let mut boundaries = LayerStyles::default();
    boundaries.layers[arrows].opacity = Some(0.0);
    boundaries.layers[arrows].width = Some(MIN_WIDTH_MULTIPLIER);
    boundaries.families[family_slot(PhysicsFamily::Electric)].opacity = Some(1.0);
    boundaries.families[family_slot(PhysicsFamily::Electric)].width = Some(MAX_WIDTH_MULTIPLIER);
    assert!(boundaries.validate().is_ok(), "the bounds are inclusive");
}

#[test]
fn sanitized_repairs_a_stored_record() {
    let mut styles = LayerStyles::default();
    let arrows = layer_slot(LayerId::Arrows);
    let flow = layer_slot(LayerId::Flow);
    let magnetic = family_slot(PhysicsFamily::Magnetic);
    styles.layers[arrows] = LayerOverride {
        opacity: Some(f32::NAN),
        width: Some(10.0),
        color: Some(Rgb([1, 2, 3])),
    };
    styles.layers[flow].opacity = Some(2.0);
    styles.families[magnetic].width = Some(f32::INFINITY);

    let repaired = styles.sanitized();
    assert_eq!(
        repaired.layers[arrows].opacity, None,
        "a non-finite value becomes unset"
    );
    assert_eq!(
        repaired.layers[arrows].width,
        Some(MAX_WIDTH_MULTIPLIER),
        "a finite width clamps into range"
    );
    assert_eq!(
        repaired.layers[flow].opacity,
        Some(1.0),
        "a finite opacity clamps into range"
    );
    assert_eq!(
        repaired.families[magnetic].width, None,
        "a non-finite family value becomes unset"
    );
    assert_eq!(
        repaired.layers[arrows].color,
        Some(Rgb([1, 2, 3])),
        "colours need no repair"
    );
    assert!(repaired.validate().is_ok(), "the repaired record is valid");
}

#[test]
fn layer_styles_round_trip_through_settings() {
    let mut styles = LayerStyles::default();
    styles.layers[layer_slot(LayerId::TeachingScene)] = LayerOverride {
        opacity: Some(0.5),
        width: Some(1.5),
        color: Some(Rgb([7, 8, 9])),
    };
    styles.families[family_slot(PhysicsFamily::Electric)].width = Some(0.5);
    let settings = AppSettings {
        layer_styles: styles,
        ..AppSettings::default()
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(loaded.layer_styles, styles);
}

#[test]
fn older_settings_load_neutral_styles() {
    #[derive(Serialize)]
    struct OlderEnvelope {
        style: AppStyle,
        layout: LayoutState,
        view: ViewRecord,
        parameters: ParameterSet,
        viewport_style: ViewportStyle,
        preset_ui: PresetUiRecord,
    }

    let current = AppSettings::default();
    let older = OlderEnvelope {
        style: current.style.clone(),
        layout: current.layout.clone(),
        view: current.view.clone(),
        parameters: current.parameters.clone(),
        viewport_style: current.viewport_style,
        preset_ui: current.preset_ui.clone(),
    };
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &older);

    let loaded = load(Some(&storage));
    assert_eq!(loaded.layer_styles, LayerStyles::default());
    assert!(loaded.layer_styles.is_neutral());
}

#[test]
fn restored_layer_styles_reach_the_state_without_dirtying() {
    let mut styles = LayerStyles::default();
    styles.layers[layer_slot(LayerId::Reference)].opacity = Some(0.6);
    let settings = AppSettings {
        layer_styles: styles,
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let loaded = load(Some(&storage));
    assert_eq!(loaded.layer_styles, styles);

    // Startup assigns the loaded record directly; unlike a user commit, the
    // restore must not request a scene rebuild on its own.
    let mut state = State::new(false);
    state.dirty = false;
    state.layer_styles = loaded.layer_styles;
    assert!(
        !state.dirty,
        "assigning the restored layer styles leaves dirty alone"
    );

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        app.state.layer_styles, styles,
        "the app constructor restores the persisted layer styles"
    );
}

#[test]
fn commit_is_a_no_op_for_an_equal_set() {
    let mut state = State::new(false);
    let mut styles = LayerStyles::default();
    styles.layers[layer_slot(LayerId::Arrows)].opacity = Some(0.5);
    state
        .commit_layer_styles(styles)
        .expect("the initial style is valid");

    state.dirty = false;
    state
        .commit_layer_styles(styles)
        .expect("an equal set stays valid");
    assert!(!state.dirty, "an equal commit does not dirty the scene");
    assert_eq!(*state.layer_styles(), styles);
}

#[test]
fn invalid_commit_keeps_the_previous_styles() {
    let mut state = State::new(false);
    let mut previous = LayerStyles::default();
    previous.layers[layer_slot(LayerId::Arrows)].width = Some(0.5);
    state
        .commit_layer_styles(previous)
        .expect("the initial style is valid");

    state.dirty = false;
    let mut invalid = previous;
    invalid.layers[layer_slot(LayerId::Arrows)].opacity = Some(1.5);
    assert!(state.commit_layer_styles(invalid).is_err());

    assert_eq!(*state.layer_styles(), previous);
    let resolved = state.layer_styles().resolve(LayerId::Arrows, None);
    assert_eq!(resolved.width, 0.5, "the previous width survives");
    assert_eq!(resolved.opacity, 1.0, "the rejected opacity never lands");
    assert!(!state.dirty, "a rejected commit leaves dirty untouched");
}

#[test]
fn commit_marks_dirty_once_for_a_real_change() {
    let mut state = State::new(false);
    state.dirty = false;
    let mut styles = LayerStyles::default();
    styles.layers[layer_slot(LayerId::Flow)].opacity = Some(0.75);

    state
        .commit_layer_styles(styles)
        .expect("the style is valid");
    assert!(state.dirty, "a real change requests a rebuild");
    assert_eq!(*state.layer_styles(), styles);

    assert!(
        state.snapshot_if_dirty().is_some(),
        "the single dirty request rebuilds once"
    );
    assert!(!state.dirty);
    assert!(
        state.snapshot_if_dirty().is_none(),
        "no second rebuild is requested"
    );
}
