mod checksum;

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_ggrs::{GGRSPlugin, Rollback, RollbackIdProvider, SessionType};
use bevy_rapier2d::prelude::*;
use bytemuck::{Pod, Zeroable};
use ggrs::{Config, PlayerType, SessionBuilder};
use ggrs::{InputStatus, PlayerHandle};
use matchbox_socket::WebRtcSocket;

use bevy_inspector_egui::WorldInspectorPlugin;

const NUM_PLAYERS: usize = 2;
const FPS: usize = 60;
const ROLLBACK_SYSTEMS: &str = "rollback_systems";
const CHECKSUM_SYSTEMS: &str = "checksum_systems";
const MAX_PREDICTION: usize = 12;
const INPUT_DELAY: usize = 2;
const CHECK_DISTANCE: usize = 2;

const MATCHBOX_ADDR: &str = "wss://match.gschup.dev";

const INPUT_UP: u16 = 0b00001;
const INPUT_DOWN: u16 = 0b00010;
const INPUT_LEFT: u16 = 0b00100;
const INPUT_RIGHT: u16 = 0b01000;
const INPUT_JUMP: u16 = 0b10000;

pub struct LocalHandles {
    pub handles: Vec<PlayerHandle>,
}

pub struct ConnectData {
    pub lobby_id: String,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Pod, Zeroable)]
pub struct GGRSInput {
    pub inp: u16,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Component)]
pub struct Player {
    pub handle: usize,
}

#[derive(Default, Reflect, Hash, Component, PartialEq)]
#[reflect(Hash, Component, PartialEq)]
pub struct FrameCount {
    pub frame: u32,
}

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = GGRSInput;
    type State = u16;
    type Address = String;
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
        .add_plugins(DefaultPlugins)
        .add_startup_system(startup)
        .add_system(keyboard_input)
        .add_system(bevy::input::system::exit_on_esc_system)
        .add_system(update_matchbox_socket);

    app.add_plugin(RapierDebugRenderPlugin::default())
        .add_plugin(WorldInspectorPlugin::new());

    let physics_plugin = RapierPhysicsPlugin::<NoUserData>::default()
        .with_physics_scale(100.)
        .with_system_setup(false);

    GGRSPlugin::<GGRSConfig>::new()
        .with_update_frequency(FPS)
        .with_input_system(input)
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Velocity>()
        .register_rollback_type::<FrameCount>()
        .register_rollback_type::<checksum::Checksum>() // Required to hash Transform/Velocity
        .with_rollback_schedule(
            Schedule::default()
                .with_stage(
                    ROLLBACK_SYSTEMS,
                    SystemStage::parallel()
                        .with_system(apply_inputs)
                        .with_system(increase_frame_count),
                )
                .with_stage_after(
                    ROLLBACK_SYSTEMS,
                    PhysicsStages::SyncBackend,
                    SystemStage::parallel()
                        .with_system_set(physics_plugin.get_sync_backend_systems()),
                )
                .with_stage_after(
                    PhysicsStages::SyncBackend,
                    PhysicsStages::StepSimulation,
                    SystemStage::parallel()
                        .with_system_set(physics_plugin.get_step_simulation_systems()),
                )
                .with_stage_after(
                    PhysicsStages::StepSimulation,
                    PhysicsStages::Writeback,
                    SystemStage::parallel().with_system_set(physics_plugin.get_writeback_systems()),
                )
                .with_stage_after(
                    PhysicsStages::StepSimulation,
                    CHECKSUM_SYSTEMS,
                    SystemStage::parallel().with_system(checksum::checksum),
                ),
        )
        .build(&mut app);

    // Be sure to setup all four stages
    app.add_stage_before(
        CoreStage::Last,
        PhysicsStages::DetectDespawn,
        SystemStage::parallel().with_system_set(physics_plugin.get_detect_despawn_systems()),
    );

    app.add_plugin(physics_plugin);

    // Make sure to insert a new configuration with fixed timestep mode after configuring the plugin
    app.insert_resource(RapierConfiguration {
        timestep_mode: TimestepMode::Fixed {
            dt: 1. / FPS as f32,
            substeps: 1,
        },
        ..default()
    });

    app.run();
}

pub fn keyboard_input(
    mut commands: Commands,
    keys: Res<Input<KeyCode>>,
    task_pool: Res<IoTaskPool>,
    socket_res: Res<Option<WebRtcSocket>>,
) {
    if keys.just_pressed(KeyCode::C) && socket_res.is_none() {
        let lobby_id = "testing-stuff?next=2";
        let room_url = format!("{MATCHBOX_ADDR}/{lobby_id}");
        let (socket, message_loop) = WebRtcSocket::new(room_url);
        task_pool.spawn(message_loop).detach();
        commands.insert_resource(Some(socket));
    }
}

pub fn startup(mut commands: Commands, mut rip: ResMut<RollbackIdProvider>) {
    commands.insert_resource(FrameCount::default());
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    commands
        .spawn()
        .insert(Name::new("Ball"))
        .insert(Rollback::new(rip.next_id()))
        .insert(Collider::ball(4.))
        .insert(LockedAxes::default())
        .insert(Restitution::coefficient(1.0))
        .insert(RigidBody::Dynamic)
        .insert(Velocity::default())
        .insert(Transform::from_xyz(0., 10., 0.));

    commands
        .spawn()
        .insert(Name::new("Player 1"))
        .insert(Player { handle: 0 })
        .insert(Rollback::new(rip.next_id()))
        .insert(Collider::cuboid(4., 4.))
        .insert(LockedAxes::ROTATION_LOCKED)
        .insert(Restitution::default())
        .insert(RigidBody::Dynamic)
        .insert(Velocity::default())
        .insert(Transform::from_xyz(-10., -50., 0.));

    commands
        .spawn()
        .insert(Name::new("Player 2"))
        .insert(Player { handle: 1 })
        .insert(Rollback::new(rip.next_id()))
        .insert(Collider::cuboid(4., 4.))
        .insert(LockedAxes::ROTATION_LOCKED)
        .insert(Restitution::default())
        .insert(RigidBody::Dynamic)
        .insert(Velocity::default())
        .insert(Transform::from_xyz(10., -50., 0.));

    commands
        .spawn()
        .insert(Name::new("Floor"))
        .insert(Collider::cuboid(200., 4.))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(Transform::from_xyz(0., -200., 0.));

    commands
        .spawn()
        .insert(Name::new("Left Wall"))
        .insert(Collider::cuboid(4., 200.))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(Transform::from_xyz(-200., 0., 0.));

    commands
        .spawn()
        .insert(Name::new("Right Wall"))
        .insert(Collider::cuboid(4., 200.))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(Transform::from_xyz(200., 0., 0.));

    commands
        .spawn()
        .insert(Name::new("Ceiling"))
        .insert(Collider::cuboid(200., 4.))
        .insert(LockedAxes::default())
        .insert(Restitution::default())
        .insert(RigidBody::Fixed)
        .insert(Transform::from_xyz(0., 200., 0.));

    // Make sure we have a socket for later systems
    commands.insert_resource::<Option<WebRtcSocket>>(None);
}

pub fn input(
    _handle: In<PlayerHandle>,
    keyboard_input: Res<Input<KeyCode>>,
    _local_handles: Res<LocalHandles>,
) -> GGRSInput {
    let mut inp: u16 = 0;

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
    if keyboard_input.just_pressed(KeyCode::Back) {
        inp |= INPUT_JUMP;
    }

    GGRSInput { inp }
}

pub fn increase_frame_count(mut frame_count: ResMut<FrameCount>) {
    frame_count.frame += 1;
}

pub fn apply_inputs(
    mut query: Query<(&mut Velocity, &Player)>,
    inputs: Res<Vec<(GGRSInput, InputStatus)>>,
) {
    for (mut v, p) in query.iter_mut() {
        let input = match inputs[p.handle].1 {
            InputStatus::Confirmed => inputs[p.handle].0.inp,
            InputStatus::Predicted => inputs[p.handle].0.inp,
            InputStatus::Disconnected => 0, // disconnected players do nothing
        };

        let horizontal = if input & INPUT_LEFT != 0 && input & INPUT_RIGHT == 0 {
            -1.
        } else if input & INPUT_LEFT == 0 && input & INPUT_RIGHT != 0 {
            1.
        } else {
            0.
        };

        let vertical = if input & INPUT_JUMP != 0 {
            if input & INPUT_DOWN != 0 && input & INPUT_UP == 0 {
                -1.
            } else {
                1.
            }
        } else {
            0.
        };

        if horizontal != 0. {
            v.linvel.x += horizontal * 2.0;
        } else {
            v.linvel.x = 0.;
        }

        if vertical != 0. {
            v.linvel.y = vertical * 100.0;
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
        .with_check_distance(CHECK_DISTANCE);

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
    commands.insert_resource(SessionType::P2PSession);
}
