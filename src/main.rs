use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

mod db;
mod handlers;
mod models;

#[tokio::main]
async fn main() {
    // Carica le variabili d'ambiente dal file .env
    dotenvy::dotenv().expect("Failed to read .env file");

    // Inizializza il sistema di logging
    initialize_logging();
    tracing::info!("Logging system initialized.");

    // Avvia il task di logging per la CPU in background
    tokio::spawn(log_cpu_usage());

    // Crea il pool di connessioni al database
    let db_pool = db::create_db_pool()
        .await
        .expect("Failed to create database pool");
    tracing::info!("Database pool created successfully.");

    // Definisci il router delle API
    let app = Router::new()
        .route("/users/register", post(handlers::register_user))
        .route("/groups", post(handlers::create_group))
        .route("/groups/:group_id/invite", post(handlers::invite_to_group))
        // Endpoint per WebSocket (logica da implementare)
        .route("/groups/:group_id/chat", get(|| async { "WebSocket endpoint placeholder" }))
        .with_state(db_pool); // Condivide il pool di connessioni con gli handler

    // Avvia il server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Inizializza il sistema di logging `tracing`.
/// Logga sia sulla console che su un file.
fn initialize_logging() {
    let file_appender = tracing_appender::rolling::daily("logs", "ruggine_server.log");
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(fmt::Layer::new().with_writer(std::io::stdout)) // Log su console
        .with(fmt::Layer::new().with_writer(non_blocking_writer)) // Log su file
        .init();
}

/// Task asincrono che logga l'uso della CPU ogni 2 minuti.
async fn log_cpu_usage() {
    let mut interval = time::interval(Duration::from_secs(120));
    loop {
        interval.tick().await;
        match sys_info::loadavg() {
            Ok(load) => {
                tracing::info!(
                    "CPU USAGE (load average): 1m={:.2} 5m={:.2} 15m={:.2}",
                    load.one,
                    load.five,
                    load.fifteen
                );
            }
            Err(e) => {
                tracing::error!("Failed to get CPU load average: {}", e);
            }
        }
    }
}