use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade}, Query,
    },
    response::IntoResponse,
    routing::get,
    Router, Extension, http::StatusCode,
};
use commands::Command;
use serde_json::{self, json};
use tokio::sync::broadcast::{Receiver, Sender, channel};
use std::{net::SocketAddr, sync::Arc, collections::HashMap};
use tower_http::{
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod commands;
struct State{
    rx_collar: Receiver<String>,
    tx_collar: Sender<String>,
    tx_requester: Sender<String>,
    rx_requester: Receiver<String>,
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
    // We store 5 Messages just in case its not consumed instantly.
    let (tx, rx1) =  channel::<String>(5);
    let (tx_collar, rx_requester) =  channel::<String>(1); // One should be enough
    let state= State{
        rx_collar: rx1,
        tx_collar,
        tx_requester: tx,
        rx_requester,
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
    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
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
            Ok(command) => match socket.send( Message::Text(command)).await{
                Ok(_) => {
                    state.tx_collar.send("Send to her\t".to_string()).unwrap();
                    continue
                },
                Err(_) => {
                    state.tx_collar.send("Nu sheee offline ask her to restart. She loves you <3\t".to_string()).unwrap();
                    print!("byeee");
                    match socket.close().await{
                        Ok(_) => return,
                        Err(_) => {
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
                        state.tx_requester.send(json!({
                            "mode": cmd.mode.as_num(),
                            "level": cmd.level,
                            "duration": cmd.duration
                        }).to_string()).unwrap();
                        let msg = state.rx_requester.resubscribe().recv().await.unwrap();
                        if socket
                            .send(Message::Text(msg+ &req_reply))
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
