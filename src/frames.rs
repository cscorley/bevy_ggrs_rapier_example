use crate::prelude::*;

/// Left outside of the rollback system to detect rollbacks
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Resource, Hash, Reflect)]
#[reflect(Hash)]
pub struct LastFrame(pub Frame);

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Resource, Hash, Reflect)]
#[reflect(Hash)]
pub struct ConfirmedFrame(pub Frame);

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Resource, Hash, Reflect)]
#[reflect(Hash)]
pub struct CurrentSessionFrame(pub Frame);

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Resource, Hash, Reflect)]
#[reflect(Hash)]
pub struct CurrentFrame(pub Frame);

#[derive(Copy, Clone, PartialEq, Eq, Debug, Resource, Hash, Reflect)]
#[reflect(Hash)]
pub struct ValidatableFrame(pub Frame);

impl Default for ValidatableFrame {
    fn default() -> Self {
        Self(std::i32::MIN)
    }
}

impl ValidatableFrame {
    pub fn is_validatable(&self, frame: Frame) -> bool {
        frame < self.0
    }
}

/// Should not be rolled back... obviously?
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Resource, Hash, Reflect)]
#[reflect(Hash)]
pub struct RollbackStatus {
    pub is_rollback: bool,
    pub is_replay: bool,
    pub rollback_frame: Frame,
    pub last_frame: Frame,
}

pub fn update_confirmed_frame(
    mut confirmed_frame: ResMut<ConfirmedFrame>,
    current_frame: Res<CurrentFrame>,
    session: Option<Res<Session<ExampleGgrsConfig>>>,
) {
    if let Some(session) = session {
        match &*session {
            Session::SyncTest(_) => confirmed_frame.0 = current_frame.0,
            Session::P2P(s) => confirmed_frame.0 = s.confirmed_frame(),
            Session::Spectator(_) => confirmed_frame.0 = current_frame.0,
        }
    }

    log::info!("confirmed frame: {}", confirmed_frame.0);
}

pub fn update_current_frame(mut current_frame: ResMut<CurrentFrame>) {
    current_frame.0 += 1;
    log::info!("---- start frame {} ----", current_frame.0);
}

pub fn update_current_session_frame(
    mut current_session_frame: ResMut<CurrentSessionFrame>,
    current_frame: Res<CurrentFrame>,
    session: Option<Res<Session<ExampleGgrsConfig>>>,
) {
    if let Some(session) = session {
        match &*session {
            Session::SyncTest(_) => current_session_frame.0 = current_frame.0,
            Session::P2P(s) => current_session_frame.0 = s.current_frame(),
            Session::Spectator(_) => current_session_frame.0 = current_frame.0,
        }
    }

    log::info!("current session frame: {}", current_session_frame.0);
}

pub fn update_rollback_status(
    current_frame: Res<CurrentFrame>,
    current_session_frame: Res<CurrentSessionFrame>,
    mut rollback_status: ResMut<RollbackStatus>,
) {
    // If the last frame is greater than the current frame, we have rolled back.
    // Same for equals, because it means our frame did not update!
    rollback_status.is_rollback = rollback_status.last_frame >= current_frame.0;
    rollback_status.is_replay =
        rollback_status.is_rollback || current_session_frame.0 > current_frame.0;

    if rollback_status.is_rollback {
        rollback_status.rollback_frame = current_frame.0;
        log::info!(
            "rollback on {} to {}",
            rollback_status.last_frame,
            rollback_status.rollback_frame,
        );
    }

    if rollback_status.is_replay {
        log::info!(
            "replay on {} of {}",
            current_session_frame.0,
            current_frame.0
        );
    }

    // I know this seems silly at first glance, but after we know we've entered
    // a rollback once, we have to resimulate all frames back to where we left
    // off... and there may be additional rollbacks that happen during that!
    rollback_status.last_frame = current_frame.0;
}

pub fn update_validatable_frame(
    current_frame: Res<CurrentFrame>,
    current_session_frame: Res<CurrentSessionFrame>,
    confirmed_frame: Res<ConfirmedFrame>,
    mut validatable_frame: ResMut<ValidatableFrame>,
) {
    validatable_frame.0 = std::cmp::min(
        current_frame.0,
        std::cmp::min(current_session_frame.0, confirmed_frame.0),
    ) - (MAX_PREDICTION as i32);

    log::info!("validatable frame: {}", validatable_frame.0);
}
