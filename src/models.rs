use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

// --- Modelli per API REST ---

#[derive(Debug, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Deserialize)]
pub struct RegisterUserPayload {
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