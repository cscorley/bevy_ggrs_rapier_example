use bevy::utils::HashMap;
use bevy_ggrs::{LocalInputs, LocalPlayers};
use bevy_matchbox::prelude::PeerId;

use crate::prelude::*;

// These are just 16 bit for bit-packing alignment in the input struct
const INPUT_UP: u16 = 0b00001;
const INPUT_DOWN: u16 = 0b00010;
const INPUT_LEFT: u16 = 0b00100;
const INPUT_RIGHT: u16 = 0b01000;

/// GGRS player handle, we use this to associate GGRS handles back to our [`Entity`]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Component)]
pub struct Player {
    pub handle: usize,
}

/// The main GGRS configuration type
pub type ExampleGgrsConfig = bevy_ggrs::GgrsConfig<GGRSInput, PeerId>;

/// Our primary data struct; what players send to one another
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Pod, Zeroable)]
pub struct GGRSInput {
    // The input from our player
    pub input: u16,
}

pub fn input(
    mut commands: Commands,
    local_players: Res<LocalPlayers>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut random: ResMut<RandomInput>,
    physics_enabled: Res<PhysicsEnabled>,
) {
    let mut local_inputs = HashMap::new();

    for handle in &local_players.0 {
        let mut input: u16 = 0;

        // Do not do anything until physics are live
        if physics_enabled.0 {
            // Build the input
            if keyboard_input.pressed(KeyCode::KeyW) {
                input |= INPUT_UP;
            }
            if keyboard_input.pressed(KeyCode::KeyA) {
                input |= INPUT_LEFT;
            }
            if keyboard_input.pressed(KeyCode::KeyS) {
                input |= INPUT_DOWN;
            }
            if keyboard_input.pressed(KeyCode::KeyD) {
                input |= INPUT_RIGHT;
            }

            // toggle off random input if our local moves at all
            if input != 0 && random.on {
                random.on = false;
            } else if input == 0 && random.on {
                let mut rng = thread_rng();
                // Return a random input sometimes, or maybe nothing.
                // Helps to trigger input-based rollbacks from the unplayed side
                match rng.gen_range(0..10) {
                    0 => input = INPUT_UP,
                    1 => input = INPUT_LEFT,
                    2 => input = INPUT_DOWN,
                    3 => input = INPUT_RIGHT,
                    _ => (),
                }
            }
        }

        local_inputs.insert(*handle, GGRSInput { input });
    }

    commands.insert_resource(LocalInputs::<ExampleGgrsConfig>(local_inputs));
}

pub fn apply_inputs(
    mut query: Query<(&mut Velocity, &Player)>,
    inputs: Res<PlayerInputs<ExampleGgrsConfig>>,
    physics_enabled: Res<PhysicsEnabled>,
) {
    for (mut v, p) in query.iter_mut() {
        let (game_input, input_status) = inputs[p.handle];
        let input = match input_status {
            InputStatus::Confirmed => game_input.input,
            InputStatus::Predicted => game_input.input,
            InputStatus::Disconnected => 0, // disconnected players do nothing
        };

        if input > 0 {
            // Useful for desync observing
            log::info!("input {:?} from {}: {}", input_status, p.handle, input)
        }

        // Do not do anything until physics are live
        // This is a poor mans emulation to stop accidentally tripping velocity updates
        if !physics_enabled.0 {
            continue;
        }

        let right = input & INPUT_RIGHT != 0;
        let left = input & INPUT_LEFT != 0;
        let up = input & INPUT_UP != 0;
        let down = input & INPUT_DOWN != 0;

        let direction_right = right && !left;
        let direction_left = left && !right;
        let direction_up = up && !down;
        let direction_down = down && !up;

        let horizontal = if direction_left {
            -1.
        } else if direction_right {
            1.
        } else {
            0.
        };

        let vertical = if direction_down {
            -1.
        } else if direction_up {
            1.
        } else {
            0.
        };

        let new_vel_x = if horizontal != 0. {
            v.linvel.x + horizontal * 10.0
        } else {
            0.
        };

        let new_vel_y = if vertical != 0. {
            v.linvel.y + vertical * 10.0
        } else {
            0.
        };

        // This is annoying but we have to make sure we only trigger an update in Rapier when explicitly necessary!
        if new_vel_x != v.linvel.x || new_vel_y != v.linvel.y {
            v.linvel.x = new_vel_x;
            v.linvel.y = new_vel_y;
        }
    }
}

pub fn force_update_rollbackables(
    mut t_query: Query<&mut Transform, With<Rollback>>,
    mut v_query: Query<&mut Velocity, With<Rollback>>,
) {
    for mut t in t_query.iter_mut() {
        t.set_changed();
    }
    for mut v in v_query.iter_mut() {
        v.set_changed();
    }
}
