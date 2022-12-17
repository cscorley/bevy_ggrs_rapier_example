use crate::prelude::*;

/// Controls whether our opponent will inject random inputs while inactive.
/// This is useful for testing rollbacks locally and can be toggled off with `r`
/// and `t`.
#[derive(Default, Reflect, Hash, Resource, PartialEq, Eq)]
#[reflect(Hash, Resource, PartialEq)]
pub struct RandomInput {
    pub on: bool,
}

/// Non-game input.  Just chucking this into the stack carelessly.
pub fn keyboard_input(mut commands: Commands, keys: Res<Input<KeyCode>>) {
    if keys.just_pressed(KeyCode::R) {
        commands.insert_resource(RandomInput { on: true });
    }
    if keys.just_pressed(KeyCode::T) {
        commands.insert_resource(RandomInput { on: false });
    }
}
