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
    pub use bevy::log::*;
    pub use bevy::prelude::*;
    pub use bevy::tasks::IoTaskPool;
    pub use bevy_framepace::{FramepacePlugin, FramepaceSettings, Limiter};
    pub use bevy_ggrs::prelude::*;
    pub use bevy_inspector_egui::quick::WorldInspectorPlugin;
    pub use bevy_matchbox::matchbox_socket::WebRtcSocket;
    pub use bevy_matchbox::prelude::*;
    pub use bevy_rapier2d::prelude::*;
    pub use bytemuck::{Pod, Zeroable};
    pub use ggrs::{Frame, InputStatus, PlayerHandle, PlayerType, SessionBuilder};
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
        .world
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
        .add_systems(Startup, reset_rapier)
        .add_systems(Startup, respawn_all)
        .add_systems(Startup, connect)
        .add_systems(Update, toggle_random_input)
        .add_systems(Update, bevy::window::close_on_esc)
        .add_systems(Update, update_matchbox_socket)
        .add_systems(Update, handle_p2p_events);

    app.add_plugins(GgrsPlugin::<ExampleGgrsConfig>::default())
        .set_rollback_schedule_fps(FPS)
        .add_systems(bevy_ggrs::ReadInputs, input)
        // We must add a specific checksum check for everything we want to include in desync detection.
        // It is probably OK to just check the components, but for demo purposes let's make sure Rapier always agrees.
        .checksum_resource_with_hash::<PhysicsRollbackState>()
        .rollback_resource_with_clone::<PhysicsRollbackState>()
        // Store everything that Rapier updates in its Writeback stage
        .rollback_component_with_reflect::<GlobalTransform>()
        .rollback_component_with_reflect::<Transform>()
        .rollback_component_with_reflect::<Velocity>()
        .rollback_component_with_reflect::<Sleeping>()
        // Game stuff
        .rollback_resource_with_reflect::<EnablePhysicsAfter>();

    // We need to a bunch of systems into the GGRSSchedule.
    // So, grab it and lets configure it with our systems, and the one from Rapier.
    app.get_schedule_mut(bevy_ggrs::GgrsSchedule)
        .unwrap() // We just configured the plugin -- this is probably fine
        // remove ambiguity detection, which doesn't work with Rapier https://github.com/dimforge/bevy_rapier/issues/356#issuecomment-1587045134
        .set_build_settings(ScheduleBuildSettings::default())
        .configure_sets(
            (
                // It is imperative that this executes first, always.
                // I'm putting this here in case you end up adding any `Commands` to this step,
                // which I think must flush at all costs before we enter the regular game logic
                ExampleSystemSets::Rollback,
                // Add our game logic and systems here.  If it impacts what the
                // physics engine should consider, do it here.
                ExampleSystemSets::Game,
                // The next 4 stages are all bevy_rapier stages.  Best to leave these in order.
                // This is setup to execute exactly how the plugin would execute if we were to use
                // with_default_system_setup(true) instead (the plugin is configured next)
                PhysicsSet::SyncBackend,
                PhysicsSet::StepSimulation,
                PhysicsSet::Writeback,
                // This must execute after writeback to store the RapierContext
                ExampleSystemSets::SaveAndChecksum,
            )
                .chain(),
        )
        .add_systems(
            (
                log_start_frame,
                update_current_session_frame,
                log_confirmed_frame,
                // the three above must actually come before we update rollback status
                update_rollback_status,
                // these three must actually come after we update rollback status
                toggle_physics,
                rollback_rapier_context,
                // Make sure to flush everything before we apply our game logic.
                apply_deferred,
            )
                // There is a bit more specific ordering you can do with these
                // systems, but since GGRS configures it's schedule to require
                // absolute unambiguous systems, I'm just going to take the lazy
                // way out and `chain` them in order.
                .chain()
                .in_set(ExampleSystemSets::Rollback),
        )
        .add_systems(
            (
                apply_inputs,
                force_update_rollbackables,
                // Make sure to flush everything before Rapier syncs
                apply_deferred,
            )
                .chain()
                .in_set(ExampleSystemSets::Game),
        )
        .add_systems(
            RapierPhysicsPlugin::<NoUserData>::get_systems(PhysicsSet::SyncBackend)
                .in_set(PhysicsSet::SyncBackend),
        )
        .add_systems(
            RapierPhysicsPlugin::<NoUserData>::get_systems(PhysicsSet::StepSimulation)
                .in_set(PhysicsSet::StepSimulation),
        )
        .add_systems(
            RapierPhysicsPlugin::<NoUserData>::get_systems(PhysicsSet::Writeback)
                .in_set(PhysicsSet::Writeback),
        )
        .add_systems(
            (
                save_rapier_context, // This must execute after writeback to store the RapierContext
                log_end_frame,
                apply_deferred, // Flushing again
            )
                .chain()
                .in_set(ExampleSystemSets::SaveAndChecksum),
        );

    // Configure plugin without system setup, otherwise your simulation will run twice
    app.add_plugins(
        RapierPhysicsPlugin::<NoUserData>::default()
            // The physics scale really should not matter for a game of this size
            .with_physics_scale(1.)
            // This allows us to hook in the systems ourselves above in the GGRS schedule
            .with_default_system_setup(false),
    );

    // Make sure to insert a new configuration with fixed timestep mode after configuring the plugin
    app.insert_resource(RapierConfiguration {
        // The timestep_mode MUST be fixed
        timestep_mode: TimestepMode::Fixed {
            dt: 1. / FPS as f32,
            substeps: 1,
        },

        // This should work with gravity, too.  It is fun for testing.
        // gravity: Vec2::ZERO,

        // Turn off query pipeline since this example does not use it
        query_pipeline_active: false,

        // We will turn this on after "loading", this helps when looking at init issues
        physics_pipeline_active: false,

        // Do not check internal structures for transform changes
        force_update_from_transform_changes: true,

        ..default()
    });

    // We don't really draw anything ourselves, just show us the raw physics colliders
    app.add_plugins(RapierDebugRenderPlugin {
        enabled: true,
        ..default()
    })
    .add_plugins(WorldInspectorPlugin::new());

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

    app.run();
}
