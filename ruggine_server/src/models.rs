use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

// --- Modelli per API REST ---

#[derive(Debug, Serialize, FromRow, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Deserialize)]
pub struct RegisterUserPayload {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginPayload {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User, // Ottimizzazione: restituisce l'utente al login
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: i64,
    pub iat: i64,
    pub username: String,
}

#[derive(Debug, Serialize, FromRow, Clone)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Deserialize)]
pub struct CreateGroupPayload {
    pub name: String,
    // Rimosso creator_id, verrà dal token JWT
}

#[derive(Deserialize)]
pub struct InviteToGroupPayload {
    // Rimosso inviter_id, verrà dal token JWT
    pub user_to_invite_id: Uuid,
}

// --- NUOVI Modelli per Inviti ---

#[derive(Debug, Serialize, FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub group_id: Uuid,
    pub group_name: String,
    pub inviter_username: String,
}

#[derive(sqlx::Type, Debug, PartialEq)]
#[sqlx(type_name = "invitation_status", rename_all = "lowercase")]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Declined,
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