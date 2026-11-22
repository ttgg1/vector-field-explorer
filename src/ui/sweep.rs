//! Issue #234: the sweep controls and custom-painted curve.
//!
//! The panel is session-only and gated behind the lesson's reveal: it cannot
//! show a curve before the prediction step. The run advances one sample per
//! frame, a working Cancel keeps the labelled partial curve, gaps break the
//! line instead of becoming zero, and a clicked sample becomes an explicit
//! preview request that the lesson applies through its ordinary bounded probe
//! setter.

use eframe::egui;

use crate::maxwell::MaxwellScene;
use crate::sweep::{
    SWEEP_MAX_SAMPLES, SWEEP_MIN_SAMPLES, SweepConfig, SweepKind, SweepReport, SweepSample,
    SweepSession, SweepStatus, fixed_parameters_text,
};

/// Section heading shown only after the lesson is revealed.
pub const SWEEP_TITLE: &str = "Parameter sweep";
/// Start/restart the bounded run.
pub const RUN_LABEL: &str = "Run sweep";
/// Cancel pending work.
pub const CANCEL_LABEL: &str = "Cancel sweep";
/// Leave preview mode and restore the starting probe.
pub const EXIT_PREVIEW_LABEL: &str = "Exit preview";
/// Leave preview mode and keep the previewed probe.
pub const ADOPT_LABEL: &str = "Adopt sample";

/// One request from the sweep controls to the owning lesson.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SweepUiAction {
    /// Nothing to apply.
    None,
    /// Preview the sample at this independent-variable value.
    Preview(f64),
    /// End preview and restore this starting probe.
    ExitPreview(f64),
    /// End preview and keep the applied probe.
    Adopt,
}

/// The stable id of the clickable chart, used by tests and accessibility.
pub fn chart_id() -> egui::Id {
    egui::Id::unique("issue-234-sweep-chart")
}

/// The chart's paint/click rectangle inside its allocated canvas.
pub fn chart_plot_rect(rect: egui::Rect) -> egui::Rect {
    rect.shrink2(egui::vec2(38.0, 20.0))
}

/// The horizontal pixel for `x` in `config`.
pub fn sample_x_position(plot: egui::Rect, config: &SweepConfig, x: f64) -> f32 {
    let span = config.max - config.min;
    let fraction = if span > 0.0 {
        ((x - config.min) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    plot.left() + fraction as f32 * plot.width()
}

/// The nearest sample index within a small click/hover radius, limited to the
/// samples evaluated so far.
pub fn sample_index_at(
    plot: egui::Rect,
    config: &SweepConfig,
    evaluated: usize,
    pointer_x: f32,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for index in 0..evaluated.min(config.len()) {
        let distance = (sample_x_position(plot, config, config.x_at(index)) - pointer_x).abs();
        if best.is_none_or(|(_, closest)| distance < closest) {
            best = Some((index, distance));
        }
    }
    best.filter(|(_, distance)| *distance <= 14.0)
        .map(|(index, _)| index)
}

/// Draw the sweep window. Returns the one-shot request the lesson should apply.
pub fn show(
    ctx: &egui::Context,
    session: &mut SweepSession,
    scene: &MaxwellScene,
    revealed: bool,
    current_probe: f64,
) -> SweepUiAction {
    if !revealed {
        return SweepUiAction::None;
    }
    let content = ctx.content_rect();
    let width = 340.0_f32.min((content.width() - 24.0).max(280.0));
    let position = egui::pos2(
        (content.right() - width - 12.0).max(content.left() + 12.0),
        12.0,
    );
    let mut action = SweepUiAction::None;
    egui::Window::new(SWEEP_TITLE)
        .collapsible(false)
        .resizable(false)
        .default_pos(position)
        .default_width(width)
        .max_width(width)
        .show(ctx, |ui| {
            action = show_contents(ui, session, scene, current_probe);
        });
    action
}

fn show_contents(
    ui: &mut egui::Ui,
    session: &mut SweepSession,
    scene: &MaxwellScene,
    current_probe: f64,
) -> SweepUiAction {
    let mut action = SweepUiAction::None;

    // Bounded work: at most one sample per frame, so the window stays
    // responsive and Cancel is always reachable.
    if session.status() == SweepStatus::Sampling {
        session.step();
    }

    ui.label(format!(
        "Independent variable: {}",
        session.kind().independent_label()
    ));
    ui.horizontal_wrapped(|ui| {
        ui.label("From");
        ui.add(egui::DragValue::new(&mut session.min).speed(0.01));
        ui.label("to");
        ui.add(egui::DragValue::new(&mut session.max).speed(0.01));
        ui.label("samples");
        ui.add(
            egui::DragValue::new(&mut session.samples).range(SWEEP_MIN_SAMPLES..=SWEEP_MAX_SAMPLES),
        );
    });
    ui.label(format!(
        "Fixed parameters: {} (not swept)",
        fixed_parameters_text(session.kind(), scene)
    ));
    ui.small(session.kind().units_note());

    ui.horizontal_wrapped(|ui| {
        if ui
            .button(RUN_LABEL)
            .on_hover_text("Sample the existing lesson reading in bounded steps")
            .clicked()
            && session.start(scene).is_ok()
        {
            session.step();
        }
        let running = session.status() == SweepStatus::Sampling;
        if ui
            .add_enabled(running, egui::Button::new(CANCEL_LABEL))
            .on_hover_text("Stop pending samples; the partial curve stays labelled cancelled")
            .clicked()
        {
            session.cancel();
        }
    });
    let status = session.status();
    let status_text = format!("Sweep status: {}", session.status_text());
    match status {
        SweepStatus::Invalid => {
            ui.colored_label(ui.visuals().warn_fg_color, status_text);
        }
        SweepStatus::Cancelled => {
            ui.colored_label(ui.visuals().warn_fg_color, status_text);
        }
        _ => {
            ui.small(status_text);
        }
    }

    if let Some(config) = session.config_used() {
        if session.samples().is_empty() {
            ui.small("No samples yet; run the sweep to draw the curve.");
        } else if let Some(index) = draw_chart(ui, session, &config, current_probe) {
            let sample = session.samples()[index].clone();
            session.select(index);
            session.note_preview(sample.x, current_probe);
            action = SweepUiAction::Preview(sample.x);
        }
        if let Some(report) = session.report() {
            draw_report(ui, session.kind(), &report);
        }
    } else {
        ui.small("No samples yet; run the sweep to draw the curve.");
    }

    if session.is_previewing() {
        let symbol = session.kind().symbol();
        ui.separator();
        ui.label(format!(
            "Previewing {symbol} = {:.3}{}; starting probe = {:.3}",
            session.preview_target().unwrap_or(current_probe),
            preview_value_text(session),
            session.preview_start().unwrap_or(current_probe)
        ));
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(EXIT_PREVIEW_LABEL)
                .on_hover_text("Restore the probe position from before the first preview")
                .clicked()
            {
                let start = session.end_preview().unwrap_or(current_probe);
                action = SweepUiAction::ExitPreview(start);
            }
            if ui
                .button(ADOPT_LABEL)
                .on_hover_text("Keep this probe position and end preview mode")
                .clicked()
            {
                session.end_preview();
                action = SweepUiAction::Adopt;
            }
        });
    }

    action
}

/// Paint the curve, axes, gap markers, current reading and selection.
fn draw_chart(
    ui: &mut egui::Ui,
    session: &SweepSession,
    config: &SweepConfig,
    current_probe: f64,
) -> Option<usize> {
    let samples = session.samples();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), 160.0),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let response = ui.interact(rect, chart_id(), egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

    let plot = chart_plot_rect(rect);
    let (y_min, y_max) = value_bounds(samples);
    let to_point = |x: f64, value: f64| {
        let px = sample_x_position(plot, config, x);
        let span = y_max - y_min;
        let fraction = if span > 0.0 {
            ((value - y_min) / span).clamp(0.0, 1.0)
        } else {
            0.5
        };
        egui::pos2(px, plot.bottom() - fraction as f32 * plot.height())
    };

    // Axes and zero line.
    let axis_stroke = egui::Stroke::new(1.0, ui.visuals().weak_text_color());
    painter.line_segment(
        [
            egui::pos2(plot.left(), plot.bottom()),
            egui::pos2(plot.right(), plot.bottom()),
        ],
        axis_stroke,
    );
    painter.line_segment(
        [
            egui::pos2(plot.left(), plot.top()),
            egui::pos2(plot.left(), plot.bottom()),
        ],
        axis_stroke,
    );
    if y_min < 0.0 && y_max > 0.0 {
        let y = to_point(config.min, 0.0).y;
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        );
    }
    painter.text(
        egui::pos2(plot.left(), rect.top() + 2.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{} vs {}",
            config.kind.axis_y_label(),
            config.kind.axis_x_label()
        ),
        egui::FontId::proportional(11.0),
        ui.visuals().weak_text_color(),
    );
    painter.text(
        egui::pos2(plot.left(), plot.bottom() + 3.0),
        egui::Align2::LEFT_TOP,
        format!("{:.2}", config.min),
        egui::FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );
    painter.text(
        egui::pos2(plot.right(), plot.bottom() + 3.0),
        egui::Align2::RIGHT_TOP,
        format!("{:.2}", config.max),
        egui::FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );
    painter.text(
        egui::pos2(plot.right(), plot.top()),
        egui::Align2::RIGHT_TOP,
        format!("{y_max:.3}"),
        egui::FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );

    // The curve, broken at every gap.
    let curve_color = egui::Color32::from_rgb(120, 210, 255);
    let mut run: Vec<egui::Pos2> = Vec::new();
    let flush = |painter: &egui::Painter, run: &mut Vec<egui::Pos2>| {
        if run.len() >= 2 {
            painter.add(egui::Shape::line(
                std::mem::take(run),
                egui::Stroke::new(2.0, curve_color),
            ));
        } else if let [point] = run.as_slice() {
            painter.circle_filled(*point, 2.0, curve_color);
            run.clear();
        } else {
            run.clear();
        }
    };
    for sample in samples {
        match sample.value {
            Some(value) if value.is_finite() => run.push(to_point(sample.x, value)),
            _ => flush(&painter, &mut run),
        }
    }
    flush(&painter, &mut run);

    // Gap markers: a tick at the baseline, never a zero-valued point.
    let gap_color = ui.visuals().warn_fg_color;
    for sample in samples.iter().filter(|sample| sample.has_gap()) {
        let x = sample_x_position(plot, config, sample.x);
        painter.line_segment(
            [
                egui::pos2(x, plot.bottom() - 7.0),
                egui::pos2(x, plot.bottom()),
            ],
            egui::Stroke::new(1.5, gap_color),
        );
        painter.circle_stroke(
            egui::pos2(x, plot.bottom() - 11.0),
            3.0,
            egui::Stroke::new(1.0, gap_color),
        );
    }

    // Current reading marker.
    let probe_x = sample_x_position(plot, config, current_probe);
    painter.line_segment(
        [
            egui::pos2(probe_x, plot.top()),
            egui::pos2(probe_x, plot.bottom()),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 90, 90)),
    );
    painter.text(
        egui::pos2(probe_x + 3.0, plot.top() + 12.0),
        egui::Align2::LEFT_TOP,
        "probe",
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgb(255, 90, 90),
    );

    if let Some(index) = session.selected()
        && let Some(sample) = samples.get(index)
    {
        let point = match sample.value {
            Some(value) if value.is_finite() => to_point(sample.x, value),
            _ => egui::pos2(sample_x_position(plot, config, sample.x), plot.bottom()),
        };
        painter.circle_filled(point, 4.0, egui::Color32::from_rgb(255, 190, 80));
    }

    if let Some(pointer) = response.hover_pos()
        && let Some(index) = sample_index_at(plot, config, samples.len(), pointer.x)
    {
        response
            .clone()
            .on_hover_text(sample_tooltip(config.kind, &samples[index]));
    }

    let mut clicked = None;
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        clicked = sample_index_at(plot, config, samples.len(), pointer.x);
    }
    clicked
}

/// The selected sample's value or gap reason, shown beside the preview target.
fn preview_value_text(session: &SweepSession) -> String {
    let Some(sample) = session
        .selected()
        .and_then(|index| session.samples().get(index))
    else {
        return String::new();
    };
    match sample.value {
        Some(value) => format!(" ({} = {value:.6})", session.kind().value_symbol()),
        None => format!(
            " (gap: {})",
            sample.gap.as_deref().unwrap_or("no finite sample")
        ),
    }
}

/// The tooltip for one sample, including the honest gap reason.
fn sample_tooltip(kind: SweepKind, sample: &SweepSample) -> String {
    match sample.value {
        Some(value) => {
            let uncertainty = sample
                .uncertainty
                .map(|error| format!(" +/- {error:.2e} (quadrature)"))
                .unwrap_or_default();
            format!(
                "{} = {:.4}; {} = {:.6}{uncertainty}",
                kind.symbol(),
                sample.x,
                kind.value_symbol(),
                value
            )
        }
        None => format!(
            "{} = {:.4}; gap: {}",
            kind.symbol(),
            sample.x,
            sample.gap.as_deref().unwrap_or("no finite sample")
        ),
    }
}

fn value_bounds(samples: &[SweepSample]) -> (f64, f64) {
    let mut min: f64 = 0.0;
    let mut max: f64 = 0.0;
    let mut any = false;
    for sample in samples {
        if let Some(value) = sample.value.filter(|value| value.is_finite()) {
            min = if any { min.min(value) } else { value };
            max = if any { max.max(value) } else { value };
            any = true;
        }
    }
    min = min.min(0.0);
    max = max.max(0.0);
    if !any || (max - min).abs() < 1e-12 {
        let pad = max.abs().max(1.0) * 0.5;
        (min - pad, max + pad)
    } else {
        let pad = (max - min) * 0.08;
        (min - pad, max + pad)
    }
}

fn draw_report(ui: &mut egui::Ui, kind: SweepKind, report: &SweepReport) {
    ui.small(format!(
        "Samples evaluated: {}; Gaps: {} (drawn as breaks, never zero)",
        report.samples, report.gaps
    ));
    match kind {
        SweepKind::RingAxialField => match report.symmetry {
            Some(check) => {
                ui.small(format!(
                    "Odd symmetry: |E(z)+E(-z)| over {} pairs; max residual = {:.3e}; tolerance = {:.3e}; {}",
                    check.pairs,
                    check.max_residual,
                    check.tolerance,
                    if check.satisfied {
                        "satisfied"
                    } else {
                        "not satisfied"
                    }
                ));
            }
            None => {
                ui.small(
                    "Odd symmetry: needs a range centred on z = 0 with paired finite samples.",
                );
            }
        },
        SweepKind::ShellFlux => match report.flux_constancy {
            Some(check) => {
                ui.small(format!(
                    "Exterior flux: {} samples; spread = {:.3e}; combined quadrature uncertainty = {:.3e}; constant within uncertainty: {}",
                    check.samples,
                    check.spread,
                    check.combined_uncertainty,
                    if check.satisfied { "yes" } else { "no" }
                ));
            }
            None => {
                ui.small("Exterior flux: needs at least two finite exterior samples.");
            }
        },
    }
}
