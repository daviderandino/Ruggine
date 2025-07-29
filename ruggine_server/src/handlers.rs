use crate::models::{
    CreateGroupPayload, Group, InviteToGroupPayload, RegisterUserPayload, User, WsClientMessage,
    WsServerMessage, LoginPayload, LoginResponse, Claims,
};
use crate::{AppState, ChatState};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use futures_util::{stream::StreamExt, SinkExt};
use jsonwebtoken::{encode, EncodingKey, Header};
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

// --- Gestione Utenti ---

// Modificato per gestire la password
pub async fn register_user(
    State(app_state): State<AppState>,
    Json(payload): Json<RegisterUserPayload>,
) -> Result<Json<User>, (StatusCode, String)> {
    if payload.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Password must be at least 8 characters long.".to_string(),
        ));
    }

    let password_hash = hash(payload.password, DEFAULT_COST).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to hash password.".to_string(),
        )
    })?;

    sqlx::query_as!(
        User,
        "INSERT INTO users (username, password_hash) VALUES ($1, $2) RETURNING id, username, password_hash, created_at",
        payload.username,
        password_hash
    )
    .fetch_one(&app_state.db_pool)
    .await
    .map(Json)
    .map_err(|e| {
        if let Some(db_err) = e.as_database_error() {
            if db_err.is_unique_violation() {
                return (StatusCode::CONFLICT, "Username already exists.".to_string());
            }
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
    })
}

// Nuovo handler per il login
pub async fn login_user(
    State(app_state): State<AppState>,
    Json(payload): Json<LoginPayload>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let user = sqlx::query_as!(
        User,
        "SELECT id, username, password_hash, created_at FROM users WHERE username = $1",
        payload.username
    )
    .fetch_optional(&app_state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "Invalid username or password.".to_string(),
        )
    })?;

    let is_valid = verify(&payload.password, &user.password_hash)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to verify password.".to_string(),
            )
        })?;

    if !is_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid username or password.".to_string(),
        ));
    }

    let now = Utc::now();
    let claims = Claims {
        sub: user.id,
        iat: now.timestamp(),
        exp: (now + Duration::days(1)).timestamp(), // Token valido per 1 giorno
        username: user.username,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(app_state.jwt_secret.as_ref()),
    )
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create token".to_string(),
        )
    })?;

    Ok(Json(LoginResponse { token }))
}

pub async fn get_user_by_username(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<User>, (StatusCode, String)> {
    // La query deve essere aggiornata per includere la colonna password_hash
    sqlx::query_as!(
        User,
        "SELECT id, username, password_hash, created_at FROM users WHERE username = $1",
        username
    )
    .fetch_optional(&app_state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .ok_or_else(|| (StatusCode::NOT_FOUND, "User not found".to_string()))
}

// --- Gestione Gruppi ---
// (Il resto del file da qui in poi rimane invariato)
pub async fn create_group(
    State(app_state): State<AppState>,
    Json(payload): Json<CreateGroupPayload>,
) -> Result<Json<Group>, (StatusCode, String)> {
    let mut tx = app_state.db_pool.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start transaction: {}", e),
        )
    })?;

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

    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to commit transaction: {}", e),
        )
    })?;

    Ok(Json(new_group))
}

pub async fn get_group_by_name(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Group>, (StatusCode, String)> {
    sqlx::query_as!(Group, "SELECT * FROM groups WHERE name = $1", name)
        .fetch_optional(&app_state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Group not found".to_string()))
}

pub async fn invite_to_group(
    State(app_state): State<AppState>,
    Path(group_id): Path<Uuid>,
    Json(payload): Json<InviteToGroupPayload>,
) -> Result<StatusCode, (StatusCode, String)> {
    if payload.inviter_id == payload.user_to_invite_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "A user cannot invite themselves.".to_string(),
        ));
    }

    let mut tx = app_state.db_pool.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
    })?;

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
            tx.commit().await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to commit transaction: {}", e),
                )
            })?;
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
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

// --- Gestione WebSocket ---

pub async fn chat_handler(
    ws: WebSocketUpgrade,
    State(app_state): State<AppState>,
    Path(group_id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id_str = params.get("user_id").cloned().unwrap_or_default();
    let user_id = Uuid::parse_str(&user_id_str).unwrap_or_default();
    
    // NOTA: In un'applicazione reale, l'autenticazione per il WebSocket
    // dovrebbe avvenire tramite il token JWT, non passando l'user_id come parametro.
    // Per semplicità, questa parte non è stata modificata.

    ws.on_upgrade(move |socket| {
        handle_socket(
            socket,
            app_state.db_pool,
            app_state.chat_state,
            group_id,
            user_id,
        )
    })
}

async fn handle_socket(
    socket: WebSocket,
    db_pool: PgPool,
    chat_state: ChatState,
    group_id: Uuid,
    user_id: Uuid,
) {
    let tx = chat_state
        .entry(group_id)
        .or_insert_with(|| broadcast::channel(100).0)
        .clone();
    
    let mut rx = tx.subscribe();

    let username = sqlx::query_scalar!("SELECT username FROM users WHERE id = $1", user_id)
        .fetch_one(&db_pool)
        .await
        .unwrap_or_else(|_| "Sconosciuto".to_string());

    let (mut sender, mut receiver) = socket.split();

    let recv_username = username.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let msg: WsClientMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let server_msg = WsServerMessage {
                sender_id: user_id,
                sender_username: recv_username.clone(),
                content: msg.content,
            };

            if tx.send(serde_json::to_string(&server_msg).unwrap()).is_err() {
                break;
            }
        }
    });

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut recv_task) => send_task.abort(),
        _ = (&mut send_task) => recv_task.abort(),
    };

    if chat_state.get(&group_id).map(|entry| entry.receiver_count()) == Some(0) {
        tracing::info!(
            "Nessun utente rimasto nel gruppo {}, pulizia canale.",
            group_id
        );
        chat_state.remove(&group_id);
    }
}