use bevy_ggrs::LocalPlayers;
use bevy_matchbox::{
    prelude::{PeerState, SingleChannel},
    MatchboxSocket,
};

use crate::prelude::*;

pub fn connect(mut commands: Commands) {
    // Connect immediately.
    // This starts to poll the matchmaking service for our other player to connect.
    commands.insert_resource(MatchboxSocket::new_ggrs(MATCHBOX_ADDR));
}

pub fn update_matchbox_socket(
    mut commands: Commands,
    mut socket: ResMut<MatchboxSocket<SingleChannel>>,
    session: Option<Res<Session<ExampleGgrsConfig>>>,
) {
    if session.is_some() {
        // Already have a session, skip for now.
        // Check out the bevy_matchbox example which lays out a few ideas on how to better
        // handle this resource w.r.t. an AppState: https://github.com/johanhelsing/matchbox/tree/main/examples/bevy_ggrs
        return;
    }

    // regularly call update_peers to update the list of connected peers
    for (peer, new_state) in socket.update_peers() {
        // you can also handle the specific dis(connections) as they occur:
        match new_state {
            PeerState::Connected => info!("peer {peer:?} connected"),
            PeerState::Disconnected => info!("peer {peer:?} disconnected"),
        }
    }

    // Need one peer
    if socket.connected_peers().count() == 0 {
        return;
    }

    // create a new ggrs session
    let mut session_build = SessionBuilder::<ExampleGgrsConfig>::new()
        .with_num_players(NUM_PLAYERS)
        .with_max_prediction_window(MAX_PREDICTION)
        .expect("Invalid prediction window")
        .with_fps(FPS)
        .expect("Invalid FPS")
        .with_input_delay(INPUT_DELAY)
        // Sparse saving should be off since we are serializing every frame
        // anyway.  With it on, it seems that there are going to be more frames
        // in between rollbacks and that can lead to more inaccuracies building
        // up over time.
        .with_sparse_saving_mode(false)
        .with_desync_detection_mode(bevy_ggrs::ggrs::DesyncDetection::On { interval: 1 });

    // add players
    let players = socket.players();
    let mut handles = Vec::new();
    for (i, player) in players.into_iter().enumerate() {
        if player == PlayerType::Local {
            handles.push(i);
        }
        session_build = session_build
            .add_player(player, i)
            .expect("Invalid player added.");
    }

    // start the GGRS session
    let channel = socket.take_channel(0).unwrap();
    let session = session_build
        .start_p2p_session(channel)
        .expect("Session could not be created.");

    commands.insert_resource(LocalPlayers(handles));

    // bevy_ggrs uses this to know when to start
    commands.insert_resource(Session::P2P(session));
}

pub fn handle_p2p_events(
    session: Option<ResMut<Session<ExampleGgrsConfig>>>,
    mut gizmos: ResMut<GizmoConfigStore>,
) {
    if let Some(mut session) = session {
        if let Session::P2P(session) = session.as_mut() {
            for event in session.events() {
                info!("GGRS Event: {:?}", event);
                match event {
                    GgrsEvent::Disconnected { addr } => {
                        panic!("Other player@{:?} disconnected", addr)
                    }
                    GgrsEvent::DesyncDetected {
                        frame,
                        local_checksum,
                        remote_checksum,
                        addr,
                    } => {
                        gizmos.insert(
                            GizmoConfig::default(),
                            PhysicsGizmos {
                                collider_color: Some(Color::linear_rgb(1., 0., 0.)),
                                ..Default::default()
                            },
                        );
                        // TODO: restore panic
                        error!(
                            "Desync detected on frame {} local {} remote {}@{:?}",
                            frame, local_checksum, remote_checksum, addr
                        );
                    }
                    _ => (),
                }
            }
        }
    }
}
