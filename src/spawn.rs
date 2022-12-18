use crate::prelude::*;

/// A marker component for spawning first thing when the app launches.  This
/// just contains some arbitrary data, it actually isn't critical (it's used to
/// sort, but we could also use [`Entity`])
#[derive(Component)]
pub struct DeterministicSpawn {
    pub index: usize,
}

#[derive(Bundle)]
pub struct DeterministicSpawnBundle {
    pub spawn: DeterministicSpawn,
    pub name: Name,
}

impl DeterministicSpawnBundle {
    pub fn new(index: usize) -> Self {
        Self {
            spawn: DeterministicSpawn { index },
            name: Name::new(format!("Deterministic Spawn {}", index)),
        }
    }
}
