use bevy_matchbox::{prelude::SingleChannel, MatchboxSocket};

use crate::prelude::*;

/// Not necessary for this demo, but useful debug output sometimes.
#[derive(Resource)]
pub struct NetworkStatsTimer(pub Timer);

pub fn connect(mut commands: Commands) {
    // Connect immediately.
    // This starts to poll the matchmaking service for our other player to connect.
    commands.insert_resource(MatchboxSocket::new_ggrs(MATCHBOX_ADDR));
}

pub fn update_matchbox_socket(
    mut commands: Commands,
    mut socket_res: Option<ResMut<MatchboxSocket<SingleChannel>>>,
) {
    if let None = socket_res {
        return;
    }

    let socket = socket_res.as_mut().unwrap();
    socket.update_peers();
    if socket.connected_peers().count() < NUM_PLAYERS {
        return;
    }

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

    commands.insert_resource(LocalHandles { handles });

    // bevy_ggrs uses this to know when to start
    commands.insert_resource(Session::P2PSession(session));
}

pub fn handle_p2p_events(session: Option<ResMut<Session<GGRSConfig>>>) {
    if let Some(mut session) = session {
        if let Session::P2PSession(session) = session.as_mut() {
            for event in session.events() {
                info!("GGRS Event: {:?}", event);
                if let ggrs::GGRSEvent::Disconnected { addr: _ } = event {
                    panic!("Other player disconnected");
                }
            }
        }
    }
}
