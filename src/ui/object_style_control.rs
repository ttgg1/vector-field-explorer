//! The shared Solid/Wireframe selector for Sources, Induction, Materials and
//! Maxwell.
//!
//! Every panel reads and writes the one persisted
//! [`crate::viewmodel::State::object_style`]; this module owns the widget text
//! and layout so the panels cannot drift. The same durable value also covers
//! Induction, which uses the Sources glyph path.

use eframe::egui;

use crate::object_scene::ObjectStyle;
use crate::viewmodel::State;

/// Stable explanation shown under the selector in every panel that embeds it.
///
/// For the bounded Materials and Maxwell cases, Wireframe draws the mapped body
/// edges in addition to the existing case outline; that outline is never
/// removed.
pub const OBJECT_STYLE_HELP: &str = "Solid fills the mapped bodies; Wireframe draws their \
     edges. The same choice applies to Sources, Induction, Materials and Maxwell. \
     SVG exports keep the line blueprint.";

/// Draw the Solid/Wireframe selector against the committed shared state.
///
/// Returns whether the click changed the persisted style.
pub fn show(ui: &mut egui::Ui, state: &mut State) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label("Object style");
        for (style, label) in [
            (ObjectStyle::Solid, "Solid"),
            (ObjectStyle::Wireframe, "Wireframe"),
        ] {
            if ui
                .selectable_label(state.object_style() == style, label)
                .clicked()
                && state.object_style() != style
            {
                changed |= state.set_object_style(style);
            }
        }
    });
    ui.label(OBJECT_STYLE_HELP);
    changed
}
