use crate::prelude::*;

#[derive(Default, Resource)]
pub struct WebRtcSocketWrapper(pub Option<WebRtcSocket>);

/// Not necessary for this demo, but useful debug output sometimes.
#[derive(Resource)]
pub struct NetworkStatsTimer(pub Timer);

pub fn connect(mut commands: Commands) {
    // Connect immediately.
    // This starts to poll the matchmaking service for our other player to connect.
    let (socket, message_loop) = WebRtcSocket::new(MATCHBOX_ADDR);
    let task_pool = IoTaskPool::get();
    task_pool.spawn(message_loop).detach();
    commands.insert_resource(WebRtcSocketWrapper(Some(socket)));
}

pub fn update_matchbox_socket(commands: Commands, mut socket_res: ResMut<WebRtcSocketWrapper>) {
    if let Some(socket) = socket_res.0.as_mut() {
        socket.accept_new_connections(); // needs mut
        if socket.players().len() >= NUM_PLAYERS {
            // take the socket
            let socket = socket_res.0.take().unwrap();
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
