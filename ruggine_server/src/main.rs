use axum::{
    routing::{get, post, delete},
    Router,
};
use cpu_time::ProcessTime;
use dashmap::DashMap;
use sqlx::{Pool, Sqlite};
use std::{env, sync::OnceLock};
use sysinfo::{System};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time;
use tracing_subscriber::{filter::FilterExt, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use uuid::Uuid;

// Dichiarazione di tutti i moduli
pub mod auth;
mod db;
mod handlers;
mod models;
pub mod error;

pub type ChatState = Arc<DashMap<Uuid, broadcast::Sender<String>>>;
static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

#[derive(Clone)]
pub struct AppState {
    db_pool: Pool<Sqlite>,
    chat_state: ChatState,
    jwt_secret: String,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().expect("Failed to read .env file");

    initialize_logging();
    tracing::info!("Logging system initialized.");

    tokio::spawn(log_cpu_usage());

    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");

    let db_pool = db::create_db_pool()
        .await
        .expect("Failed to create database pool");
    tracing::info!("Database pool created successfully.");

    let chat_state = ChatState::new(DashMap::new());

    let app_state = AppState {
        db_pool,
        chat_state,
        jwt_secret,
    };

    let app = Router::new()
        .route("/users/register", post(handlers::register_user))
        .route("/users/login", post(handlers::login_user))
        .route(
            "/users/by_username/:username",
            get(handlers::get_user_by_username),
        )
        .route("/groups", post(handlers::create_group))
        .route("/groups/by_name/:name", get(handlers::get_group_by_name))
        .route(
            "/groups/:group_id/messages", // Rotta per la cronologia
            get(handlers::get_group_messages),
        )
        .route("/groups/:group_id/members",get(handlers::get_group_members))
        .route(
            "/groups/:group_id/leave", // <-- AGGIUNGI QUESTA ROTTA
            delete(handlers::leave_group),
        )
        .route("/groups/:group_id/invite", post(handlers::invite_to_group))
        .route("/groups/:group_id/chat", get(handlers::chat_handler))
        .route("/invitations", get(handlers::get_pending_invitations))
        .route(
            "/invitations/:invitation_id/accept",
            post(handlers::accept_invitation),
        )
        .route(
            "/invitations/:invitation_id/decline",
            post(handlers::decline_invitation),
        )
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn initialize_logging() {
    let file_appender = tracing_appender::rolling::daily("logs", "ruggine_server.log");
    let (non_blocking_writer, guard) = tracing_appender::non_blocking(file_appender);

    // Salvo guard nella variabile statica per scrivere nel log file
    LOG_GUARD.set(guard).ok();
    //Cosa stampare in console
    let console_layer = fmt::Layer::new()
        .with_writer(std::io::stdout)
        .with_filter(EnvFilter::new("info"));

    // Cosa stampare nel file
    let file_layer = fmt::Layer::new()
        .with_writer(non_blocking_writer)
        //.with_filter(EnvFilter::new("debug"))
        //.with_filter(EnvFilter::new("trace"))
        .with_filter(EnvFilter::new("info"));



    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .init()
}

async fn log_cpu_usage() {
    let mut interval = time::interval(Duration::from_secs(120));
    let mut sys = System::new_all();
    // Get this process’s PID
    let pid = sysinfo::get_current_pid().expect("failed to get current pid");
    loop{
        let start = ProcessTime::now();
        sys.refresh_process(pid);
        interval.tick().await;
        if let Some(process) = sys.process(pid) {
            tracing::info!(
                "Process {} CPU usage: {:.2}% CPU Time: {:?}",
                process.name(),
                process.cpu_usage(),
                start.elapsed(),
            );
        }
    }
}