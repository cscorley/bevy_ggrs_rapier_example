use bevy_ggrs::RollbackFrameCount;

use crate::prelude::*;

/// Our physics rollback state container, which will be rolled back and we will
/// use to restore our physics state.
#[derive(Default, Reflect, Clone, Hash, Resource, PartialEq, Eq)]
#[reflect(Hash, Resource, PartialEq)]
pub struct PhysicsRollbackState {
    pub rapier_state: Vec<u8>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Resource, Hash, Reflect)]
#[reflect(Hash)]
pub struct PhysicsEnabled(pub bool);

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
    mut physics_enabled: ResMut<PhysicsEnabled>,
    mut config: ResMut<RapierConfiguration>,
) {
    let current_frame: i32 = (*current_frame).into();
    log::info!(
        "Physics on frame {:?} {:?} {:?}",
        current_frame,
        physics_enabled.0,
        enable_physics_after
    );

    let should_activate = enable_physics_after.is_enabled(current_frame);
    if physics_enabled.0 != should_activate {
        log::info!(
            "Toggling physics on frame {:?}: {:?} -> {:?}",
            current_frame,
            physics_enabled.0,
            should_activate
        );
        physics_enabled.0 = should_activate;
    }

    config.physics_pipeline_active = physics_enabled.0;
}

pub fn rollback_rapier_context(
    rollback_status: Res<RollbackStatus>,
    game_state: Res<PhysicsRollbackState>,
    mut rapier: ResMut<RapierContext>,
) {
    // Only restore our state if we are in a rollback.  This step is *critical*.
    // Only doing this during rollbacks saves us a step every frame.  Here, we
    // also do not allow rollback to frame 0.  Physics state is already correct
    // in this case.  This prevents lagged clients from getting immediate desync
    // and is entirely a hack since we don't enable physics until later anyway.
    //
    // You can also test that desync detection is working by "disabling" this
    // system by uncommenting this line:
    // return;

    if rollback_status.is_rollback && rollback_status.rollback_frame > 1 {
        // Serialize our physics state for hashing, to display the state in-flight.
        // This should not be necessary for this demo to work, as we will do the
        // real checksum during `save_game_state` at the end of the pipeline.
        // TODO:  Remove this for your real game.  It is unnecessary work!
        log::info!(
            "Context expected hash before rollback: {:?}",
            game_state.rapier_state.reflect_hash()
        );

        if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
            log::info!(
                "Context hash before rollback: {:?}",
                context_bytes.reflect_hash()
            );
        }

        if let Ok(context) = bincode::deserialize::<RapierContext>(game_state.rapier_state.as_ref())
        {
            // commands.insert_resource(context);
            // *rapier = context;

            // Inserting or replacing directly seems to screw up some of the
            // crate-only properties.  So, we'll copy over each public
            // property instead.
            rapier.bodies = context.bodies;
            rapier.broad_phase = context.broad_phase;
            rapier.ccd_solver = context.ccd_solver;
            rapier.colliders = context.colliders;
            rapier.impulse_joints = context.impulse_joints;
            rapier.integration_parameters = context.integration_parameters;
            rapier.islands = context.islands;
            rapier.multibody_joints = context.multibody_joints;
            rapier.narrow_phase = context.narrow_phase;
            rapier.query_pipeline = context.query_pipeline;

            // pipeline is not serialized
            // rapier.pipeline = context.pipeline;
        }

        // Again, not necessary for the demo, just to show the rollback changes
        // as they occur.
        // TODO:  Remove this for your real game.  It is unnecessary work!
        if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
            log::info!(
                "Context hash after rollback: {:?}",
                context_bytes.reflect_hash()
            );
        }
    }
}

pub fn save_rapier_context(
    mut game_state: ResMut<PhysicsRollbackState>,
    rapier: Res<RapierContext>,
) {
    // This serializes our context every frame.  It's not great, but works to
    // integrate the two plugins.  To do less of it, we would need to change
    // bevy_ggrs to serialize arbitrary structs like this one in addition to
    // component tracking.  If you need this to happen less, I'd recommend not
    // using the plugin and implementing GGRS yourself.
    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        log::info!(
            "Context hash before save: {:?}",
            game_state.rapier_state.reflect_hash()
        );

        game_state.rapier_state = context_bytes;

        log::info!(
            "Context hash after save: {:?}",
            game_state.rapier_state.reflect_hash()
        );
    }
}
