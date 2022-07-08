use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade}, Query,
    },
    response::IntoResponse,
    routing::get,
    Router, Extension, http::StatusCode,
};
mod commands;
mod utils;

use serde_json::{self, json};
use tokio::sync::broadcast::channel;
use std::{net::{SocketAddr, IpAddr}, sync::Arc, collections::HashMap};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};


#[tokio::main]
async fn main() {
    // Initialize the config
    let config = utils::Config::new();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| format!("colla-rs={0},tower_http={0}", config.log_level).into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::trace!("Opening channels");
    // We store 5 Messages just in case its not consumed instantly.
    let (tx, rx1) =  channel::<String>(5);
    let (tx_collar, rx_requester) =  channel::<String>(1); // One should be enough
    tracing::trace!("creating state");
    let state= utils::State{
        rx_collar: rx1,
        tx_collar,
        tx_requester: tx,
        rx_requester,
        key_collar: config.key_collar.clone(),
        key_him: config.key_him.clone(),
    };
    
    tracing::trace!("creating router with layers");
    // setup the webserver
    let app = Router::new()
        .route("/ws_requester", get(ws_requester))
        .route("/ws_collar", get(ws_collar))
        .layer(Extension(Arc::new(state)))
        // logging so we can eyeball what's going on
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    // run it with hyper
    let addr = SocketAddr::new(IpAddr::V4(config.interface_ip), 4000);
    tracing::debug!("Going live on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn ws_requester(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<utils::State>>,
) -> impl IntoResponse {
    tracing::trace!("Incoming request to open websocket as master");
    if params.contains_key("key"){
        if params.get("key").unwrap().as_str() == state.key_him{
            tracing::debug!("Master authenticated and is now online");
            ws.on_upgrade(move |socket| {
                handle_controlling_socket(socket, state.clone())
            })
        }else {
            tracing::info!("unauthorized master access denied");
            StatusCode::UNAUTHORIZED.into_response()
        }
    }else {
        tracing::info!("unauthorized master access denied");
        StatusCode::BAD_REQUEST.into_response()
    }
}
async fn ws_collar(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<utils::State>>,
) -> impl IntoResponse {
    tracing::trace!("Incoming request to open websocket as pet");
    if params.contains_key("key"){
        if params.get("key").unwrap().as_str() ==  state.key_collar{
            tracing::debug!("Pet authenticated and is now online");
            ws.on_upgrade(move |socket| {
                handle_collar_socket(socket, state.clone())
            })

        }else {
            tracing::info!("unauthorized pet access denied");
            StatusCode::UNAUTHORIZED.into_response()
        }
    }else {
        tracing::info!("unauthorized pet access denied");
        StatusCode::BAD_REQUEST.into_response()
    }
}

async fn handle_collar_socket(mut socket: WebSocket, state: Arc<utils::State>) {
    // use this socket to forward valid commands (from the regular socket handler) to the controller board
    loop {
        match state.rx_collar.resubscribe().recv().await{
            Ok(command) => match socket.send( Message::Text(command.clone())).await{
                Ok(_) => {
                    tracing::debug!("Send command to pet: {}", command);
                    state.tx_collar.send("Send to her\t".to_string()).unwrap();
                    continue
                },
                Err(_) => {
                    tracing::debug!("Failed to send command to pet:\n {}\n Closing connection", command);
                    state.tx_collar.send("Nu sheee offline ask her to restart. She loves you <3\t".to_string()).unwrap();
                    match socket.close().await{
                        Ok(_) => return,
                        Err(_) => {
                            tracing::debug!("Failed to close connection, client reconnection required");
                            state.tx_collar.send("Nu the pipe broke tell her to restart or u will be angry. She loves you <3\t".to_string()).unwrap();
                            return
                        },
                    };
                },
            },
            Err(_) => continue,
        }
    }
}

async fn handle_controlling_socket(mut socket: WebSocket, state:  Arc<utils::State>){
    loop {
        if let Some(msg) = socket.recv().await {
            if let Ok(msg) = msg {
                match msg {
                    Message::Text(t) => {
                        tracing::trace!("client sent str: {:?}", &t);
                        // TODO: improve the serde_json error handling: there's currently none at all, RIP socket.
                        let cmd: commands::Command = match serde_json::from_str(&t){
                            Ok(val) => val,
                            Err(_) => {
                                socket.send(Message::Text("invalid params".to_string())).await.unwrap();
                                continue;
                            },
                        };
                        tracing::debug!("Request received - mode: {}, str: {}, dur: {}", cmd.mode, cmd.level, cmd.duration);
                        // Check command validity + prepare socket reply
                        let mut req_reply: String = String::from("\n ack");
                        if !commands::check_validity(cmd.clone()) {
                            tracing::trace!("invalid command:\n {:?}", &cmd);
                            req_reply = "\n invalid params".to_string();
                        }

                        tracing::trace!("sending command to pet websocket");
                        state.tx_requester.send(json!({
                            "mode": cmd.mode.as_num(),
                            "level": cmd.level,
                            "duration": cmd.duration
                        }).to_string()).unwrap();
                        tracing::trace!("Waiting for response from the pet websocket");
                        let msg = state.rx_requester.resubscribe().recv().await.unwrap();
                        if socket
                            .send(Message::Text(msg+ &req_reply))
                            .await
                            .is_err()
                        {
                            tracing::debug!("client disconnected while trying to send response");
                            return;
                        }
                    }
                    Message::Close(_) => {
                        tracing::debug!("client disconnected");
                        return;
                    }
                    _ => tracing::trace!("Received unexpected message {:?}", msg)
                }
            } else {
                tracing::debug!("client disconnected");
                return;
            }
        }
    }
}
