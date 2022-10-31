mod log_plugin;

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_ggrs::{GGRSPlugin, Rollback, RollbackIdProvider, SessionType};
use bevy_rapier2d::prelude::*;
use bytemuck::{Pod, Zeroable};
use ggrs::{Config, Frame, InputStatus, P2PSession, PlayerHandle, PlayerType, SessionBuilder};
use matchbox_socket::WebRtcSocket;
use rand::{thread_rng, Rng};

use bevy::log::*;
use bevy_inspector_egui::WorldInspectorPlugin;
use bevy_inspector_egui_rapier::InspectableRapierPlugin;

const NUM_PLAYERS: usize = 2;
const FPS: usize = 60;
const ROLLBACK_SYSTEMS: &str = "rollback_systems";
const GAME_SYSTEMS: &str = "game_systems";
const CHECKSUM_SYSTEMS: &str = "checksum_systems";
const MAX_PREDICTION: usize = 8;
const INPUT_DELAY: usize = 2;

// Having a "load screen" time helps with initial desync issues.  No idea why,
// but this tests well. There is also sometimes a bug when a rollback to frame 0
// occurs if two clients have high latency.  Having this in place at least for 1
// frame helps prevent that :-)
const LOAD_SECONDS: usize = 1;

// How far back we'll keep frame hash info for our other player
const DESYNC_MAX_FRAMES: usize = 30;

// TODO: Hey you!!! You, the one reading this!  Yes, you.
// Buy gschup a coffee next time you get the chance.
// https://ko-fi.com/gschup
// They host this match making service for us to use FOR FREE.
// It has been an incredibly useful thing I don't have to think about while working
// and learning how to implement this stuff and I guarantee it will be for you too.
const MATCHBOX_ADDR: &str = "wss://match.gschup.dev/bevy-ggrs-rapier-example?next=2";
// TODO: Maybe update this room name (bevy-ggrs-rapier-example) so we don't test with each other :-)

// These are just 16 bit for bit-packing alignment in the input struct
const INPUT_UP: u16 = 0b0001;
const INPUT_DOWN: u16 = 0b0010;
const INPUT_LEFT: u16 = 0b0100;
const INPUT_RIGHT: u16 = 0b1000;

/// Local handles, this should just be 1 entry in this demo, but you may end up wanting to implement 2v2
#[derive(Default)]
pub struct LocalHandles {
    pub handles: Vec<PlayerHandle>,
}

/// Our primary data struct; what players send to one another
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Pod, Zeroable)]
pub struct GGRSInput {
    // The input from our player
    pub input: u16,

    // Desync detection
    pub last_confirmed_hash: u16,
    pub last_confirmed_frame: Frame,
    // Ok, so I know what you're thinking:
    // > "That's not input!"
    // Well, you're right, and we're going to abuse the existing socket to also
    // communicate about the last confirmed frame we saw and what was the hash
    // of the physics state.  This allows us to detect desync.  This could also
    // use a new socket, but who wants to hole punch twice?  I have been working
    // on a GGRS branch (linked below) that introduces a new message type, but
    // it is not ready.  However, input-packing works good enough for now.
    // https://github.com/cscorley/ggrs/tree/arbitrary-messages-0.8
}

/// The main GGRS configuration type
#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = GGRSInput;
    // bevy_ggrs doesn't really use State, so just make this a small whatever
    type State = u8;
    type Address = String;
}

/// Metadata we need to store about frames we've rendered locally
#[derive(Default, Hash, Component, PartialEq, Eq, Debug)]
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
#[derive(Default, Hash, Component, PartialEq, Eq, Debug)]
pub struct RxFrameHash {
    /// The frame number for this metadata
    pub frame: Frame,

    /// The checksum of the Rapier physics state for the frame.  I use this term interchangably with `hash`, sorry.
    pub rapier_checksum: u16,

    /// Has been validated by us against other player
    pub validated: bool,
}

// A collection of confirmed frame hashes we've seen locally
#[derive(Default, Hash, Component, PartialEq, Eq)]
pub struct FrameHashes(pub [FrameHash; DESYNC_MAX_FRAMES]);

// A collection of confirmed frame hashes we've received from our other player
// This only works for 1v1.  This would have to be extended to consider all
// remotes in larger scenarios (I accept pull requests!)
#[derive(Default, Hash, Component, PartialEq, Eq)]
pub struct RxFrameHashes(pub [RxFrameHash; DESYNC_MAX_FRAMES]);

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

/// GGRS player handle, we use this to associate GGRS handles back to our [`Entity`]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Component)]
pub struct Player {
    pub handle: usize,
}

/// Easy way to detect rollbacks
#[derive(Default, Reflect, Hash, Component, PartialEq, Eq)]
#[reflect(Hash, Component, PartialEq)]
pub struct LastFrameCount {
    pub frame: Frame,
}

/// Controls whether our opponent will inject random inputs while inactive.
/// This is useful for testing rollbacks locally and can be toggled off with `r`
/// and `t`.
#[derive(Default, Reflect, Hash, Component, PartialEq, Eq)]
#[reflect(Hash, Component, PartialEq)]
pub struct RandomInput {
    pub on: bool,
}

/// Our GameState, which will be rolled back and we will use to restore our
/// physics state.
#[derive(Default, Reflect, Hash, Component, PartialEq, Eq)]
#[reflect(Hash, Component, PartialEq)]
pub struct GameState {
    pub rapier_state: Option<Vec<u8>>,
    pub frame: Frame,
    pub rapier_checksum: u16,
}

/// Not necessary for this demo, but useful debug output sometimes.
struct NetworkStatsTimer(Timer);

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
    // For comparison, in release mode my context hash at init: 4591
    let _ = app
        .world
        .spawn_batch((0..11).map(DeterministicSpawnBundle::new))
        .collect::<Vec<Entity>>();

    // Something smaller so we can put these side by side
    let window_info = WindowDescriptor {
        title: "Example".into(),
        width: 800.0,
        height: 600.0,
        ..default()
    };

    // DefaultPlugins will use window descriptor
    app.insert_resource(window_info)
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(LogSettings {
            level: Level::DEBUG,
            ..default()
        })
        .add_plugins_with(DefaultPlugins, |plugins| plugins.disable::<LogPlugin>())
        // Add our own log plugin to help with comparing desync output
        .add_plugin(log_plugin::LogPlugin)
        .add_startup_system(startup)
        .add_system(keyboard_input)
        .add_system(bevy::window::close_on_esc)
        .add_system(update_matchbox_socket)
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(2.0, true)))
        .add_system(print_network_stats_system)
        .add_system(print_events_system)
        // We don't really draw anything ourselves, just show us the raw physics colliders
        .add_plugin(RapierDebugRenderPlugin::default())
        .add_plugin(InspectableRapierPlugin)
        .add_plugin(WorldInspectorPlugin::default());

    GGRSPlugin::<GGRSConfig>::new()
        .with_update_frequency(FPS)
        .with_input_system(input)
        .register_rollback_type::<GameState>()
        // Store everything that Rapier updates in its Writeback stage
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Velocity>()
        .register_rollback_type::<Sleeping>()
        .with_rollback_schedule(
            Schedule::default()
                // It is imperative that this executes first, always.  Yes, I know about `.after()`.
                // I'm putting this here in case you end up adding any `Commands` to this step,
                // which I think must flush at all costs before we enter the regular game logic
                .with_stage(
                    ROLLBACK_SYSTEMS,
                    SystemStage::parallel().with_system(update_game_state),
                )
                // Add our game logic and systems here.
                // If it impacts what the physics engine should consider, do it here.
                .with_stage_after(
                    ROLLBACK_SYSTEMS,
                    GAME_SYSTEMS,
                    SystemStage::parallel()
                        .with_system(apply_inputs)
                        // The `frame_validator` relies on the execution of `apply_inputs` and must come after.
                        // It could happen anywhere else, I just stuck it here to be clear.
                        // If this is causing your game to quit, you have a bug!
                        .with_system(frame_validator.after(apply_inputs)),
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
                    SystemStage::parallel().with_system(save_game_state),
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
            // Scale of 8 since that's the factor size of our ball & players.
            // This choice was made kind of arbitrarily.
            .with_physics_scale(8.)
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

    app.run();
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

pub fn startup(
    mut commands: Commands,
    mut rip: ResMut<RollbackIdProvider>,
    mut rapier: ResMut<RapierContext>,
    spawn_pool: Query<(Entity, &DeterministicSpawn)>,
) {
    // Add a bit more CCD.  This is simply my preference and should not impact the Rapier/GGRS combo.
    rapier.integration_parameters.max_ccd_substeps = 5;

    // Insert our game state, already setup with the (hopefully) empty RapierContext
    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        let rapier_checksum = fletcher16(&context_bytes);
        log::info!("Context hash at init: {}", rapier_checksum);

        commands.insert_resource(GameState {
            rapier_state: Some(context_bytes),
            rapier_checksum,
            ..default()
        })
    } else {
        commands.insert_resource(GameState::default());
    }

    // A bunch of stuff for our wacky systems :-)
    commands.insert_resource(RandomInput { on: true });
    commands.insert_resource(FrameHashes::default());
    commands.insert_resource(RxFrameHashes::default());
    commands.insert_resource(LastFrameCount::default());
    commands.insert_resource(LocalHandles::default());
    commands.spawn_bundle(Camera2dBundle::default());

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
        .insert(Rollback::new(rip.next_id()))
        .insert(Collider::ball(4.))
        .insert(RigidBody::Dynamic)
        .insert(Velocity::default())
        .insert(Sleeping::default())
        .insert(Ccd::enabled())
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(0., 10., 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Player 1"))
        .insert(Player { handle: 0 })
        .insert(Rollback::new(rip.next_id()))
        .insert(Collider::cuboid(8., 8.))
        .insert(LockedAxes::ROTATION_LOCKED)
        .insert(Restitution::default())
        .insert(RigidBody::Dynamic)
        .insert(Velocity::default())
        .insert(Sleeping::default())
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(-10., -50., 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Player 2"))
        .insert(Player { handle: 1 })
        .insert(Rollback::new(rip.next_id()))
        .insert(Collider::cuboid(8., 8.))
        .insert(LockedAxes::ROTATION_LOCKED)
        .insert(Restitution::default())
        .insert(RigidBody::Dynamic)
        .insert(Velocity::default())
        .insert(Sleeping::default())
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(10., -50., 0.));

    let thickness = 10.0;
    let box_length = 200.0;
    let overlapping_box_length = box_length + thickness;

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Floor"))
        .insert(Collider::cuboid(overlapping_box_length, thickness))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(0., -box_length, 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Left Wall"))
        .insert(Collider::cuboid(thickness, overlapping_box_length))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(-box_length, 0., 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Right Wall"))
        .insert(Collider::cuboid(thickness, overlapping_box_length))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(box_length, 0., 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Ceiling"))
        .insert(Collider::cuboid(overlapping_box_length, thickness))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(0., box_length, 0.));

    let corner_position = box_length - thickness + 4.;
    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Southeast Corner"))
        .insert(
            Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(-thickness * 2., 0.),
                Vec2::new(0., thickness * 2.),
            ])
            .unwrap(),
        )
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(corner_position, -corner_position, 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Southwest Corner"))
        .insert(
            Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(thickness * 2., 0.),
                Vec2::new(0., thickness * 2.),
            ])
            .unwrap(),
        )
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(-corner_position, -corner_position, 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Northeast Corner"))
        .insert(
            Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(-thickness * 2., 0.),
                Vec2::new(0., -thickness * 2.),
            ])
            .unwrap(),
        )
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(corner_position, corner_position, 0.));

    commands
        .entity(sorted_entity_pool.pop().unwrap())
        .insert(Name::new("Northwest Corner"))
        .insert(
            Collider::convex_hull(&[
                Vec2::new(0., 0.),
                Vec2::new(thickness * 2., 0.),
                Vec2::new(0., -thickness * 2.),
            ])
            .unwrap(),
        )
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(-corner_position, corner_position, 0.));

    // Connect immediately.
    // This starts to poll the matchmaking service for our other player to connect.
    let (socket, message_loop) = WebRtcSocket::new(MATCHBOX_ADDR);
    let task_pool = IoTaskPool::get();
    task_pool.spawn(message_loop).detach();
    commands.insert_resource(Some(socket));
}

pub fn input(
    _handle: In<PlayerHandle>, // Required by bevy_ggrs
    keyboard_input: Res<Input<KeyCode>>,
    random: Res<RandomInput>,
    rapier_config: Res<RapierConfiguration>,
    mut hashes: ResMut<FrameHashes>,
) -> GGRSInput {
    let mut input: u16 = 0;
    let mut last_confirmed_frame = ggrs::NULL_FRAME;
    let mut last_confirmed_hash = 0;

    // Do not do anything until physics are live
    if !rapier_config.physics_pipeline_active {
        return GGRSInput {
            input,
            last_confirmed_frame,
            last_confirmed_hash,
        };
    }

    // Find a hash that we haven't sent yet.
    // This probably seems like overkill but we have to track a bunch anyway, we
    // might as well do our due diligence and inform our opponent of every hash
    // we have This may mean we ship them out of order.  The important thing is
    // we determine the desync *eventually* because that match is pretty much
    // invalidated without a state synchronization mechanism (which GGRS/GGPO
    // does not have out of the box.)
    for frame_hash in hashes.0.iter_mut() {
        if frame_hash.confirmed && !frame_hash.sent {
            info!("Sending data {:?}", frame_hash);
            last_confirmed_frame = frame_hash.frame;
            last_confirmed_hash = frame_hash.rapier_checksum;
            frame_hash.sent = true;
        }
    }

    if keyboard_input.pressed(KeyCode::W) {
        input |= INPUT_UP;
    }
    if keyboard_input.pressed(KeyCode::A) {
        input |= INPUT_LEFT;
    }
    if keyboard_input.pressed(KeyCode::S) {
        input |= INPUT_DOWN;
    }
    if keyboard_input.pressed(KeyCode::D) {
        input |= INPUT_RIGHT;
    }

    if input == 0 && random.on {
        let mut rng = thread_rng();
        // Return a random input sometimes, or maybe nothing.
        // Helps to trigger input-based rollbacks from the unplayed side
        match rng.gen_range(0..6) {
            0 => input = INPUT_UP,
            1 => input = INPUT_LEFT,
            2 => input = INPUT_DOWN,
            3 => input = INPUT_RIGHT,
            _ => (),
        }
    }

    GGRSInput {
        input,
        last_confirmed_frame,
        last_confirmed_hash,
    }
}

pub fn update_game_state(
    mut game_state: ResMut<GameState>,
    mut last_frame_count: ResMut<LastFrameCount>,
    mut rapier: ResMut<RapierContext>,
    mut config: ResMut<RapierConfiguration>,
    /*
    mut transforms: Query<&mut Transform, With<Rollback>>,
    mut velocities: Query<&mut Velocity, With<Rollback>>,
    mut sleepings: Query<&mut Sleeping, With<Rollback>>,
    mut exit: EventWriter<bevy::app::AppExit>,
    */
) {
    let is_rollback = last_frame_count.frame > game_state.frame;
    if is_rollback {
        log::info!(
            "rollback on {} to {}",
            last_frame_count.frame,
            game_state.frame
        );
    }

    game_state.frame += 1;
    // I know this seems silly at first glance, but after we know we've entered
    // a rollback once, we have to resimulate all frames back to where we left
    // off... and there may be additional rollbacks that happen during that!
    last_frame_count.frame = game_state.frame;

    // Serialize our physics state for hashing.  We can likely avoid this work during rollbacks.
    // This should not be necessary for this demo to work, as we will do the real checksum
    // during `save_game_state` at the end of the pipeline.
    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        // Update existing checksum to current state
        game_state.rapier_checksum = fletcher16(&context_bytes);

        // Compare to serialized state, also a rollback indicator, only useful for desync debugging
        let serialized_checksum = if let Some(state_context) = game_state.rapier_state.as_ref() {
            fletcher16(state_context)
        } else {
            0
        };

        log::info!(
            "Context hash at start: {}\t{}",
            game_state.rapier_checksum,
            serialized_checksum
        );
    }

    // Trigger changes for all of these to be sure sync picks them up.
    // This is tin-foil hat stuff, and has been useful to debug desync origins
    /*
           for mut t in transforms.iter_mut() {
               t.set_changed();
           }
           for mut v in velocities.iter_mut() {
               v.set_changed();
           }
           for mut s in sleepings.iter_mut() {
               s.set_changed();
           }
    */

    // Only restore our state if we are in a rollback.  This step is *critical*.
    // Only doing this during rollbacks saves us a step every frame.  Here, we
    // also do not allow rollback to frame 0.  Physics state is already correct
    // in this case.  This prevents lagged clients from getting immediate desync
    // and is entirely a hack since we don't enable physics until later anyway.
    //
    // You can also test that desync detection is working by disabling:
    // if false {
    if is_rollback && game_state.frame > 1 {
        if let Some(state_context) = game_state.rapier_state.as_ref() {
            if let Ok(context) = bincode::deserialize::<RapierContext>(state_context) {
                // commands.insert_resource(context);
                // *rapier = context;

                // Inserting or replacing directly seems to screw up some of the
                // crate-only properties.  So, we'll copy over each public
                // property instead.
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
            }
        }
    }

    // Enable physics pipeline after awhile.
    if game_state.frame > (FPS * LOAD_SECONDS) as i32 && !config.physics_pipeline_active {
        config.physics_pipeline_active = true;
    }

    // Useful for init testing to make sure our checksums always start the same.
    // Exit the app after a few frames
    if game_state.frame > 10 {
        // exit.send(bevy::app::AppExit);
    }
}

pub fn save_game_state(
    mut game_state: ResMut<GameState>,
    rapier: Res<RapierContext>,
    mut hashes: ResMut<FrameHashes>,
    session: Option<Res<P2PSession<GGRSConfig>>>,
) {
    // This serializes our context every frame.  It's not great, but works to
    // integrate the two plugins.  To do less of it, we would need to change
    // bevy_ggrs to serialize arbitrary structs like this one in addition to
    // component tracking.  If you need this to happen less, I'd recommend not
    // using the plugin and implementing GGRS yourself.
    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        game_state.rapier_checksum = fletcher16(&context_bytes);
        game_state.rapier_state = Some(context_bytes);

        // If our session has begun in earnest (it should have) we can store off
        // the checksum information into our hash history.  We'll use this
        // information to compare whenever the opponent sends over their
        // confirmed frame data.
        if let Some(session) = session {
            if let Some(frame_hash) = hashes
                .0
                .get_mut((game_state.frame as usize) % DESYNC_MAX_FRAMES)
            {
                frame_hash.frame = game_state.frame as i32;
                frame_hash.rapier_checksum = game_state.rapier_checksum;
                frame_hash.sent = false;
                frame_hash.validated = false;
                let confirmed_frame = session.confirmed_frame();
                // TODO:  can this be <= ?
                // depends on if confirmed frame is always at highest value or is
                // increased as frames are re-simulated up to that point.
                // I think all we get out of that is more validations?
                // Need to check impl of confirmed_frame to make sure it's worthwhile
                frame_hash.confirmed = frame_hash.frame == confirmed_frame;

                log::info!("confirmed frame: {:?}", confirmed_frame);
                log::info!("Stored frame hash at save: {:?}", frame_hash);
            }
        }

        log::info!("Context hash at save: {}", game_state.rapier_checksum);
        log::info!("----- end frame {} -----", game_state.frame);
    }
}

/// Our desync detector!
/// Validates the hashes we've received so far against the ones we've calculated ourselves.
/// If there is a difference, panic.  Your game will probably want to handle this more gracefully.
pub fn frame_validator(mut hashes: ResMut<FrameHashes>, mut rx_hashes: ResMut<RxFrameHashes>) {
    for (i, rx) in rx_hashes.0.iter_mut().enumerate() {
        // Check every confirmed frame that has not been validated
        if rx.frame > 0 && !rx.validated {
            // Get that same frame in our buffer
            if let Some(sx) = hashes.0.get_mut(i) {
                // Make sure it's the exact same frame and also confirmed and not yet validated
                if sx.frame == rx.frame && sx.confirmed && !sx.validated {
                    // If this is causing your game to exit, you have a bug!
                    assert_eq!(
                        sx.rapier_checksum, rx.rapier_checksum,
                        "Failed checksum checks {:?} != {:?}",
                        sx, rx
                    );
                    log::info!("Frame validated {:?}", sx.frame);
                    // Set both as validated
                    sx.validated = true;
                    rx.validated = true;
                }
            }
        }
    }
}

pub fn apply_inputs(
    mut query: Query<(&mut Velocity, &Player)>,
    inputs: Res<Vec<(GGRSInput, InputStatus)>>,
    mut hashes: ResMut<RxFrameHashes>,
    local_handles: Res<LocalHandles>,
) {
    for (mut v, p) in query.iter_mut() {
        let (game_input, input_status) = inputs[p.handle];
        // Check the desync for this player if they're not a local handle
        // Did they send us some goodies?
        if !local_handles.handles.contains(&p.handle) && game_input.last_confirmed_frame > 0 {
            log::info!("Got frame data {:?}", game_input);
            if let Some(frame_hash) = hashes
                .0
                .get_mut((game_input.last_confirmed_frame as usize) % DESYNC_MAX_FRAMES)
            {
                // Only update this local data if the frame is new-to-us.
                // We don't want to overwrite any existing validated status
                // unless the frame is replacing what is already in the buffer.
                if frame_hash.frame != game_input.last_confirmed_frame {
                    frame_hash.frame = game_input.last_confirmed_frame;
                    frame_hash.rapier_checksum = game_input.last_confirmed_hash;
                    frame_hash.validated = false;
                }
            }
        }

        // On to the boring stuff
        let input = match input_status {
            InputStatus::Confirmed => game_input.input,
            InputStatus::Predicted => game_input.input,
            InputStatus::Disconnected => 0, // disconnected players do nothing
        };

        if input > 0 {
            // Useful for desync observing
            log::info!("input {:?} from {}: {}", input_status, p.handle, input)
        }

        let horizontal = if input & INPUT_LEFT != 0 && input & INPUT_RIGHT == 0 {
            -1.
        } else if input & INPUT_LEFT == 0 && input & INPUT_RIGHT != 0 {
            1.
        } else {
            0.
        };

        let vertical = if input & INPUT_DOWN != 0 && input & INPUT_UP == 0 {
            -1.
        } else if input & INPUT_DOWN == 0 && input & INPUT_UP != 0 {
            1.
        } else {
            0.
        };

        if horizontal != 0. {
            v.linvel.x += horizontal * 10.0;
        } else {
            v.linvel.x = 0.;
        }

        if vertical != 0. {
            v.linvel.y += vertical * 10.0;
        } else {
            v.linvel.y = 0.;
        }
    }
}

pub fn update_matchbox_socket(commands: Commands, mut socket_res: ResMut<Option<WebRtcSocket>>) {
    if let Some(socket) = socket_res.as_mut() {
        socket.accept_new_connections();
        if socket.players().len() >= NUM_PLAYERS {
            // take the socket
            let socket = socket_res.as_mut().take().unwrap();
            create_ggrs_session(commands, socket);
        }
    }
}

fn create_ggrs_session(mut commands: Commands, socket: WebRtcSocket) {
    // create a new ggrs session
    let mut session_build = SessionBuilder::<GGRSConfig>::new()
        .with_num_players(NUM_PLAYERS)
        .with_max_prediction_window(MAX_PREDICTION)
        .with_fps(FPS)
        .expect("Invalid FPS")
        .with_input_delay(INPUT_DELAY)
        // Sparse saving should be off since we are serializing every frame
        // anyway.  With it on, it seems that there are going to be more frames
        // in between rollbacks and that can lead to more inaccuracies building
        // up over time.
        .with_sparse_saving_mode(false);

    // add players
    let mut handles = Vec::new();
    for (i, player_type) in socket.players().iter().enumerate() {
        if *player_type == PlayerType::Local {
            handles.push(i);
        }
        session_build = session_build
            .add_player(player_type.clone(), i)
            .expect("Invalid player added.");
    }

    // start the GGRS session
    let session = session_build
        .start_p2p_session(socket)
        .expect("Session could not be created.");

    commands.insert_resource(session);
    commands.insert_resource(LocalHandles { handles });

    // bevy_ggrs uses this to know when to start
    commands.insert_resource(SessionType::P2PSession);
}

fn print_events_system(session: Option<ResMut<P2PSession<GGRSConfig>>>) {
    if let Some(mut session) = session {
        for event in session.events() {
            println!("GGRS Event: {:?}", event);
        }
    }
}

fn print_network_stats_system(
    time: Res<Time>,
    mut timer: ResMut<NetworkStatsTimer>,
    session: Option<Res<P2PSession<GGRSConfig>>>,
) {
    // print only when timer runs out
    if timer.0.tick(time.delta()).just_finished() {
        if let Some(session) = session {
            let num_players = session.num_players() as usize;
            for i in 0..num_players {
                if let Ok(stats) = session.network_stats(i) {
                    println!("NetworkStats for player {}: {:?}", i, stats);
                }
            }
        }
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
