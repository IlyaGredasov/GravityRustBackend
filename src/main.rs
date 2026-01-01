mod handlers;
mod routes;
mod space_computation;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use axum::{
    Router,
    http::{Request, Response},
    routing::post,
    serve,
};
use socketioxide::SocketIo;
use space_computation::Simulation;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{Span, info, info_span};

type UserId = String;
pub(crate) struct SimulationExecutionPool {
    pub simulation: Arc<Mutex<Simulation>>,
    pub thread: JoinHandle<()>,
    pub stop_flag: Arc<AtomicBool>,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub pools: Arc<Mutex<HashMap<UserId, SimulationExecutionPool>>>,
    pub sockets: Arc<Mutex<HashMap<UserId, socketioxide::extract::SocketRef>>>,
}

pub(crate) fn stop_execution_pool(state: &AppState, user_id: &str) {
    let mut map = state.pools.lock().unwrap();
    if let Some(pool) = map.remove(user_id) {
        pool.stop_flag.store(true, Ordering::Relaxed);
        let _ = pool.thread.join();
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let (socket_layer, io) = SocketIo::new_layer();
    let state = AppState {
        pools: Arc::new(Mutex::new(HashMap::new())),
        sockets: Arc::new(Mutex::new(HashMap::new())),
    };

    handlers::configure_socket_io(&io, state.clone());
    let app = Router::new()
        .route("/launch_simulation", post(routes::launch_simulation))
        .route("/delete_simulation", post(routes::delete_simulation))
        .with_state(state.clone())
        .layer(socket_layer)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &Request<_>| {
                    info_span!("request", method = %req.method(), path = %req.uri().path())
                })
                .on_request(|_req: &Request<_>, _span: &Span| {
                    info!("--> request started");
                })
                .on_response(|_res: &Response<_>, _latency: Duration, _span: &Span| {
                    info!("<-- response sent");
                })
        )
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = "0.0.0.0:5000".parse().unwrap();
    println!("Listening on http://{}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state))
        .await
        .unwrap();
}

async fn shutdown_signal(state: AppState) {
    let _ = tokio::signal::ctrl_c().await;
    let user_ids = {
        let pools = state.pools.lock().unwrap();
        pools.keys().cloned().collect::<Vec<_>>()
    };
    for user_id in user_ids {
        stop_execution_pool(&state, &user_id);
    }
}
