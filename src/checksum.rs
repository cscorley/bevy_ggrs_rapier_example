use bevy::prelude::*;
use bevy_ggrs::Rollback;
use bevy_rapier2d::prelude::*;

use crate::*;

#[derive(Default, Reflect, Hash, Component)]
#[reflect(Hash)]
pub struct Checksum {
    value: u16,
}

pub fn checksum(
    mut query: Query<(&Transform, &Velocity, &mut Checksum), With<Rollback>>,
    mut frame_count: ResMut<FrameCount>,
    rapier: Res<RapierContext>,
) {
    for (t, v, mut checksum) in query.iter_mut() {
        let mut bytes = Vec::with_capacity(16 * 3 * 2);

        bytes.extend_from_slice(&t.translation.x.to_le_bytes());
        bytes.extend_from_slice(&t.translation.y.to_le_bytes());
        bytes.extend_from_slice(&t.translation.z.to_le_bytes());

        bytes.extend_from_slice(&t.rotation.w.to_le_bytes());
        bytes.extend_from_slice(&t.rotation.x.to_le_bytes());
        bytes.extend_from_slice(&t.rotation.y.to_le_bytes());
        bytes.extend_from_slice(&t.rotation.z.to_le_bytes());

        bytes.extend_from_slice(&t.scale.x.to_le_bytes());
        bytes.extend_from_slice(&t.scale.y.to_le_bytes());
        bytes.extend_from_slice(&t.scale.z.to_le_bytes());

        bytes.extend_from_slice(&v.linvel.x.to_le_bytes());
        bytes.extend_from_slice(&v.linvel.y.to_le_bytes());
        bytes.extend_from_slice(&v.angvel.to_le_bytes());

        // naive checksum implementation
        checksum.value = fletcher16(&bytes);
        log::info!("Hash at frame {}: {}", frame_count.frame, checksum.value);
    }

    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        frame_count.rapier_checksum = fletcher16(&context_bytes);
        log::info!(
            "Context Hash after frame {}: {}",
            frame_count.frame,
            frame_count.rapier_checksum
        );
    }
}

/// Computes the fletcher16 checksum, copied from wikipedia: <https://en.wikipedia.org/wiki/Fletcher%27s_checksum>
pub fn fletcher16(data: &[u8]) -> u16 {
    let mut sum1: u16 = 0;
    let mut sum2: u16 = 0;

    for byte in data {
        sum1 = (sum1 + *byte as u16) % 255;
        sum2 = (sum2 + sum1) % 255;
    }

    (sum2 << 8) | sum1
}
