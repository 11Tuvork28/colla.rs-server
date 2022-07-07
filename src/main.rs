use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        TypedHeader, Query,
    },
    response::IntoResponse,
    routing::get,
    Router, Extension, http::StatusCode,
};
use commands::Command;
use serde_json::{self, json};
use tokio::sync::broadcast::{Receiver, Sender, channel};
use std::{net::SocketAddr, sync::Arc, collections::HashMap, fmt::Debug};
use tower_http::{
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod commands;
struct State{
    rx_collar: Receiver<Command>,
    tx_requester: Sender<Command>
}
#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "colla-rs=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let (tx, mut rx1) =  channel::<Command>(1);
    let state= State{
        rx_collar: rx1,
        tx_requester: tx
    };
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
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn ws_requester(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<State>>,
) -> impl IntoResponse {
    if params.contains_key("key"){
        if params.get("key").unwrap().as_str() == "dfa0a405bc7cbb213861337e61f7dc979082765e"{
            ws.on_upgrade(move |socket| {
                handle_socket(socket, state.clone())
            })
        }else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }else {
        StatusCode::BAD_REQUEST.into_response()
    }
}
async fn ws_collar(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<State>>,
) -> impl IntoResponse {
    if params.contains_key("key"){
        if params.get("key").unwrap().as_str() == "1fa8d152446a06ae3a88bfde5e20ce145100c204"{
            ws.on_upgrade(move |socket| {
                handle_controller_socket(socket, state.clone())
            })

        }else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }else {
        StatusCode::BAD_REQUEST.into_response()
    }
}

async fn handle_controller_socket(mut socket: WebSocket, state: Arc<State>) {
    // use this socket to forward valid commands (from the regular socket handler) to the controller board
    loop {
        match state.rx_collar.resubscribe().recv().await{
            Ok(command) => match socket.send( Message::Text(json!({
                "mode": command.mode.as_num(),
                "level": command.level.to_string(),
                "duration": command.duration.to_string()

            }).to_string())).await{
                Ok(_) => continue,
                Err(_) => {
                    print!("byeee");
                    socket.close().await.unwrap();
                    return;
                },
            },
            Err(_) => continue,
        }
    }
}

async fn handle_socket(mut socket: WebSocket, state:  Arc<State>){
    loop {
        if let Some(msg) = socket.recv().await {
            if let Ok(msg) = msg {
                match msg {
                    Message::Text(t) => {
                        println!("client sent str: {:?}", &t);
                        // TODO: improve the serde_json error handling: there's currently none at all, RIP socket.
                        let cmd: commands::Command = match serde_json::from_str(&t){
                            Ok(val) => val,
                            Err(_) => {
                                socket.send(Message::Text("invalid params".to_string())).await.unwrap();
                                continue;
                            },
                        };
                        println!("Request received - mode: {}, str: {}, dur: {}", cmd.mode, cmd.level, cmd.duration);
                        // Check command validity + prepare socket reply
                        let mut req_reply: String = String::from("ack");
                        if !commands::check_validity(cmd.clone()) {
                            req_reply = "invalid params".to_string();
                        }
                        // TODO: Send these params to the controller socket before client response
                        // Reply to the socket with the command status
                        state.tx_requester.send(cmd).unwrap();
                        if socket
                            .send(Message::Text(req_reply))
                            .await
                            .is_err()
                        {
                            println!("client disconnected");
                            return;
                        }
                    }
                    Message::Binary(b) => {
                        println!("client sent binary data: {:?}", &b);
                    }
                    Message::Ping(_) => {
                        println!("socket ping");
                    }
                    Message::Pong(_) => {
                        println!("socket pong");
                    }
                    Message::Close(_) => {
                        println!("client disconnected");
                        return;
                    }
                }
            } else {
                println!("client disconnected");
                return;
            }
        }
    }
}
