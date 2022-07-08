use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        TypedHeader,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use serde_json;
use std::net::SocketAddr;
use tower_http::{
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod commands;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "colla-rs=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // setup the webserver
    let app = Router::new()
        .route("/ws", get(ws_handler))
        // logging so we can eyeball what's going on
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    // run it with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
) -> impl IntoResponse {
    if let Some(TypedHeader(user_agent)) = user_agent {
        println!("`{}` connected", user_agent.as_str());
        // TODO: check for a specific header that only the controller board has.
        // ... then ws.on_upgrade(handle_controller_socket)
    }

    ws.on_upgrade(handle_socket)
}

async fn handle_controller_socket(mut socket: WebSocket) {
    // use this socket to forward valid commands (from the regular socket handler) to the controller board
    todo!();
}

async fn handle_socket(mut socket: WebSocket) {
    loop {
        if let Some(msg) = socket.recv().await {
            if let Ok(msg) = msg {
                match msg {
                    Message::Text(t) => {
                        println!("client sent str: {:?}", &t);
                        // TODO: improve the serde_json error handling: there's currently none at all, RIP socket.
                        let cmd: commands::Command = serde_json::from_str(&t).unwrap();
                        println!("Request received - mode: {}, str: {}, dur: {}", cmd.mode, cmd.level, cmd.duration);
                        // Check command validity + prepare socket reply
                        let mut req_reply: String = String::from("ack");
                        if !commands::check_validity(cmd) {
                            req_reply = "invalid params".to_string();
                        }
                        // TODO: Send these params to the controller socket before client response
                        // Reply to the socket with the command status
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
