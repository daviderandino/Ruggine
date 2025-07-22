use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{InviteToGroupPayload, CreateGroupPayload, Group, RegisterUserPayload, User};

// --- Gestione Utenti ---

/// Handler per registrare un nuovo utente.
/// POST /users/register
pub async fn register_user(
    State(db_pool): State<PgPool>,
    Json(payload): Json<RegisterUserPayload>,
) -> Result<Json<User>, (StatusCode, String)> {
    let new_user = sqlx::query_as!(
        User,
        "INSERT INTO users (username) VALUES ($1) RETURNING *",
        payload.username
    )
    .fetch_one(&db_pool)
    .await
    .map_err(|e| {
        // Gestisce il caso di username duplicato
        (StatusCode::CONFLICT, format!("Failed to create user: {}", e))
    })?;

    Ok(Json(new_user))
}


// --- Gestione Gruppi ---

/// Handler per creare un nuovo gruppo.
/// L'utente che lo crea ne diventa automaticamente membro.
/// POST /groups
pub async fn create_group(
    State(db_pool): State<PgPool>,
    Json(payload): Json<CreateGroupPayload>,
) -> Result<Json<Group>, (StatusCode, String)> {
    // Usiamo una transazione per assicurare che entrambe le operazioni (creazione gruppo e aggiunta membro)
    // vengano eseguite con successo o nessuna delle due.
    let mut tx = db_pool.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start transaction: {}", e),
        )
    })?;

    // 1. Crea il gruppo
    let new_group = sqlx::query_as!(
        Group,
        "INSERT INTO groups (name) VALUES ($1) RETURNING *",
        payload.name
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create group: {}", e),
        )
    })?;

    // 2. Aggiungi il creatore come membro del gruppo
    sqlx::query!(
        "INSERT INTO group_members (user_id, group_id) VALUES ($1, $2)",
        payload.creator_id,
        new_group.id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to add creator to group: {}", e),
        )
    })?;

    // Finalizza la transazione
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to commit transaction: {}", e),
        )
    })?;

    Ok(Json(new_group))
}

/// Handler per invitare un utente in un gruppo.
/// POST /groups/{groupId}/invite
/// POST /groups/{groupId}/invite
pub async fn invite_to_group(
    State(db_pool): State<PgPool>,
    Path(group_id): Path<Uuid>,
    Json(payload): Json<InviteToGroupPayload>,
) -> Result<StatusCode, (StatusCode, String)> {
    // L'utente non può auto-invitarsi
    if payload.inviter_id == payload.user_to_invite_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "A user cannot invite themselves.".to_string(),
        ));
    }

    // Iniziamo una transazione per eseguire i controlli e l'inserimento in modo atomico
    let mut tx = db_pool.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
    })?;

    // 1. Controlla che chi invita sia effettivamente un membro del gruppo
    let is_inviter_a_member: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = $1 AND group_id = $2)",
    )
    .bind(payload.inviter_id)
    .bind(group_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !is_inviter_a_member.0 {
        return Err((
            StatusCode::FORBIDDEN,
            "Only group members can send invites.".to_string(),
        ));
    }

    // 2. Controlla che l'utente invitato non sia già un membro
    let is_already_member: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = $1 AND group_id = $2)",
    )
    .bind(payload.user_to_invite_id)
    .bind(group_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_already_member.0 {
        return Err((
            StatusCode::CONFLICT,
            "User is already a member of this group.".to_string(),
        ));
    }

    // 3. Crea l'invito nel database.
    // Se esiste già un invito (violazione del vincolo UNIQUE), la query fallirà.
    let result = sqlx::query!(
        r#"
        INSERT INTO group_invitations (group_id, inviter_id, invited_user_id)
        VALUES ($1, $2, $3)
        "#,
        group_id,
        payload.inviter_id,
        payload.user_to_invite_id
    )
    .execute(&mut *tx)
    .await;

    match result {
        Ok(_) => {
            // Se tutto va bene, conferma la transazione
            tx.commit().await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to commit transaction: {}", e),
                )
            })?;
            // L'invito è stato creato con successo
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            // Se c'è un errore (es. utente non trovato o invito duplicato)
            if let Some(db_err) = e.as_database_error() {
                if db_err.is_unique_violation() {
                    return Err((StatusCode::CONFLICT, "An invitation for this user to this group already exists.".to_string()));
                }
                if db_err.is_foreign_key_violation() {
                    return Err((StatusCode::NOT_FOUND, "The user or group specified does not exist.".to_string()));
                }
            }
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}