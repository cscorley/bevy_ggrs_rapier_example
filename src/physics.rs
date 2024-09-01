use bevy_ggrs::RollbackFrameCount;

use crate::prelude::*;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Resource, Hash, Reflect)]
#[reflect(Hash, Resource, PartialEq)]
pub struct EnablePhysicsAfter {
    pub start: Frame,
    pub end: Frame,
}

impl Default for EnablePhysicsAfter {
    fn default() -> Self {
        Self::with_default_offset(0)
    }
}

impl EnablePhysicsAfter {
    pub fn new(start: Frame, end: Frame) -> Self {
        log::info!("Enabling after {:?},{:?}", start, end);
        Self { start, end }
    }

    pub fn with_default_offset(offset: Frame) -> Self {
        Self::new(offset, offset + (FPS * LOAD_SECONDS) as i32)
    }

    pub fn update_after_default(&mut self, offset: Frame) {
        let old_start = self.start;
        let old_end = self.end;
        self.start = offset;
        self.end = offset + (FPS * LOAD_SECONDS) as i32;
        log::info!(
            "Updated enable after ({:?}, {:?}) -> ({:?}, {:?})",
            old_start,
            old_end,
            self.start,
            self.end
        );
    }

    pub fn is_enabled(&self, frame: Frame) -> bool {
        // Since the starting frame is calculated at the end,
        // when we rollback to the start frame we will have the enable after
        // resource of that frame it was created as a result of, which is wrong.
        // assume that 1 frame is actually good and should not be ignored
        !(self.start < frame && frame < self.end)
    }
}

pub fn pause_physics_test(
    mut enable_physics_after: ResMut<EnablePhysicsAfter>,
    current_frame: Res<RollbackFrameCount>,
) {
    let current_frame: i32 = (*current_frame).into();

    if current_frame % (FPS as i32 * 10) == 0 {
        // Disable physics every few seconds to test physics pausing and resuming
        enable_physics_after.update_after_default(current_frame);
        log::info!(
            "Physics on frame {:?} {:?}",
            current_frame,
            enable_physics_after
        );
    }
}

pub fn toggle_physics(
    enable_physics_after: Res<EnablePhysicsAfter>,
    current_frame: Res<RollbackFrameCount>,
    mut time: ResMut<Time<Physics>>,
) {
    let current_frame: i32 = (*current_frame).into();
    let is_active = !time.is_paused();
    log::info!(
        "Physics on frame {:?} {:?} {:?}",
        current_frame,
        is_active,
        enable_physics_after
    );

    let should_activate = enable_physics_after.is_enabled(current_frame);
    if should_activate != is_active {
        log::info!(
            "Toggling physics on frame {:?}: {:?} -> {:?}",
            current_frame,
            is_active,
            should_activate
        );
    }

    if should_activate {
        time.unpause();
    } else {
        time.pause();
    }
}
