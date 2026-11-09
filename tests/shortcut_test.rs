// Ticket 60: the pure S/P seed shortcuts and the pointer-or-centre seed point.
// The mapping is plain-data only: feature gating lives in `click_action_plan`
// and the app applies the returned action through the click seam.

use eframe::egui;
use vector_field_explorer::view_command::{seed_shortcut, shortcut_seed_point};
use vector_field_explorer::viewmodel::LeftClickAction;

fn none() -> egui::Modifiers {
    egui::Modifiers::NONE
}

#[test]
fn plain_s_and_p_map_to_the_two_actions() {
    assert_eq!(
        seed_shortcut(egui::Key::S, none(), false, false, false),
        Some(LeftClickAction::SeedStreamline),
        "S seeds a streamline",
    );
    assert_eq!(
        seed_shortcut(egui::Key::P, none(), false, false, false),
        Some(LeftClickAction::AddParticles),
        "P adds a particle burst",
    );
}

#[test]
fn other_keys_are_not_seed_shortcuts() {
    for key in [
        egui::Key::A,
        egui::Key::C,
        egui::Key::R,
        egui::Key::F,
        egui::Key::Space,
        egui::Key::Escape,
    ] {
        assert_eq!(
            seed_shortcut(key, none(), false, false, false),
            None,
            "{key:?} is not a seed shortcut",
        );
    }
}

#[test]
fn text_edit_focus_suppresses_both_shortcuts() {
    for key in [egui::Key::S, egui::Key::P] {
        assert_eq!(
            seed_shortcut(key, none(), false, true, false),
            None,
            "{key:?} belongs to the formula bar while typing",
        );
    }
}

#[test]
fn s_is_ignored_while_flying_but_p_still_bursts() {
    assert_eq!(
        seed_shortcut(egui::Key::S, none(), false, false, true),
        None,
        "S is fly's backward key",
    );
    assert_eq!(
        seed_shortcut(egui::Key::P, none(), false, false, true),
        Some(LeftClickAction::AddParticles),
        "P is unused by fly and still bursts",
    );
}

#[test]
fn repeats_are_ignored() {
    for key in [egui::Key::S, egui::Key::P] {
        assert_eq!(
            seed_shortcut(key, none(), true, false, false),
            None,
            "{key:?} fires once per physical press",
        );
    }
}

#[test]
fn only_plain_modifiers_fire() {
    let chords = [
        none().plus(egui::Modifiers::CTRL),
        none().plus(egui::Modifiers::COMMAND),
        none().plus(egui::Modifiers::ALT),
        none().plus(egui::Modifiers::SHIFT),
        none().plus(egui::Modifiers::COMMAND).plus(egui::Modifiers::SHIFT),
    ];
    for modifiers in chords {
        for key in [egui::Key::S, egui::Key::P] {
            assert_eq!(
                seed_shortcut(key, modifiers, false, false, false),
                None,
                "{key:?} with {modifiers:?} must not seed",
            );
        }
    }
}

#[test]
fn shortcut_seed_point_uses_the_pointer_inside_the_viewport() {
    let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(110.0, 120.0));
    let pointer = egui::pos2(50.0, 70.0);
    assert_eq!(
        shortcut_seed_point(Some(pointer), rect),
        pointer,
        "a pointer over the viewport is the seed point",
    );
}

#[test]
fn shortcut_seed_point_falls_back_to_the_centre_outside_or_absent() {
    let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(110.0, 120.0));
    let centre = rect.center();
    assert_eq!(
        shortcut_seed_point(Some(egui::pos2(500.0, 70.0)), rect),
        centre,
        "a pointer outside the viewport falls back to its centre",
    );
    assert_eq!(
        shortcut_seed_point(None, rect),
        centre,
        "no pointer at all (captured/locked) also falls back to the centre",
    );
}
