// Modules
mod commands;
mod messages;
mod utils;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use serde_json::json;
use tracing::Level;

use crate::messages::Message as InternalMessage;
use futures::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc
};
use tokio::{sync::broadcast::channel, time::interval};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tokio::time::Duration;
#[tokio::main]
async fn main() {
    // Initialize the config
    let config = utils::Config::new();

    tracing_subscriber::registry()
        .with(EnvFilter::new(&format!(
            "collar_rs_server={}",
            config.log_level
        )))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::event!(target: "collar_rs_server",Level::TRACE,"Opening channels");
    // We store 5 Messages just in case its not consumed instantly.
    let (tx, rx1) = channel::<commands::Command>(5);
    let (tx_collar, rx_requester) = channel::<InternalMessage>(1); // One should be enough
    let (tx_collar_read, rx_collar_write) = channel::<InternalMessage>(1); // One should be enough
    tracing::event!(target: "collar_rs_server",Level::TRACE,"creating state");
    drop(rx1);
    let state = utils::State {
        tx_collar,
        tx_requester: tx,
        rx_requester,
        key_collar: config.key_collar.clone(),
        key_him: config.key_him.clone(),
        rx_collar_write,
        tx_collar_read,
    };

    tracing::event!(target: "collar_rs_server",Level::TRACE,"creating router with layers");
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
    tracing::event!(target: "collar_rs_server",Level::DEBUG,"Going live on {}", addr);
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
    tracing::event!(target: "collar_rs_server",Level::TRACE,"Incoming request to open websocket as master");
    if params.contains_key("key") {
        if params.get("key").unwrap().as_str() == state.key_him {
            tracing::event!(target: "collar_rs_server",Level::DEBUG,"Master authenticated and is now online");
            ws.on_upgrade(move |socket| handle_controlling_socket(socket, state.clone()))
        } else {
            tracing::event!(target: "collar_rs_server",Level::INFO,"unauthorized master access denied");
            StatusCode::UNAUTHORIZED.into_response()
        }
    } else {
        tracing::event!(target: "collar_rs_server",Level::INFO,"unauthorized master access denied");
        StatusCode::BAD_REQUEST.into_response()
    }
}
async fn ws_collar(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<utils::State>>,
) -> impl IntoResponse {
    tracing::event!(target: "collar_rs_server",Level::TRACE,"Incoming request to open websocket as pet");
    if params.contains_key("key") {
        if params.get("key").unwrap().as_str() == state.key_collar {
            tracing::event!(target: "collar_rs_server",Level::DEBUG,"Pet authenticated and is now online");
            state
                .tx_collar
                .send(InternalMessage::new(
                    201,
                    messages::MessageTyp::PetOnline,
                    messages::ActionType::None,
                ))
                .unwrap();
            ws.on_upgrade(move |socket| handle_collar_socket(socket, state.clone()))
        } else {
            tracing::event!(target: "collar_rs_server",Level::INFO,"unauthorized pet access denied");
            StatusCode::UNAUTHORIZED.into_response()
        }
    } else {
        tracing::event!(target: "collar_rs_server",Level::INFO,"unauthorized pet access denied");
        StatusCode::BAD_REQUEST.into_response()
    }
}

async fn handle_collar_socket(socket: WebSocket, state: Arc<utils::State>) {
    // use this socket to forward valid commands (from the regular socket handler) to the controller board
    let (sender, receiver) = socket.split();
    let mut write_task = tokio::spawn(collar_write(sender, state.clone()));
    let mut send_task = tokio::spawn(collar_read(receiver, state.clone()));
    let keep_alive_task =  {
        let state = state.clone();
        tokio::spawn( async move {
            let mut interval = interval(Duration::from_secs(120));
            loop {
                interval.tick().await;
                tracing::event!(target: "collar_rs_server",Level::DEBUG,"Sending keep alive");
                state.tx_collar.send(InternalMessage::new(200, messages::MessageTyp::KeepAlive, messages::ActionType::None)).unwrap();
                state.tx_requester.send(commands::Command{
                    mode: commands::Modes::Led,
                    level: 50,
                    duration: 500,
                }).unwrap();
                
            }
        })
    };
    // If any one of the tasks exit, abort the other.
    tokio::select! {
        _ = (&mut send_task) =>  {
            tracing::event!(target: "collar_rs_server",Level::TRACE,"Send task failed, aborting write task and exiting");
            write_task.abort();
            keep_alive_task.abort()
        }
        _ = (&mut write_task) =>{
             tracing::event!(target: "collar_rs_server",Level::TRACE,"Write task failed, aborting write task and exiting");
            send_task.abort();
            keep_alive_task.abort()
        },
    };
    // Log what happened and inform the controlling websocket
    tracing::event!(target: "collar_rs_server",Level::DEBUG,"One task exited, aborted both and returning.\nPet reconnection required.");
    state
        .tx_collar
        .send(InternalMessage::new(
            500,
            messages::MessageTyp::PetOffline,
            messages::ActionType::Reconnect,
        ))
        .unwrap();
}
async fn collar_write(mut sender: SplitSink<WebSocket, Message>, state: Arc<utils::State>) {
    loop {
        match state.tx_requester.subscribe().recv().await {
            Ok(command) => match sender
                .send(Message::Text(
                    json!({
                        "mode": command.mode.as_num(),
                        "duration": command.duration,
                        "level": command.level,
                    })
                    .to_string(),
                ))
                .await
            {
                Ok(_) => {
                    tracing::event!(target: "collar_rs_server",Level::DEBUG,"Send command to pet:\n {:?}", command);
                    match state.rx_collar_write.resubscribe().recv().await {
                        Ok(msg) => {
                            match msg.get_type() {
                                messages::MessageTyp::ACK => {
                                    state
                                        .tx_collar
                                        .send(InternalMessage::new(
                                            200,
                                            messages::MessageTyp::ACK,
                                            messages::ActionType::None,
                                        ))
                                        .unwrap();
                                    continue;
                                }
                                _ => continue,
                            };
                        }
                        Err(_) => state
                            .tx_collar
                            .send(InternalMessage::new(
                                200,
                                messages::MessageTyp::PetOnline,
                                messages::ActionType::Reconnect,
                            ))
                            .unwrap(),
                    };
                }
                Err(_) => {
                    tracing::event!(target: "collar_rs_server",Level::DEBUG,
                        "Failed to send command to pet:\n {:?}\n Closing connection",
                        command
                    );
                    state
                        .tx_collar
                        .send(InternalMessage::new(
                            500,
                            messages::MessageTyp::PetOffline,
                            messages::ActionType::Reconnect,
                        ))
                        .unwrap();
                    match sender.close().await {
                        Ok(_) => return,
                        Err(_) => {
                            tracing::event!(target: "collar_rs_server",Level::DEBUG,
                                "Failed to close connection, client reconnection required"
                            );
                            state
                                .tx_collar
                                .send(InternalMessage::new(
                                    500,
                                    messages::MessageTyp::PetUnrecoverableError,
                                    messages::ActionType::RebootReconnect,
                                ))
                                .unwrap();
                            return;
                        }
                    };
                }
            },
            Err(_) => continue,
        }
    }
}
async fn collar_read(mut receiver: SplitStream<WebSocket>, state: Arc<utils::State>) {
    loop {
        match receiver.next().await {
            Some(val) => match val {
                Ok(msg) => match msg {
                    Message::Text(text) => match text.as_str() {
                        "ack" => state
                            .tx_collar_read
                            .send(InternalMessage::new(
                                200,
                                messages::MessageTyp::ACK,
                                messages::ActionType::None,
                            ))
                            .unwrap(),
                        _ => continue,
                    },
                    Message::Close(_) => return,
                    _ => continue,
                },
                Err(_) => return,
            },
            None => {
                state
                            .tx_collar_read
                            .send(InternalMessage::new(
                                200,
                                messages::MessageTyp::PetOffline,
                                messages::ActionType::RebootReconnect,
                            ))
                            .unwrap();
                            return;
            },
        };
    }
}

async fn handle_controlling_socket(socket: WebSocket, state: Arc<utils::State>) {
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
    tracing::event!(target: "collar_rs_server",Level::DEBUG,"Master disconnected.\n");
}
async fn handle_controlling_write(
    mut sender: SplitSink<WebSocket, Message>,
    state: Arc<utils::State>,
) {
    loop {
        match state.rx_requester.resubscribe().recv().await {
            Ok(msg) => {
                if sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap()))
                    .await
                    .is_err()
                {
                    tracing::event!(target: "collar_rs_server",Level::DEBUG,"Client disconnected while trying to send response");
                    return;
                }
            }
            Err(_) => continue,
        }
    }
}

async fn handle_controlling_read(mut receiver: SplitStream<WebSocket>, state: Arc<utils::State>) {
    loop {
        match receiver.next().await {
            Some(msg) => match msg {
                Ok(msg) => match msg {
                    Message::Text(t) => {
                        tracing::event!(target: "collar_rs_server",Level::TRACE,"client sent str: {:?}", &t);
                        // TODO: improve the serde_json error handling: there's currently none at all, RIP socket.
                        let cmd: commands::Command = match serde_json::from_str(&t) {
                            Ok(val) => val,
                            Err(_) => {
                                state
                                    .tx_collar
                                    .send(InternalMessage::new(
                                        400,
                                        messages::MessageTyp::InvalidParams,
                                        messages::ActionType::None,
                                    ))
                                    .unwrap();
                                continue;
                            }
                        };
                        tracing::event!(target: "collar_rs_server",Level::DEBUG,
                            "Request received - mode: {}, str: {}, dur: {}",
                            cmd.mode,
                            cmd.level,
                            cmd.duration
                        );
                        // Check command validity & and send response
                        if !commands::check_validity(cmd.clone()) {
                            tracing::event!(target: "collar_rs_server",Level::TRACE,"invalid command:\n {:?}", &cmd);
                            state
                                .tx_collar
                                .send(InternalMessage::new(
                                    400,
                                    messages::MessageTyp::InvalidCommand,
                                    messages::ActionType::None,
                                ))
                                .unwrap();
                        }

                        tracing::event!(target: "collar_rs_server",Level::TRACE,"sending command to pet websocket");
                        if state.tx_requester.receiver_count() > 0{
                            state.tx_requester.send(cmd).unwrap();
                            tracing::event!(target: "collar_rs_server",Level::TRACE,"Waiting for response from the pet websocket");
                        }else {
                            state.tx_collar.send(InternalMessage::new(400, messages::MessageTyp::PetOffline, messages::ActionType::Reconnect)).unwrap();
                        }
                    }
                    Message::Close(_) => {
                        tracing::debug!("client disconnected");
                        return;
                    }
                    _ => tracing::trace!("Received unexpected message {:?}", msg),
                },
                Err(_) => return,
            },
            None => {
                tracing::debug!("client disconnected");
                return;
            }
        }
    }
}
