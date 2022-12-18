mod checksum;
mod colliders;
mod desync;
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
    pub use crate::checksum::*;
    pub use crate::colliders::*;
    pub use crate::desync::*;
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
    pub use bevy_ggrs::{GGRSPlugin, PlayerInputs, Rollback, RollbackIdProvider, Session};
    pub use bevy_inspector_egui::WorldInspectorPlugin;
    pub use bevy_inspector_egui_rapier::InspectableRapierPlugin;
    pub use bevy_rapier2d::prelude::*;
    pub use bytemuck::{Pod, Zeroable};
    pub use ggrs::{Frame, InputStatus, PlayerHandle, PlayerType, SessionBuilder};
    pub use matchbox_socket::WebRtcSocket;
    pub use rand::{thread_rng, Rng};

    pub const NUM_PLAYERS: usize = 2;
    pub const FPS: usize = 60;
    pub const ROLLBACK_SYSTEMS: &str = "rollback_systems";
    pub const GAME_SYSTEMS: &str = "game_systems";
    pub const CHECKSUM_SYSTEMS: &str = "checksum_systems";
    pub const MAX_PREDICTION: usize = 5;
    pub const INPUT_DELAY: usize = 3;

    // Having a "load screen" time helps with initial desync issues.  No idea why,
    // but this tests well. There is also sometimes a bug when a rollback to frame 0
    // occurs if two clients have high latency.  Having this in place at least for 1
    // frame helps prevent that :-)
    pub const LOAD_SECONDS: usize = 1;

    // How far back we'll keep frame hash info for our other player. This should be
    // some multiple of MAX_PREDICTION, preferrably 3x, so that we can desync detect
    // outside the rollback and prediction windows.
    pub const DESYNC_MAX_FRAMES: usize = 30;

    // TODO: Hey you!!! You, the one reading this!  Yes, you.
    // Buy gschup a coffee next time you get the chance.
    // https://ko-fi.com/gschup
    // They host this match making service for us to use FOR FREE.
    // It has been an incredibly useful thing I don't have to think about while working
    // and learning how to implement this stuff and I guarantee it will be for you too.
    pub const MATCHBOX_ADDR: &str = "wss://match.gschup.dev/bevy-ggrs-rapier-example?next=2";
    // TODO: Maybe update this room name (bevy-ggrs-rapier-example) so we don't test with each other :-)
}

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
    let window_info = WindowDescriptor {
        title: "Example".into(),
        width: 800.0,
        height: 600.0,
        ..default()
    };

    // DefaultPlugins will use window descriptor
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(LogSettings {
            level: Level::INFO,
            ..default()
        })
        .insert_resource(FramepaceSettings {
            limiter: Limiter::from_framerate(FPS as f64),
        })
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    window: window_info,
                    ..default()
                })
                .build()
                .disable::<LogPlugin>(),
        )
        // Add our own log plugin to help with comparing desync output
        .add_plugin(log_plugin::LogPlugin)
        .add_startup_system(startup)
        .add_startup_system(reset_rapier)
        .add_startup_system(respawn_all)
        .add_startup_system(connect)
        .add_system(toggle_random_input)
        .add_system(bevy::window::close_on_esc)
        .add_system(update_matchbox_socket)
        .add_system(handle_p2p_events);

    GGRSPlugin::<GGRSConfig>::new()
        .with_update_frequency(FPS)
        .with_input_system(input)
        .register_rollback_resource::<PhysicsRollbackState>()
        .register_rollback_resource::<CurrentFrame>()
        // Store everything that Rapier updates in its Writeback stage
        .register_rollback_component::<GlobalTransform>()
        .register_rollback_component::<Transform>()
        .register_rollback_component::<Velocity>()
        .register_rollback_component::<Sleeping>()
        // Game stuff
        .register_rollback_resource::<EnablePhysicsAfter>()
        .with_rollback_schedule(
            Schedule::default()
                // It is imperative that this executes first, always.  Yes, I know about `.after()`.
                // I'm putting this here in case you end up adding any `Commands` to this step,
                // which I think must flush at all costs before we enter the regular game logic
                .with_stage(
                    ROLLBACK_SYSTEMS,
                    SystemStage::parallel()
                        // Just strictly ordered so we have ordered comparable
                        // logging.  Could be optimized if the logger info was
                        // synthesized into it's own system or something
                        .with_system(update_current_frame)
                        .with_system(update_current_session_frame.after(update_current_frame))
                        .with_system(update_confirmed_frame.after(update_current_session_frame))
                        // The three above must actually come before we update rollback status
                        .with_system(update_rollback_status.after(update_confirmed_frame))
                        // These three must actually come after we update rollback status
                        .with_system(update_validatable_frame.after(update_rollback_status))
                        .with_system(toggle_physics.after(update_rollback_status))
                        .with_system(rollback_rapier_context.after(toggle_physics)),
                )
                // Add our game logic and systems here.  If it impacts what the
                // physics engine should consider, do it here.
                .with_stage_after(
                    ROLLBACK_SYSTEMS,
                    GAME_SYSTEMS,
                    SystemStage::parallel()
                        .with_system(apply_inputs)
                        // The `frame_validator` relies on the execution of `apply_inputs` and must come after.
                        // It could happen anywhere else, I just stuck it here to be clear.
                        // If this is causing your game to quit, you have a bug!
                        .with_system(frame_validator.after(apply_inputs))
                        .with_system(force_update_rollbackables),
                )
                // The next 3 stages are all bevy_rapier stages.  Best to leave these in order.
                .with_stage_after(
                    GAME_SYSTEMS,
                    PhysicsStages::SyncBackend,
                    SystemStage::parallel().with_system_set(
                        RapierPhysicsPlugin::<NoUserData>::get_systems(PhysicsStages::SyncBackend),
                    ),
                )
                .with_stage_after(
                    PhysicsStages::SyncBackend,
                    PhysicsStages::StepSimulation,
                    SystemStage::parallel().with_system_set(
                        RapierPhysicsPlugin::<NoUserData>::get_systems(
                            PhysicsStages::StepSimulation,
                        ),
                    ),
                )
                .with_stage_after(
                    PhysicsStages::StepSimulation,
                    PhysicsStages::Writeback,
                    SystemStage::parallel().with_system_set(
                        RapierPhysicsPlugin::<NoUserData>::get_systems(PhysicsStages::Writeback),
                    ),
                )
                // This must execute after writeback to store the RapierContext
                .with_stage_after(
                    PhysicsStages::Writeback,
                    CHECKSUM_SYSTEMS,
                    SystemStage::parallel().with_system(save_rapier_context),
                ),
        )
        .build(&mut app);

    // Be sure to setup all four stages.
    // We don't despawn in this example, but you may want to :-)
    app.add_stage_before(
        CoreStage::Last,
        PhysicsStages::DetectDespawn,
        SystemStage::parallel().with_system_set(RapierPhysicsPlugin::<NoUserData>::get_systems(
            PhysicsStages::DetectDespawn,
        )),
    );

    // Configure plugin without system setup, otherwise your simulation will run twice
    app.add_plugin(
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

    app
        // We don't really draw anything ourselves, just show us the raw physics colliders
        .add_plugin(RapierDebugRenderPlugin {
            enabled: true,
            ..default()
        })
        .add_plugin(InspectableRapierPlugin)
        .add_plugin(WorldInspectorPlugin::default())
        .add_plugin(FramepacePlugin);

    app.run();
}
