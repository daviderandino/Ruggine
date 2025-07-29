use axum::{
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use sqlx::PgPool;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

mod db;
mod handlers;
mod models;

pub type ChatState = Arc<DashMap<Uuid, broadcast::Sender<String>>>;

// Struct unica per contenere tutto lo stato dell'applicazione
#[derive(Clone)]
pub struct AppState {
    db_pool: PgPool,
    chat_state: ChatState,
    jwt_secret: String, // Aggiunto segreto per JWT
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().expect("Failed to read .env file");

    initialize_logging();
    tracing::info!("Logging system initialized.");

    tokio::spawn(log_cpu_usage());
    
    // Leggi il segreto JWT dalle variabili d'ambiente
    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");

    // Inizializza gli stati
    let db_pool = db::create_db_pool()
        .await
        .expect("Failed to create database pool");
    tracing::info!("Database pool created successfully.");
    
    let chat_state = ChatState::new(DashMap::new());

    // Crea l'istanza dello stato applicativo
    let app_state = AppState {
        db_pool,
        chat_state,
        jwt_secret,
    };

    // Crea il router con un singolo stato
    let app = Router::new()
        .route("/users/register", post(handlers::register_user))
        .route("/users/login", post(handlers::login_user)) // Nuova rotta per il login
        .route("/users/by_username/:username", get(handlers::get_user_by_username))
        .route("/groups", post(handlers::create_group))
        .route("/groups/by_name/:name", get(handlers::get_group_by_name))
        .route("/groups/:group_id/invite", post(handlers::invite_to_group))
        .route("/groups/:group_id/chat", get(handlers::chat_handler))
        .with_state(app_state); // Passa lo stato unificato

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn initialize_logging() {
    let file_appender = tracing_appender::rolling::daily("logs", "ruggine_server.log");
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(fmt::Layer::new().with_writer(std::io::stdout))
        .with(fmt::Layer::new().with_writer(non_blocking_writer))
        .init();
}

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