//! Pure guided-tour model coverage (#99 slice A).
//!
//! The tour table names only capabilities this build ships. These tests pin the
//! ordering, the semantic anchors, the progress bounds and the fact that the
//! model touches nothing except its own progress value.

use vector_field_explorer::tour::{
    AdvanceRule, ControlId, PanelId, Region, TOUR, TourAnchor, TourProgress, TourStop, TourTarget,
    resolve,
};

fn assert_real_target(stop: &TourStop) {
    match stop.target {
        TourTarget::Control(control) => assert!(
            ControlId::ALL.contains(&control),
            "stop {:?} targets a control missing from ControlId::ALL",
            stop.id
        ),
        TourTarget::Panel(panel) => assert!(
            PanelId::ALL.contains(&panel),
            "stop {:?} targets a panel missing from PanelId::ALL",
            stop.id
        ),
        TourTarget::Viewport(region) => assert!(
            Region::ALL.contains(&region),
            "stop {:?} targets a region missing from Region::ALL",
            stop.id
        ),
        TourTarget::Maximize => {}
    }
}

#[test]
fn tour_table_has_unique_non_empty_ids_and_text() {
    assert!(
        (10..=14).contains(&TOUR.len()),
        "the design bounds the tour to 10-14 stops, found {}",
        TOUR.len()
    );

    for (index, stop) in TOUR.iter().enumerate() {
        assert!(!stop.id.is_empty(), "stop {index} has an empty id");
        assert!(!stop.title.is_empty(), "stop {index} has an empty title");
        assert!(!stop.body.is_empty(), "stop {index} has an empty body");
        assert!(
            !TOUR[..index].iter().any(|earlier| earlier.id == stop.id),
            "duplicate stop id {:?}",
            stop.id
        );
    }
}

#[test]
fn every_stop_targets_an_existing_capability() {
    for stop in TOUR {
        assert_real_target(stop);
    }
}

#[test]
fn every_control_and_panel_resolves_through_the_anchor_descriptor() {
    for panel in PanelId::ALL {
        assert_eq!(
            resolve(TourTarget::Panel(*panel)),
            TourAnchor::Panel(*panel)
        );
    }
    for region in Region::ALL {
        assert_eq!(
            resolve(TourTarget::Viewport(*region)),
            TourAnchor::Viewport(*region)
        );
    }
    assert_eq!(resolve(TourTarget::Maximize), TourAnchor::Maximize);

    for control in ControlId::ALL {
        let panel = control.panel();
        assert!(
            PanelId::ALL.contains(&panel),
            "control {:?} maps to panel {:?} missing from PanelId::ALL",
            control,
            panel
        );
        assert_eq!(
            resolve(TourTarget::Control(*control)),
            TourAnchor::Control {
                panel,
                control: *control
            }
        );
    }
}

#[test]
fn changed_rules_name_a_real_control() {
    for stop in TOUR {
        if let AdvanceRule::Changed(name) = stop.advance {
            assert!(
                ControlId::ALL.iter().any(|control| control.key() == name),
                "stop {:?} waits on {name:?}, which is not a ControlId key",
                stop.id
            );
        }
    }
}

#[test]
fn advance_and_back_preserve_index_bounds() {
    let mut progress = TourProgress::new();
    assert_eq!(progress.index(), 0);
    assert!(!progress.is_finished());

    progress.back();
    assert_eq!(progress.index(), 0, "back at the first stop must clamp");

    for _ in 0..TOUR.len() {
        progress.advance();
    }
    assert!(progress.is_finished());
    assert_eq!(progress.index(), TOUR.len());

    progress.advance();
    assert_eq!(
        progress.index(),
        TOUR.len(),
        "advance at the end must clamp"
    );

    progress.back();
    assert_eq!(
        progress.index(),
        TOUR.len(),
        "back must be a no-op once the tour is finished"
    );
    assert!(progress.is_finished());
}

#[test]
fn back_after_dismiss_keeps_the_tour_finished() {
    let mut progress = TourProgress::new();
    progress.advance();
    progress.dismiss();
    assert!(progress.is_finished());

    progress.back();
    assert!(
        progress.is_finished(),
        "dismissal ends the tour for the session; back must not reopen it"
    );
    assert_eq!(progress.index(), TOUR.len());
    assert_eq!(progress.current(), None);
}

#[test]
fn dismiss_finishes_and_restart_returns_to_the_start() {
    let mut progress = TourProgress::new();
    progress.advance();
    progress.mark_opened(PanelId::Presets);
    progress.dismiss();
    assert!(progress.is_finished());
    assert_eq!(progress.index(), TOUR.len());

    progress.restart();
    assert_eq!(progress.index(), 0);
    assert!(!progress.is_finished());
    assert!(progress.opened_panels().is_empty());
}

#[test]
fn opened_panels_are_recorded_once_per_panel() {
    let mut progress = TourProgress::new();
    assert!(progress.mark_opened(PanelId::Presets));
    assert!(!progress.mark_opened(PanelId::Presets));
    assert!(progress.mark_opened(PanelId::Sources));
    assert_eq!(
        progress.opened_panels(),
        &[PanelId::Presets, PanelId::Sources]
    );

    progress.restart();
    assert!(progress.opened_panels().is_empty());
}

#[test]
fn current_stop_follows_the_index_and_never_panics_after_finish() {
    let mut progress = TourProgress::new();
    for expected in TOUR {
        assert_eq!(progress.current().map(|stop| stop.id), Some(expected.id));
        progress.advance();
    }
    assert_eq!(progress.current(), None);
}

#[test]
fn maxwell_stop_waits_for_a_case_change() {
    let stop = TOUR
        .iter()
        .find(|stop| stop.id == "maxwell")
        .expect("the tour keeps its Maxwell case stop");
    assert_eq!(
        stop.advance,
        AdvanceRule::Changed("maxwell_case"),
        "the Maxwell stop instructs the user to choose a case, so it must wait on that change"
    );
}

#[test]
fn flux_stop_anchors_to_the_integral_surface_control() {
    let stop = TOUR
        .iter()
        .find(|stop| stop.id == "probes")
        .expect("the tour keeps its Field probes stop");
    assert_eq!(
        stop.target,
        TourTarget::Control(ControlId::FieldProbeSurface),
        "the flux step points at the real integral-surface control, not scalar point placement"
    );
    assert_ne!(
        stop.target,
        TourTarget::Control(ControlId::FieldProbePlace),
        "scalar point placement is not an integral probe control"
    );
    assert_eq!(ControlId::FieldProbeSurface.panel(), PanelId::Controls);
    assert!(
        ControlId::FieldProbeSurface.requires_live_rect(),
        "an absent integral control yields no anchor rather than the whole panel"
    );
}

#[test]
fn full_tour_run_opens_each_target_panel_exactly_once() {
    let mut progress = TourProgress::new();
    let mut expected: Vec<PanelId> = Vec::new();

    for stop in TOUR {
        assert_real_target(stop);
        assert_eq!(
            progress.current().map(|current| current.id),
            Some(stop.id),
            "the walk follows the table order"
        );

        let panel = match stop.target {
            TourTarget::Control(control) => Some(control.panel()),
            TourTarget::Panel(panel) => Some(panel),
            TourTarget::Viewport(_) | TourTarget::Maximize => None,
        };
        if let Some(panel) = panel {
            progress.mark_opened(panel);
            if !expected.contains(&panel) {
                expected.push(panel);
            }
        }

        progress.advance();
    }

    assert!(progress.is_finished());
    assert_eq!(progress.current(), None);
    assert_eq!(
        progress.opened_panels(),
        expected.as_slice(),
        "opened panels must be the target panels in first-opened order"
    );
    for panel in progress.opened_panels() {
        assert_eq!(
            progress
                .opened_panels()
                .iter()
                .filter(|opened| *opened == panel)
                .count(),
            1,
            "panel {panel:?} must be recorded exactly once"
        );
    }
}
