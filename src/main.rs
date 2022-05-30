mod log_plugin;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_ggrs::{GGRSPlugin, Rollback, RollbackIdProvider, SessionType};
use bevy_rapier2d::prelude::*;
use bytemuck::{Pod, Zeroable};
use ggrs::{Config, PlayerType, SessionBuilder};
use ggrs::{InputStatus, PlayerHandle};
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

// TODO: Buy gschup a coffee next time you get the chance
const MATCHBOX_ADDR: &str = "wss://match.gschup.dev";

const INPUT_UP: u8 = 0b0001;
const INPUT_DOWN: u8 = 0b0010;
const INPUT_LEFT: u8 = 0b0100;
const INPUT_RIGHT: u8 = 0b1000;

pub struct LocalHandles {
    pub handles: Vec<PlayerHandle>,
}

pub struct ConnectData {
    pub lobby_id: String,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Pod, Zeroable)]
pub struct GGRSInput {
    pub inp: u8,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Component)]
pub struct Player {
    pub handle: usize,
}

#[derive(Default, Reflect, Hash, Component, PartialEq)]
#[reflect(Hash, Component, PartialEq)]
pub struct LastFrameCount {
    pub frame: u32,
}

#[derive(Default, Reflect, Hash, Component, PartialEq)]
#[reflect(Hash, Component, PartialEq)]
pub struct RandomInput {
    pub on: bool,
}

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = GGRSInput;
    type State = u8;
    type Address = String;
}

#[derive(Default, Reflect, Hash, Component, PartialEq)]
#[reflect(Hash, Component, PartialEq)]
pub struct GameState {
    pub rapier_state: Option<Vec<u8>>,
    pub frame: u32,
    pub rapier_checksum: u16,
}

fn main() {
    let mut app = App::new();

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
        .add_system(bevy::input::system::exit_on_esc_system)
        .add_system(update_matchbox_socket);

    app.add_plugin(RapierDebugRenderPlugin::default());
    app.add_plugin(InspectableRapierPlugin);
    app.add_plugin(WorldInspectorPlugin::default());
    /*
        These are nice but noisy when comparing desync output
       app.add_plugin(bevy_diagnostic::DiagnosticsPlugin::default());
       app.add_plugin(bevy_diagnostic::FrameTimeDiagnosticsPlugin::default());
       app.add_plugin(bevy_diagnostic::EntityCountDiagnosticsPlugin::default());
       app.add_plugin(bevy_diagnostic::LogDiagnosticsPlugin::default());
    */

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
                // It is imperative that this executes first, always.  Yes, I know about .after()
                .with_stage(
                    ROLLBACK_SYSTEMS,
                    SystemStage::parallel().with_system(update_game_state),
                )
                // Add our game logic and systems here.  If it impacts what the
                // physics engine should consider, do it here.
                .with_stage_after(
                    ROLLBACK_SYSTEMS,
                    GAME_SYSTEMS,
                    SystemStage::parallel().with_system(apply_inputs),
                )
                // The next 3 stages are all bevy_rapier stages.  Best to leave these in order.
                .with_stage_after(
                    ROLLBACK_SYSTEMS,
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
    // We don't despawn in this example, but you may want to :)
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
            // Scale of 8 since that's the factor size of our ball & players
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

        // This should work with gravity, too
        // gravity: Vec2::ZERO,

        // Turn off query pipeline since this example does not use it
        query_pipeline_active: false,

        ..default()
    });

    app.run();
}

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
    task_pool: Res<IoTaskPool>,
) {
    // Add a bit more CCD
    rapier.integration_parameters.max_ccd_substeps = 2;

    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        let rapier_checksum = fletcher16(&context_bytes);
        log::info!("Context Hash at startup: {}", rapier_checksum);

        commands.insert_resource(GameState {
            rapier_state: Some(context_bytes),
            rapier_checksum,
            ..default()
        })
    } else {
        commands.insert_resource(GameState::default());
    }

    commands.insert_resource(RandomInput { on: true });
    commands.insert_resource(LastFrameCount::default());
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    commands
        .spawn()
        .insert(Name::new("Ball"))
        .insert(Rollback::new(rip.next_id()))
        .insert(Collider::ball(4.))
        // Allowing rotations seems to increase the chance of a difference in calculation.
        .insert(LockedAxes::ROTATION_LOCKED)
        .insert(Restitution::coefficient(2.0))
        .insert(RigidBody::Dynamic)
        .insert(Velocity::default())
        .insert(Sleeping::default())
        .insert(Ccd::enabled())
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(0., 10., 0.));

    commands
        .spawn()
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
        .spawn()
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
        .spawn()
        .insert(Name::new("Floor"))
        .insert(Collider::cuboid(overlapping_box_length, thickness))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(0., -box_length, 0.));

    commands
        .spawn()
        .insert(Name::new("Left Wall"))
        .insert(Collider::cuboid(thickness, overlapping_box_length))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(-box_length, 0., 0.));

    commands
        .spawn()
        .insert(Name::new("Right Wall"))
        .insert(Collider::cuboid(thickness, overlapping_box_length))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(box_length, 0., 0.));

    commands
        .spawn()
        .insert(Name::new("Ceiling"))
        .insert(Collider::cuboid(overlapping_box_length, thickness))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(GlobalTransform::default())
        .insert(Transform::from_xyz(0., box_length, 0.));

    let corner_position = box_length - thickness + 4.;
    commands
        .spawn()
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
        .spawn()
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
        .spawn()
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
        .spawn()
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

    // Make sure we have a socket for later systems
    // commands.insert_resource::<Option<WebRtcSocket>>(None);
    let lobby_id = "testing-stuff?next=2";
    let room_url = format!("{MATCHBOX_ADDR}/{lobby_id}");
    let (socket, message_loop) = WebRtcSocket::new(room_url);
    task_pool.spawn(message_loop).detach();
    commands.insert_resource(Some(socket));
}

pub fn input(
    _handle: In<PlayerHandle>, // Required by bevy_ggrs
    keyboard_input: Res<Input<KeyCode>>,
    _local_handles: Res<LocalHandles>,
    random: Res<RandomInput>,
) -> GGRSInput {
    let mut inp: u8 = 0;

    if keyboard_input.pressed(KeyCode::W) {
        inp |= INPUT_UP;
    }
    if keyboard_input.pressed(KeyCode::A) {
        inp |= INPUT_LEFT;
    }
    if keyboard_input.pressed(KeyCode::S) {
        inp |= INPUT_DOWN;
    }
    if keyboard_input.pressed(KeyCode::D) {
        inp |= INPUT_RIGHT;
    }

    if inp == 0 && random.on {
        let mut rng = thread_rng();
        // Return a random input sometimes, or maybe nothing.
        // Helps to trigger input-based rollbacks from the unplayed side
        match rng.gen_range(0..6) {
            0 => inp = INPUT_UP,
            1 => inp = INPUT_LEFT,
            2 => inp = INPUT_DOWN,
            3 => inp = INPUT_RIGHT,
            _ => (),
        }
    }

    GGRSInput { inp }
}

pub fn update_game_state(
    mut game_state: ResMut<GameState>,
    mut last_frame_count: ResMut<LastFrameCount>,
    mut rapier: ResMut<RapierContext>,
    mut transforms: Query<&mut Transform, With<Rollback>>,
    mut velocities: Query<&mut Velocity, With<Rollback>>,
    mut sleepings: Query<&mut Sleeping, With<Rollback>>,
    mut exit: EventWriter<AppExit>,
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
    last_frame_count.frame = game_state.frame;

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
            "Context Hash at frame {}: {}   {}",
            game_state.frame,
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

    // Only restore our state if we are in a rollback.  This *shouldn't* matter,
    // but does save us a step every frame.
    if is_rollback {
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

    // Useful for init testing to make sure our checksums always start the same.
    // Exit the app after a few frames
    if game_state.frame > 10 {
        //exit.send(AppExit);
    }
}

pub fn save_game_state(mut game_state: ResMut<GameState>, rapier: Res<RapierContext>) {
    // This serializes our context every frame.  It's not great, but works to
    // integrate the two plugins.  To do less of it, we would need to change
    // bevy_ggrs to serialize arbitrary structs like this one in addition to
    // component tracking.  If you need this to happen less, I'd recommend not
    // using the plugin and implementing GGRS yourself.
    if let Ok(context_bytes) = bincode::serialize(rapier.as_ref()) {
        game_state.rapier_checksum = fletcher16(&context_bytes);
        log::info!(
            "Context Hash after frame {}: {}",
            game_state.frame,
            game_state.rapier_checksum
        );

        game_state.rapier_state = Some(context_bytes);
    }
}

pub fn apply_inputs(
    mut query: Query<(&mut Velocity, &Player)>,
    inputs: Res<Vec<(GGRSInput, InputStatus)>>,
    game_state: Res<GameState>,
) {
    for (mut v, p) in query.iter_mut() {
        let input_status = inputs[p.handle].1;
        let input = match input_status {
            InputStatus::Confirmed => inputs[p.handle].0.inp,
            InputStatus::Predicted => inputs[p.handle].0.inp,
            InputStatus::Disconnected => 0, // disconnected players do nothing
        };

        if input > 0 {
            // Useful for desync observing
            log::info!(
                "input {:?} from {} at frame {}: {}",
                input_status,
                p.handle,
                game_state.frame,
                input
            )
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
