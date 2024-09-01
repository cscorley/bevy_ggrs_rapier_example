mod colliders;
mod frames;
mod log_plugin;
mod network;
mod physics;
mod random_movement;
mod rollback;
mod spawn;
mod startup;

// A prelude to simplify other file imports
mod prelude {
    pub use crate::colliders::*;
    pub use crate::frames::*;
    pub use crate::log_plugin::LogSettings;
    pub use crate::network::*;
    pub use crate::physics::*;
    pub use crate::random_movement::*;
    pub use crate::rollback::*;
    pub use crate::spawn::*;
    pub use crate::startup::*;
    pub use avian2d::prelude::*;
    pub use bevy::log::*;
    pub use bevy::prelude::*;
    pub use bevy_framepace::{FramepacePlugin, FramepaceSettings, Limiter};
    pub use bevy_ggrs::prelude::*;
    pub use bevy_inspector_egui::quick::WorldInspectorPlugin;
    pub use bytemuck::{Pod, Zeroable};
    pub use ggrs::{Frame, InputStatus, PlayerType, SessionBuilder};
    pub use rand::{thread_rng, Rng};

    pub const NUM_PLAYERS: usize = 2;
    pub const FPS: usize = 60;
    pub const MAX_PREDICTION: usize = 5;
    pub const INPUT_DELAY: usize = 3;

    // Having a "load screen" time helps with initial desync issues.  No idea why,
    // but this tests well. There is also sometimes a bug when a rollback to frame 0
    // occurs if two clients have high latency.  Having this in place at least for 1
    // frame helps prevent that :-)
    pub const LOAD_SECONDS: usize = 1;

    // TODO: Hey you!!! You, the one reading this!  Yes, you.

    // Buy gschup a coffee next time you get the chance.
    // https://ko-fi.com/gschup
    // They host this match making service for us to use FOR FREE.
    // It has been an incredibly useful thing I don't have to think about while working
    // and learning how to implement this stuff and I guarantee it will be for you too.
    // pub const MATCHBOX_ADDR: &str = "wss://match.gschup.dev/bevy-ggrs-rapier-example?next=2";
    // Unfortunately, this matchbox is too out of date to work with the latest plugin.

    // So, use Johan's compatible matchbox.
    // Check out their work on "Cargo Space", especially the blog posts, which are incredibly enlightening!
    // https://johanhelsing.studio/cargospace
    pub const MATCHBOX_ADDR: &str =
        "wss://match-0-7.helsing.studio/bevy-ggrs-rapier-example?next=2";
    // Care to run your own matchbox?  Great!
    // pub const MATCHBOX_ADDR: &str = "ws://localhost:3536/bevy-ggrs-rapier-example?next=2";
    // TODO: Maybe update this room name (bevy-ggrs-rapier-example) so we don't test with each other :-)
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
enum ExampleSystemSets {
    Rollback,
    Game,
    SaveAndChecksum,
}

use bevy::ecs::schedule::ScheduleBuildSettings;
use bevy_ggrs::{GgrsApp, GgrsPlugin};

use crate::prelude::*;

fn main() {
    let mut app = App::new();

    // First thing's first:  we need to gain control of how our entities that
    // will have physics interactions spawn.  This generates placeholders at
    // the very start, ensuring the first thing this app does is have a pool
    // of entities that we can select from later, before any plugins can spawn
    // ahead of us, or in the middle of us.  These entities will be used to
    // deterministically assign components we care about to them in the startup
    // phase, and because they're deterministically assigned, we can serialize
    // them in Rapier the same every time.
    //
    // Yes, this is kind of silly, but a handy workaround for now.
    // For comparison, in release mode my context hash at init: 18674
    // Having 100+ entities ready to spawn will cause bevy_rapier to receive
    // components out-of-order.  This is good for testing desync on frame 1!
    let _ = app
        .world_mut()
        .spawn_batch((0..101).map(DeterministicSpawnBundle::new))
        .collect::<Vec<Entity>>();

    // Something smaller so we can put these side by side
    let window_info = Window {
        title: "Example".into(),
        resolution: (800.0, 600.0).into(),
        ..default()
    };

    // DefaultPlugins will use window descriptor
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(LogSettings {
            level: Level::INFO,
            ..default()
        })
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(window_info),
                    ..default()
                })
                .build()
                .disable::<LogPlugin>(),
        )
        // Add our own log plugin to help with comparing desync output
        .add_plugins(log_plugin::LogPlugin)
        .add_systems(Startup, startup)
        //.add_systems(Startup, reset_rapier)
        .add_systems(Startup, respawn_all)
        .add_systems(Startup, connect)
        .add_systems(Update, toggle_random_input)
        .add_systems(Update, close_on_esc)
        .add_systems(Update, update_matchbox_socket)
        .add_systems(Update, handle_p2p_events);

    app.add_plugins(GgrsPlugin::<ExampleGgrsConfig>::default())
        .set_rollback_schedule_fps(FPS)
        .add_systems(bevy_ggrs::ReadInputs, input)
        // We must add a specific checksum check for everything we want to include in desync detection.
        // It is probably OK to just check the components, but for demo purposes let's make sure Rapier always agrees.
        // Store everything that Rapier updates in its Writeback stage
        // TODO: checksum more
        .checksum_component::<Transform>(|t| fletcher16(&t.translation.x.to_ne_bytes()) as u64)
        .rollback_resource_with_copy::<Checksum>()
        .rollback_component_with_copy::<GlobalTransform>()
        .rollback_component_with_copy::<Transform>()
        .rollback_component_with_copy::<LinearVelocity>()
        .rollback_component_with_copy::<AngularVelocity>()

        // automatic
        .rollback_component_with_clone::<Collider>()
        .rollback_component_with_clone::<CollidingEntities>()
        .rollback_component_with_copy::<AccumulatedTranslation>()
        .rollback_component_with_copy::<CenterOfMass>()
        .rollback_component_with_copy::<ColliderAabb>()
        .rollback_component_with_copy::<ColliderDensity>()
        .rollback_component_with_copy::<ColliderMarker>()
        .rollback_component_with_copy::<ColliderMassProperties>()
        .rollback_component_with_copy::<ColliderParent>()
        .rollback_component_with_copy::<ColliderTransform>()
        .rollback_component_with_copy::<ExternalAngularImpulse>()
        .rollback_component_with_copy::<ExternalForce>()
        .rollback_component_with_copy::<ExternalImpulse>()
        .rollback_component_with_copy::<ExternalTorque>()
        .rollback_component_with_copy::<Friction>()
        .rollback_component_with_copy::<Inertia>()
        .rollback_component_with_copy::<InverseInertia>()
        .rollback_component_with_copy::<InverseMass>()
        .rollback_component_with_copy::<LockedAxes>()
        .rollback_component_with_copy::<Mass>()
        .rollback_component_with_copy::<Position>()
        .rollback_component_with_copy::<Restitution>()
        .rollback_component_with_copy::<RigidBody>()
        .rollback_component_with_copy::<Rotation>()
        .rollback_component_with_copy::<Sleeping>()
        .rollback_component_with_copy::<SleepingDisabled>()
        .rollback_component_with_copy::<TimeSleeping>()
        .rollback_component_with_copy::<avian2d::position::PreSolveAccumulatedTranslation>()
        .rollback_component_with_copy::<avian2d::position::PreviousRotation>()
        .rollback_component_with_copy::<avian2d::sync::PreviousGlobalTransform>()
        //.rollback_component_with_copy::<PreSolveAngularVelocity>() // pub(crate)
        //.rollback_component_with_copy::<PreSolveLinearVelocity>() // pub(crate)
        //.rollback_component_with_copy::<PreviousColliderTransform>() // pub(crate)

        // TODO: not sure if rolling these back are necessary
        //.rollback_resource_with_copy::<Time<()>>()
        //.rollback_resource_with_copy::<Time<Fixed>>()
        //.rollback_resource_with_copy::<Time<Physics>>()
        //.rollback_resource_with_copy::<Time<Real>>()
        //.rollback_resource_with_copy::<Time<Substeps>>()
        //.rollback_resource_with_copy::<Time<Virtual>>()
        //.rollback_component_with_copy::<SphericalJoint>() // 3d
        .rollback_component_with_clone::<ColliderConstructor>()
        .rollback_component_with_clone::<ColliderConstructorHierarchy>()
        .rollback_component_with_clone::<RayCaster>()
        .rollback_component_with_clone::<RayHits>()
        .rollback_component_with_clone::<Sensor>()
        .rollback_component_with_clone::<ShapeCaster>()
        .rollback_component_with_clone::<ShapeHits>()
        .rollback_component_with_clone::<broad_phase::AabbIntersections>()
        .rollback_component_with_copy::<AngularDamping>()
        .rollback_component_with_copy::<CollisionLayers>()
        .rollback_component_with_copy::<CollisionMargin>()
        .rollback_component_with_copy::<DebugRender>()
        .rollback_component_with_copy::<DistanceJoint>()
        .rollback_component_with_copy::<Dominance>()
        .rollback_component_with_copy::<FixedJoint>()
        .rollback_component_with_copy::<GravityScale>()
        .rollback_component_with_copy::<LinearDamping>()
        .rollback_component_with_copy::<PrismaticJoint>()
        .rollback_component_with_copy::<RevoluteJoint>()
        .rollback_component_with_copy::<SpeculativeMargin>()
        .rollback_component_with_copy::<SweptCcd>()
        .rollback_component_with_copy::<avian2d::sync::ancestor_marker::AncestorMarker<ColliderMarker>>()
        .rollback_component_with_copy::<avian2d::sync::ancestor_marker::AncestorMarker<RigidBody>>()
        .rollback_resource_with_clone::<NarrowPhaseConfig>()
        .rollback_resource_with_clone::<avian2d::sync::SyncConfig>()
        .rollback_resource_with_clone::<dynamics::solver::SolverConfig>()
        .rollback_resource_with_copy::<DeactivationTime>()
        .rollback_resource_with_copy::<SleepingThreshold>()
        .rollback_resource_with_copy::<SubstepCount>()
        .rollback_resource_with_reflect::<BroadCollisionPairs>()
        .rollback_resource_with_reflect::<Gravity>()
        // Game stuff
        .rollback_resource_with_reflect::<EnablePhysicsAfter>();

    // We need to a bunch of systems into the GGRSSchedule.
    // So, grab it and lets configure it with our systems, and the one from Rapier.
    app.get_schedule_mut(bevy_ggrs::GgrsSchedule)
        .unwrap() // We just configured the plugin -- this is probably fine
        // remove ambiguity detection, which doesn't work with Rapier https://github.com/dimforge/bevy_rapier/issues/356#issuecomment-1587045134
        .set_build_settings(ScheduleBuildSettings::default());

    // Configure plugin without system setup, otherwise your simulation will run twice
    app.add_plugins(PhysicsPlugins::new(bevy_ggrs::GgrsSchedule));
    app.insert_resource(Time::<Fixed>::from_hz(FPS as f64));

    app.add_systems(
        bevy_ggrs::GgrsSchedule,
        (
            log_start_frame,
            update_current_session_frame,
            log_confirmed_frame,
            // the three above must actually come before we update rollback status
            update_rollback_status,
            // these three must actually come after we update rollback status
            toggle_physics,
            apply_inputs,
            apply_deferred,
        )
            .chain()
            .before(PhysicsSet::Prepare),
    );
    app.add_systems(
        bevy_ggrs::GgrsSchedule,
        (
            //            pause_physics_test,
            log_end_frame,
            apply_deferred, // Flushing again
        )
            .chain()
            .after(PhysicsSet::Sync),
    );

    // We don't really draw anything ourselves, just show us the raw physics colliders
    app.add_plugins(PhysicsDebugPlugin::default())
        .insert_gizmo_config(PhysicsGizmos::default(), GizmoConfig::default());

    app.add_plugins(WorldInspectorPlugin::new());

    /*
       // I have found that since GGRS is limiting the movement FPS anyway,
       // there isn't much of a point in rendering more frames than necessary.
       // One thing I've yet to prove out is if this is actually detrimental or
       // not to resimulation, since we're basically taking up time that GGRS
       // would use already to pace itself.
       // You may find this useless, or bad.  Submit a PR if it is!
       app.insert_resource(FramepaceSettings {
           limiter: Limiter::from_framerate(FPS as f64),
       })
       .add_plugins(FramepacePlugin);
    */
    app.run();
}

pub fn close_on_esc(
    mut commands: Commands,
    focused_windows: Query<(Entity, &Window)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    for (window, focus) in focused_windows.iter() {
        if !focus.focused {
            continue;
        }

        if input.just_pressed(KeyCode::Escape) {
            commands.entity(window).despawn();
        }
    }
}
pub fn fletcher16(data: &[u8]) -> u16 {
    let mut sum1: u16 = 0;
    let mut sum2: u16 = 0;

    for byte in data {
        sum1 = (sum1 + *byte as u16) % 255;
        sum2 = (sum2 + sum1) % 255;
    }

    (sum2 << 8) | sum1
}
