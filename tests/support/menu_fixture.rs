//! Shared mutable fixture for menu actions (ticket 15/16a).
//!
//! Menu tests build a [`MenuFixture`] once, mutate the records they care
//! about, and borrow [`MenuFixture::ui`] into
//! [`vector_field_explorer::ui::menu::apply_action`]. Keeping it here stops
//! `tests/menu_test.rs` and `tests/download_link_test.rs` from carrying two
//! copies of the same seven-field record.

use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::fly_camera::FlyCamera;
use vector_field_explorer::render::Camera;
use vector_field_explorer::sim_clock::SimClock;
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::menu::{MenuAction, SideEffect, UiState, apply_action};
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

/// The mutable state a menu action may touch.
pub struct MenuFixture {
    pub layout: LayoutState,
    pub orbit: OrbitCamera,
    pub fly: Option<FlyCamera>,
    pub camera: Camera,
    pub state: State,
    pub controls: Controls,
    pub clock: SimClock,
}

impl MenuFixture {
    pub fn new() -> Self {
        Self {
            layout: LayoutState::default(),
            orbit: OrbitCamera::default(),
            fly: None,
            camera: Camera::default(),
            state: State::new(false),
            controls: Controls::new(),
            clock: SimClock::default(),
        }
    }

    pub fn ui(&mut self) -> UiState<'_> {
        UiState {
            layout: &mut self.layout,
            orbit: &mut self.orbit,
            fly: &mut self.fly,
            camera: &mut self.camera,
            state: &mut self.state,
            controls: &mut self.controls,
            clock: &mut self.clock,
        }
    }
}

/// Apply one action to a fresh fixture and report the side effect.
pub fn effect_of(action: MenuAction) -> SideEffect {
    let mut fixture = MenuFixture::new();
    apply_action(action, &mut fixture.ui())
}
