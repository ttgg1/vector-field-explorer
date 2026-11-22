//! Issue #273: in-context help and glossary for the prompt interpreter.
//!
//! The table is the single source of strings for the glossary panel. It only
//! names vocabulary the shipped offline grammar accepts; every accepted example
//! is checked against [`crate::prompt_model::parse_prompt`] in the tests and
//! every rejected example is parsed into an explicit limitation. The panel
//! itself is pure UI over [`crate::ui::prompt_model::PromptModelState`].

use eframe::egui;

use crate::prompt_model::{FORMULA_PATH, FRAME_NOTE, UNITS_NOTE};

/// The control that opens the glossary beside the section heading.
pub const TOGGLE_LABEL: &str = "Help / Glossary";
/// The heading inside the open glossary.
pub const TITLE: &str = "Prompt help & glossary";
/// The explicit dismiss control inside the open glossary.
pub const CLOSE_LABEL: &str = "Close";

/// Whether one accepted example builds a point charge or a dipole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExampleKind {
    PointCharge,
    Dipole,
}

/// One supported term and the explanation shown for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlossaryEntry {
    pub term: &'static str,
    pub detail: &'static str,
}

/// One titled group of glossary terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlossarySection {
    pub title: &'static str,
    pub intro: &'static str,
    pub entries: &'static [GlossaryEntry],
}

/// One prompt the glossary advertises as accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedExample {
    pub prompt: &'static str,
    pub meaning: &'static str,
    pub kind: ExampleKind,
}

/// One prompt the glossary advertises as unsupported, with the reason it states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RejectedExample {
    pub prompt: &'static str,
    /// The shared limitation prefix the shipped parser uses.
    pub reason: &'static str,
}

/// Distribution nouns the grammar explicitly rejects instead of guessing.
pub const UNSUPPORTED_DISTRIBUTION_TERMS: &[&str] =
    &["sphere", "cube", "disc", "ring", "line", "sheet"];

/// The complete glossary, in reading order.
pub const GLOSSARY_SECTIONS: &[GlossarySection] = &[
    GlossarySection {
        title: "Accepted prompts",
        intro: "This offline grammar understands one point charge at the origin or an equal and \
                opposite dipole along x, y or z. Everything else is unsupported and points to the \
                formula path.",
        entries: &[
            GlossaryEntry {
                term: "Point charge at the origin",
                detail: "Write a positive or negative point charge at the origin, for example \
                         `a positive point charge at the origin`. A plain `point charge at the \
                         origin` is positive. The charge must sit at the origin; a charge placed \
                         elsewhere is outside this grammar.",
            },
            GlossaryEntry {
                term: "Point-charge magnitude",
                detail: "An unnamed magnitude stays the symbolic `q` at the normalized teaching \
                         default 1. Add a positive number with `q = 2`, `charge = 2` or \
                         `magnitude = 2`.",
            },
            GlossaryEntry {
                term: "Dipole pair",
                detail: "Write two equal and opposite point charges, a pair, or a dipole, for \
                         example `two equal and opposite charges separated by d along x`. The \
                         pair is centered on the origin, with the charges at plus and minus half \
                         the separation on the chosen axis.",
            },
            GlossaryEntry {
                term: "Dipole axis",
                detail: "Name the axis as x, y or z using `x axis`, `x-axis`, `along x`, \
                         `on the x`, `x direction`, `x=...`, `+x` or `-x`. If no axis appears, \
                         the preview asks for one before Apply is available.",
            },
            GlossaryEntry {
                term: "Dipole separation",
                detail: "Write `separated by d`, `separation of d`, `separation d`, `distance d`, \
                         a positive number such as 2, or `d/2`. An unnamed separation becomes `d` \
                         with normalized value 1, and the preview's Separation control edits it.",
            },
            GlossaryEntry {
                term: "Source signs and placements",
                detail: "Signs are preserved, not normalized away: a negative charge stays \
                         negative. Explicit placements such as `positive charge at +x, negative \
                         charge at -x, separated by d` pin both the axis and which end is \
                         positive; swapping the signs reverses the model.",
            },
            GlossaryEntry {
                term: "Positive end",
                detail: "If the prompt does not say which end of the dipole is positive, answer \
                         the preview's positive-end question with +x or -x (or +y/-y, +z/-z). \
                         Apply stays unavailable until the question is answered.",
            },
        ],
    },
    GlossarySection {
        title: "Preview, frame and units",
        intro: "The preview is an editable, idealized description; nothing is applied until you \
                confirm it.",
        entries: &[
            GlossaryEntry {
                term: "Ambiguity questions",
                detail: "An unnamed axis asks x, y or z; an unnamed positive end asks +axis or \
                         -axis. These are questions about an otherwise supported model, not parse \
                         errors, and Apply stays disabled until each one is answered.",
            },
            GlossaryEntry {
                term: "Frame",
                detail: FRAME_NOTE,
            },
            GlossaryEntry {
                term: "Normalized teaching units",
                detail: UNITS_NOTE,
            },
            GlossaryEntry {
                term: "Requested quantities",
                detail: "The words field, potential, force and flux mark quantities to measure \
                         after applying. The preview labels them but does not solve them.",
            },
            GlossaryEntry {
                term: "Ideal equations",
                detail: "The preview shows the ideal analytic E(r) and V(r) for the interpreted \
                         sources. The 1/r² field has no finite value at a charge, so the sampled \
                         peak is softened.",
            },
            GlossaryEntry {
                term: "Softened evaluation",
                detail: "The shipped source field is sampled with softening length 0.1 world \
                         units. The equations stay ideal, and a softened sample is never graded \
                         against the ideal analytic peak.",
            },
        ],
    },
    GlossarySection {
        title: "Limits and actions",
        intro: "Unsupported prompts explain the limit rather than guessing. Only Apply changes \
                the document.",
        entries: &[
            GlossaryEntry {
                term: "Unsupported distributions",
                detail: "Spheres, cubes, discs, rings, lines, sheets and other distributed \
                         charges are outside this offline grammar. The preview states the \
                         limitation instead of guessing, and Apply stays disabled.",
            },
            GlossaryEntry {
                term: "Formula path",
                detail: FORMULA_PATH,
            },
            GlossaryEntry {
                term: "Apply",
                detail: "Apply commits the preview through the ordinary source-scene setters, so \
                         the result is one normal document edit with undo. It builds the \
                         previewed point sources; it runs no solver.",
            },
            GlossaryEntry {
                term: "Cancel",
                detail: "Cancel clears the typed prompt and the interpretation. It never changes \
                         the scene, source list, plot or undo history.",
            },
            GlossaryEntry {
                term: "This help",
                detail: "Opening or closing this help changes no prompt, interpretation, error, \
                         message or document. Escape and Close both dismiss it.",
            },
        ],
    },
];

/// Prompt shapes the glossary presents as accepted.
pub const ACCEPTED_EXAMPLES: &[AcceptedExample] = &[
    AcceptedExample {
        prompt: "a positive point charge at the origin",
        meaning: "one positive point charge at the origin",
        kind: ExampleKind::PointCharge,
    },
    AcceptedExample {
        prompt: "a negative point charge at the origin",
        meaning: "one negative point charge at the origin",
        kind: ExampleKind::PointCharge,
    },
    AcceptedExample {
        prompt: "a positive point charge at the origin with q = 2",
        meaning: "one positive point charge with magnitude 2",
        kind: ExampleKind::PointCharge,
    },
    AcceptedExample {
        prompt: "two equal and opposite charges separated by d along x",
        meaning: "a dipole on x that asks which end is positive",
        kind: ExampleKind::Dipole,
    },
    AcceptedExample {
        prompt: "positive charge at +x, negative charge at -x, separated by d",
        meaning: "a dipole on x with the positive end pinned at +x",
        kind: ExampleKind::Dipole,
    },
    AcceptedExample {
        prompt: "two equal and opposite charges separated by 2 along y",
        meaning: "a dipole on y with a separation of 2",
        kind: ExampleKind::Dipole,
    },
    AcceptedExample {
        prompt: "two equal and opposite charges separated by d",
        meaning: "a dipole that asks for the axis and the positive end",
        kind: ExampleKind::Dipole,
    },
];

/// Prompt shapes the glossary presents as unsupported, with their stated reason.
pub const REJECTED_EXAMPLES: &[RejectedExample] = &[
    RejectedExample {
        prompt: "a uniformly charged sphere at the origin",
        reason: "Unsupported charge distribution",
    },
    RejectedExample {
        prompt: "a uniformly charged cube at the origin",
        reason: "Unsupported charge distribution",
    },
    RejectedExample {
        prompt: "a disc of charge at the origin",
        reason: "Unsupported charge distribution",
    },
    RejectedExample {
        prompt: "a charged ring at the origin",
        reason: "Unsupported charge distribution",
    },
    RejectedExample {
        prompt: "a finite line charge at the origin",
        reason: "Unsupported charge distribution",
    },
    RejectedExample {
        prompt: "a sheet of charge at the origin",
        reason: "Unsupported charge distribution",
    },
    RejectedExample {
        prompt: "a point charge at x = 2",
        reason: "Unsupported charge distribution",
    },
];

/// Draw the open glossary. Returns whether the explicit Close button was used.
pub fn show(ui: &mut egui::Ui) -> bool {
    let mut close = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_max_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.heading(TITLE);
            if ui.button(CLOSE_LABEL).clicked() {
                close = true;
            }
        });
        ui.add(
            egui::Label::new(
                "This help is session-only. Escape or Close dismisses it without changing the \
                 typed prompt or the interpretation.",
            )
            .wrap(),
        );
        for section in GLOSSARY_SECTIONS {
            ui.add_space(4.0);
            ui.strong(section.title);
            ui.add(egui::Label::new(section.intro).wrap());
            for entry in section.entries {
                ui.add(egui::Label::new(egui::RichText::new(entry.term).strong()).wrap());
                ui.add(egui::Label::new(entry.detail).wrap());
            }
        }
        ui.add_space(4.0);
        ui.separator();
        ui.strong("Accepted examples");
        for example in ACCEPTED_EXAMPLES {
            ui.add(egui::Label::new(egui::RichText::new(example.prompt).monospace()).wrap());
            ui.add(egui::Label::new(example.meaning).wrap());
        }
        ui.add_space(4.0);
        ui.strong("Rejected examples");
        for example in REJECTED_EXAMPLES {
            ui.add(egui::Label::new(egui::RichText::new(example.prompt).monospace()).wrap());
            ui.add(egui::Label::new(example.reason).wrap());
        }
    });
    close
}
