use bevy::prelude::*;
use ggrs::Frame;

use crate::{frames::ValidatableFrame, DESYNC_MAX_FRAMES};

/// Metadata we need to store about frames we've rendered locally
#[derive(Default, Hash, Resource, PartialEq, Eq, Debug)]
pub struct FrameHash {
    /// The frame number for this metadata
    pub frame: Frame,

    /// The checksum of the Rapier physics state for the frame.  I use this term interchangably with `hash`, sorry.
    pub rapier_checksum: u16,

    /// Has been confirmed by GGRS
    pub confirmed: bool,

    /// Has been sent by us to other players
    pub sent: bool,

    /// Has been validated by us against other player
    pub validated: bool,
}

/// Metadata we need to store about frames we've received from other player
#[derive(Default, Hash, Resource, PartialEq, Eq, Debug)]
pub struct RxFrameHash {
    /// The frame number for this metadata
    pub frame: Frame,

    /// The checksum of the Rapier physics state for the frame.  I use this term interchangably with `hash`, sorry.
    pub rapier_checksum: u16,

    /// Has been validated by us against other player
    pub validated: bool,
}

// A collection of confirmed frame hashes we've seen locally
#[derive(Default, Hash, Resource, PartialEq, Eq)]
pub struct FrameHashes(pub [FrameHash; DESYNC_MAX_FRAMES]);

// A collection of confirmed frame hashes we've received from our other player
// This only works for 1v1.  This would have to be extended to consider all
// remotes in larger scenarios (I accept pull requests!)
#[derive(Default, Hash, Resource, PartialEq, Eq)]
pub struct RxFrameHashes(pub [RxFrameHash; DESYNC_MAX_FRAMES]);

/// Our desync detector!
/// Validates the hashes we've received so far against the ones we've calculated ourselves.
/// If there is a difference, panic.  Your game will probably want to handle this more gracefully.
pub fn frame_validator(
    mut hashes: ResMut<FrameHashes>,
    mut rx_hashes: ResMut<RxFrameHashes>,
    validatable_frame: Res<ValidatableFrame>,
) {
    for (i, rx) in rx_hashes.0.iter_mut().enumerate() {
        // Check every confirmed frame that has not been validated
        if rx.frame > 0 && !rx.validated {
            // Get that same frame in our buffer
            if let Some(sx) = hashes.0.get_mut(i) {
                // Make sure it's the exact same frame and also confirmed and not yet validated
                // and importantly is SAFE to validate
                if sx.frame == rx.frame
                    && sx.confirmed
                    && !sx.validated
                    && validatable_frame.is_validatable(sx.frame)
                {
                    // If this is causing your game to exit, you have a bug!
                    assert_eq!(
                        sx.rapier_checksum, rx.rapier_checksum,
                        "Failed checksum checks {:?} != {:?}",
                        sx, rx
                    );
                    // Set both as validated
                    log::info!("Frame validated {:?}", sx.frame);
                    sx.validated = true;
                    rx.validated = true;
                }
            }
        }
    }
}
