use crate::prelude::*;

pub fn startup(mut commands: Commands) {
    // frame updating
    commands.insert_resource(LastFrame::default());
    commands.insert_resource(CurrentFrame::default());
    commands.insert_resource(CurrentSessionFrame::default());
    commands.insert_resource(ConfirmedFrame::default());
    commands.insert_resource(RollbackStatus::default());
    commands.insert_resource(ValidatableFrame::default());

    // desync detection
    commands.insert_resource(FrameHashes::default());
    commands.insert_resource(RxFrameHashes::default());

    // ggrs local players
    commands.insert_resource(LocalHandles::default());
    //commands.insert_resource(WrappedSessionType::default());

    // physics toggling
    commands.insert_resource(EnablePhysicsAfter::default());
    commands.insert_resource(PhysicsEnabled::default());

    // random movement for testing
    commands.insert_resource(RandomInput { on: true });

    // network timer
    commands.insert_resource(NetworkStatsTimer(Timer::from_seconds(
        2.0,
        TimerMode::Repeating,
    )))
}

pub fn reset_rapier(
    mut commands: Commands,
    mut rapier: ResMut<RapierContext>,
    collider_handles: Query<Entity, With<RapierColliderHandle>>,
    rb_handles: Query<Entity, With<RapierRigidBodyHandle>>,
) {
    // You might be wondering:  why is this here?  What purpose does it serve?
    // In just resets everything on startup!
    // Yes.  But this bad boy right here is a good system you can use to reset
    // Rapier whenever you please in your game (e.g., after a game ends or
    // between rounds).  It isn't quite a nuclear option, but a rollbackable one!

    // Force rapier to reload everything
    for e in collider_handles.iter() {
        commands.entity(e).remove::<RapierColliderHandle>();
    }
    for e in rb_handles.iter() {
        commands.entity(e).remove::<RapierRigidBodyHandle>();
    }

    // Re-initialize everything we overwrite with default values
    let context = RapierContext::default();
    rapier.bodies = context.bodies;
    rapier.colliders = context.colliders;
    rapier.broad_phase = context.broad_phase;
    rapier.narrow_phase = context.narrow_phase;
    rapier.ccd_solver = context.ccd_solver;
    rapier.impulse_joints = context.impulse_joints;
    rapier.integration_parameters = context.integration_parameters;
    rapier.islands = context.islands;
    rapier.multibody_joints = context.multibody_joints;
    rapier.pipeline = context.pipeline;
    rapier.query_pipeline = context.query_pipeline;

    // Add a bit more CCD
    // This is objectively just something that could be setup once, but we did
    // just wholesale overwrite this anyway.  I think you can just not override
    // integration_parameters above, but where's the fun in that?
    rapier.integration_parameters.max_ccd_substeps = 5;

    // Serialize our "blank" slate for frame 0.
    // This is actually important because it is possible to rollback to this!
    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        let rapier_checksum = fletcher16(&context_bytes);
        log::info!("Context hash at init: {}", rapier_checksum);

        commands.insert_resource(PhysicsRollbackState {
            rapier_state: Some(context_bytes),
            rapier_checksum,
        })
    } else {
        commands.insert_resource(PhysicsRollbackState::default());
    }
}

pub fn respawn_all(mut commands: Commands, spawn_pool: Query<(Entity, &DeterministicSpawn)>) {
    commands.spawn(Camera2dBundle::default());

    // Everything must be spawned in the same order, every time,
    // deterministically.  There is also potential for bevy itself to return
    // queries to bevy_rapier that do not have the entities in the same order,
    // but in my experience with this example, that happens somewhat rarely.  A
    // patch to bevy_rapier is required to ensure some sort of Entity order
    // otherwise on the reading end, much like the below sorting of our spawn.
    // WARNING:  This is something on my branch only!  This is in bevy_rapier PR #233

    // Get our entities and sort them by the spawn component index
    let mut sorted_spawn_pool: Vec<(Entity, &DeterministicSpawn)> = spawn_pool.iter().collect();
    sorted_spawn_pool.sort_by_key(|e| e.1.index);
    // Get the Entities in reverse for easy popping
    let mut sorted_entity_pool: Vec<Entity> = sorted_spawn_pool.iter().map(|p| p.0).rev().collect();

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Ball"))
        .insert(DynamicColliderBundle {
            collider: Collider::ball(4.),
            restitution: Restitution::coefficient(2.0),
            ccd: Ccd::enabled(),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(0., 10., 0.),
            ..default()
        })
        .add_rollback();

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Player 1"))
        .insert(Player { handle: 0 })
        .insert(DynamicColliderBundle {
            collider: Collider::cuboid(8., 8.),
            locked_axes: LockedAxes::ROTATION_LOCKED,
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(-10., -50., 0.),
            ..default()
        })
        .add_rollback();

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Player 2"))
        .insert(Player { handle: 1 })
        .insert(DynamicColliderBundle {
            collider: Collider::cuboid(8., 8.),
            locked_axes: LockedAxes::ROTATION_LOCKED,
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(10., -50., 0.),
            ..default()
        })
        .add_rollback();

    let thickness = 10.0;
    let box_length = 200.0;
    let overlapping_box_length = box_length + thickness;

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Floor"))
        .insert(FixedColliderBundle {
            collider: Collider::cuboid(overlapping_box_length, thickness),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(0., -box_length, 0.),
            ..default()
        });

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Left Wall"))
        .insert(FixedColliderBundle {
            collider: Collider::cuboid(thickness, overlapping_box_length),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(-box_length, 0., 0.),
            ..default()
        });

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Right Wall"))
        .insert(FixedColliderBundle {
            collider: Collider::cuboid(thickness, overlapping_box_length),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(box_length, 0., 0.),
            ..default()
        });

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Ceiling"))
        .insert(FixedColliderBundle {
            collider: Collider::cuboid(overlapping_box_length, thickness),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(0., box_length, 0.),
            ..default()
        });

    let corner_position = box_length - thickness + 4.;
    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Southeast Corner"))
        .insert(FixedColliderBundle {
            collider: Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(-thickness * 2., 0.),
                Vec2::new(0., thickness * 2.),
            ])
            .unwrap(),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(corner_position, -corner_position, 0.),
            ..default()
        });

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Southwest Corner"))
        .insert(FixedColliderBundle {
            collider: Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(thickness * 2., 0.),
                Vec2::new(0., thickness * 2.),
            ])
            .unwrap(),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(-corner_position, -corner_position, 0.),
            ..default()
        });

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Northeast Corner"))
        .insert(FixedColliderBundle {
            collider: Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(-thickness * 2., 0.),
                Vec2::new(0., -thickness * 2.),
            ])
            .unwrap(),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(corner_position, corner_position, 0.),
            ..default()
        });

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Northwest Corner"))
        .insert(FixedColliderBundle {
            collider: Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(thickness * 2., 0.),
                Vec2::new(0., -thickness * 2.),
            ])
            .unwrap(),
            ..default()
        })
        .insert(TransformBundle {
            local: Transform::from_xyz(-corner_position, corner_position, 0.),
            ..default()
        });
}
