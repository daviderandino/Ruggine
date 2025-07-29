use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

// --- Modelli per API REST ---

#[derive(Debug, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    // La password non viene mai serializzata e inviata al client
    #[serde(skip_serializing)]
    pub password_hash: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Deserialize)]
pub struct RegisterUserPayload {
    pub username: String,
    pub password: String, // Aggiunto campo password
}

// Nuovo payload per il login
#[derive(Deserialize)]
pub struct LoginPayload {
    pub username: String,
    pub password: String,
}

// Nuova risposta per il login, che contiene il token
#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
}

// Claims JWT
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,   // Subject (user_id)
    pub exp: i64,    // Expiration time
    pub iat: i64,    // Issued at
    pub username: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Deserialize)]
pub struct CreateGroupPayload {
    pub name: String,
    pub creator_id: Uuid,
}

#[derive(Deserialize)]
pub struct InviteToGroupPayload {
    pub inviter_id: Uuid,
    pub user_to_invite_id: Uuid,
}

// --- Modelli per WebSocket ---

#[derive(Deserialize)]
pub struct WsClientMessage {
    pub content: String,
}

#[derive(Serialize, Clone)]
pub struct WsServerMessage {
    pub sender_id: Uuid,
    pub sender_username: String,
    pub content: String,
}