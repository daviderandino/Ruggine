use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

// Modello per la tabella 'users'
#[derive(Debug, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

// Struttura per il corpo della richiesta di registrazione utente
#[derive(Deserialize)]
pub struct RegisterUserPayload {
    pub username: String,
}

// Modello per la tabella 'groups'
#[derive(Debug, Serialize, FromRow)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

// Struttura per il corpo della richiesta di creazione gruppo
#[derive(Deserialize)]
pub struct CreateGroupPayload {
    pub name: String,
    pub creator_id: Uuid,
}

// Struttura per il corpo della richiesta di invito a un gruppo
#[derive(Deserialize)]
pub struct InviteToGroupPayload {
    pub inviter_id: Uuid,
    pub user_to_invite_id: Uuid,
}

// Modello per la tabella 'messages' (per ora non serve per le API REST)
#[derive(Debug, Serialize, FromRow)]
pub struct Message {
    pub id: Uuid,
    pub group_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}