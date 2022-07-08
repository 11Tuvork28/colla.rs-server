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
use futures::{sink::SinkExt, stream::{StreamExt, SplitSink, SplitStream}};

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
    let (tx_collar_read, rx_collar_write) =  channel::<String>(1); // One should be enough
    tracing::trace!("creating state");
    let state= utils::State{
        rx_collar: rx1,
        tx_collar,
        tx_requester: tx,
        rx_requester,
        key_collar: config.key_collar.clone(),
        key_him: config.key_him.clone(),
        rx_collar_write,
        tx_collar_read,
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

async fn handle_collar_socket(socket: WebSocket, state: Arc<utils::State>) {
    // use this socket to forward valid commands (from the regular socket handler) to the controller board
    let (sender, receiver) = socket.split();
    let mut write_task = tokio::spawn(collar_write(sender, state.clone()));
    let mut send_task = tokio::spawn(collar_read(receiver, state.clone()));

    // If any one of the tasks exit, abort the other.
    tokio::select! {
        _ = (&mut send_task) =>  {
            tracing::trace!("Send task failed, aborting write task and exiting");
            write_task.abort()
        }
        _ = (&mut write_task) =>{
            tracing::trace!("Write task failed, aborting write task and exiting"); 
            send_task.abort()
        },
    };
    // Log what happened and inform the controlling websocket
    tracing::debug!("One task exited, aborted both and returning.\nPet reconnection required."); 
    state.tx_collar.send("Pet went offline... waiting for reconnect".to_string()).unwrap();
}
async fn collar_write(mut sender: SplitSink<WebSocket, Message>, state: Arc<utils::State>){
    loop {
        match state.rx_collar.resubscribe().recv().await{
            Ok(command) => match sender.send( Message::Text(command.clone())).await{
                Ok(_) => {
                    tracing::debug!("Send command to pet: {}", command);
                    state.tx_collar.send("Send to her\t".to_string()).unwrap();
                    match state.rx_collar_write.resubscribe().recv().await{
                        Ok(msg) => match msg.as_str() {
                            "ack" => continue,
                            _ => return,
                        },
                        Err(_) => return,
                    }
                },
                Err(_) => {
                    tracing::debug!("Failed to send command to pet:\n {}\n Closing connection", command);
                    state.tx_collar.send("Nu sheee offline ask her to restart. She loves you <3\t".to_string()).unwrap();
                    match sender.close().await{
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
async fn collar_read(mut receiver: SplitStream<WebSocket>, state: Arc<utils::State>){
    loop {
        match receiver.next().await{
            Some(val) => match val{
                Ok(msg) => match msg{
                    Message::Text(text) => match text.as_str(){
                        "ack" => state.tx_collar_read.send("Ack".to_string()).unwrap(),
                        _ => continue,
                    },
                    Message::Close(_) => return,
                    _ => continue,
                },
                Err(_) => return
            },
            None => continue,
        };
    }
}

async fn handle_controlling_socket(socket: WebSocket, state:  Arc<utils::State>){
    let (sender, receiver) = socket.split();
    let mut write_task = tokio::spawn(handle_controlling_write(sender, state.clone()));
    let mut send_task = tokio::spawn(handle_controlling_read(receiver, state.clone()));

    // If any one of the tasks exit, abort the other.
    tokio::select! {
        _ = (&mut send_task) =>  {
            tracing::trace!("Send task failed, aborting write task and exiting");
            write_task.abort()
        }
        _ = (&mut write_task) =>{
            tracing::trace!("Write task failed, aborting write task and exiting"); 
            send_task.abort()
        },
    };
    // Log what happened and inform the controlling websocket
    tracing::debug!("Master disconnected.\n"); 
}
async fn handle_controlling_write(mut sender: SplitSink<WebSocket, Message>, state: Arc<utils::State>){
    loop {
        match state.rx_requester.resubscribe().recv().await{
            Ok(msg) => if sender.send(Message::Text(msg))
                .await
                .is_err()
            {
                tracing::debug!("Client disconnected while trying to send response");
                return;
            },
            Err(_) => continue,
        }
    }
}

async fn handle_controlling_read(mut receiver: SplitStream<WebSocket>, state: Arc<utils::State>){
    loop{
       match receiver.next().await {
        Some(msg) =>  match msg {
            Ok(msg) => match msg{
                Message::Text(t) => {
                    tracing::trace!("client sent str: {:?}", &t);
                    // TODO: improve the serde_json error handling: there's currently none at all, RIP socket.
                    let cmd: commands::Command = match serde_json::from_str(&t){
                        Ok(val) => val,
                        Err(_) => {
                            state.tx_collar.send("invalid params".to_string()).unwrap();
                            continue;
                        },
                    };
                    tracing::debug!("Request received - mode: {}, str: {}, dur: {}", cmd.mode, cmd.level, cmd.duration);
                    // Check command validity & and send response
                    if !commands::check_validity(cmd.clone()) {
                        tracing::trace!("invalid command:\n {:?}", &cmd);
                        state.tx_collar.send("invalid command".to_string()).unwrap();
                    }

                    tracing::trace!("sending command to pet websocket");
                    state.tx_requester.send(json!({
                        "mode": cmd.mode.as_num(),
                        "level": cmd.level,
                        "duration": cmd.duration
                    }).to_string()).unwrap();
                    tracing::trace!("Waiting for response from the pet websocket");
                }
                Message::Close(_) => {
                    tracing::debug!("client disconnected");
                    return;
                }
                _ => tracing::trace!("Received unexpected message {:?}", msg),
            },
            Err(_) => return,
        }
        None => {
                tracing::debug!("client disconnected");
                return;
            }
        }
    }
}