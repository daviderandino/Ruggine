use crate::error::AppError;
use crate::models::{
    Claims, CreateGroupPayload, Group, Invitation, InviteToGroupPayload, LoginPayload,
    LoginResponse, RegisterUserPayload, User, WsClientMessage, WsServerMessage,
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
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

// --- Gestione Utenti ---

pub async fn leave_group(
    claims: Claims,
    State(app_state): State<AppState>,
    Path(group_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let user_id = claims.sub;
    let username = claims.username; // Prendiamo il nome utente dalle claims del token

    let mut tx = app_state.db_pool.begin().await?;

    let result = sqlx::query!(
        "DELETE FROM group_members WHERE user_id = ? AND group_id = ?",
        user_id,
        group_id
    )
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        tx.commit().await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM group_members WHERE group_id = ?")
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;
    
    // Impegnamo la transazione prima di inviare il messaggio broadcast
    tx.commit().await?;

    // --- INIZIO MODIFICA ---
    // Invia un messaggio di notifica alla chat del gruppo
    if let Some(tx) = app_state.chat_state.get(&group_id) {
        let system_message = WsServerMessage {
            sender_id: Uuid::nil(), // ID speciale per i messaggi di sistema
            sender_username: "system".to_string(), // Non mostrato, ma utile per debug
            content: format!("{} ha lasciato il gruppo.", username),
        };
        // Invia il messaggio, ignorando l'errore se non ci sono più iscritti
        let _ = tx.send(serde_json::to_string(&system_message).unwrap());
    }
    // --- FINE MODIFICA ---

    // Se non ci sono più membri, ora che la notifica è stata inviata, possiamo pulire il gruppo
    if count.0 == 0 {
        sqlx::query!("DELETE FROM groups WHERE id = ?", group_id)
            .execute(&app_state.db_pool)
            .await?;
        tracing::info!("Gruppo {} eliminato perché non ha più membri.", group_id);
        
        // Rimuovi anche lo stato della chat dalla memoria
        app_state.chat_state.remove(&group_id);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn register_user(
    State(app_state): State<AppState>,
    Json(payload): Json<RegisterUserPayload>,
) -> Result<Json<User>, AppError> {
    if payload.password.len() < 8 {
        return Err(AppError::InvalidInput(
            "Password must be at least 8 characters long.".to_string(),
        ));
    }

    let password_hash = hash(payload.password, DEFAULT_COST)?;

    sqlx::query_as!(
        User,
        "INSERT INTO users (username, password_hash) VALUES (?, ?) RETURNING id as \"id!: uuid::Uuid\", username, password_hash, created_at as \"created_at!: sqlx::types::time::OffsetDateTime\"",
        payload.username,
        password_hash
    )
    .fetch_one(&app_state.db_pool)
    .await
    .map(Json)
    .map_err(|e| {
        if let Some(db_err) = e.as_database_error() {
            if db_err.is_unique_violation() {
                return AppError::UsernameExists;
            }
        }
        e.into()
    })
}

pub async fn login_user(
    State(app_state): State<AppState>,
    Json(payload): Json<LoginPayload>,
) -> Result<Json<LoginResponse>, AppError> {
    let user = sqlx::query_as!(
        User,
        "SELECT id \"id!: uuid::Uuid\", username, password_hash, created_at as \"created_at!: sqlx::types::time::OffsetDateTime\" FROM users WHERE username = ?",
        payload.username
    )
    .fetch_optional(&app_state.db_pool)
    .await?
    .ok_or(AppError::WrongCredentials)?;

    if !verify(&payload.password, &user.password_hash).unwrap_or(false) {
        return Err(AppError::WrongCredentials);
    }

    let user_groups = sqlx::query_as!(
        Group,
        r#"
        SELECT g.id as "id!: uuid::Uuid", g.name, g.created_at as "created_at!: sqlx::types::time::OffsetDateTime"
        FROM groups g
        JOIN group_members gm ON g.id = gm.group_id
        WHERE gm.user_id = ?
        ORDER BY g.created_at ASC
        "#,
        user.id
    )
    .fetch_all(&app_state.db_pool)
    .await?;

    let now = Utc::now();
    let claims = Claims {
        sub: user.id,
        iat: now.timestamp(),
        exp: (now + Duration::days(1)).timestamp(),
        username: user.username.clone(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(app_state.jwt_secret.as_ref()),
    )?;

    Ok(Json(LoginResponse {
        token,
        user,
        groups: user_groups,
    }))
}

pub async fn get_user_by_username(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<User>, AppError> {
    sqlx::query_as!(
        User,
        "SELECT id \"id!: uuid::Uuid\", username, password_hash, created_at as \"created_at!: sqlx::types::time::OffsetDateTime\" FROM users WHERE username = ?",
        username
    )
    .fetch_optional(&app_state.db_pool)
    .await?
    .map(Json)
    .ok_or(AppError::UserNotFound)
}

// --- Handler Protetti con Auth ---

pub async fn create_group(
    claims: Claims,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateGroupPayload>,
) -> Result<Json<Group>, AppError> {
    let creator_id = claims.sub;
    let mut tx = app_state.db_pool.begin().await?;

    let new_group = sqlx::query_as!(Group, "INSERT INTO groups (name) VALUES (?) RETURNING
            id          AS \"id!: uuid::Uuid\",
            name,
            created_at  AS \"created_at!: sqlx::types::time::OffsetDateTime\"
        ",
        payload.name)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query!(
        "INSERT INTO group_members (user_id, group_id) VALUES (?, ?)",
        creator_id,
        new_group.id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(new_group))
}

pub async fn invite_to_group(
    claims: Claims,
    State(app_state): State<AppState>,
    Path(group_id): Path<Uuid>,
    Json(payload): Json<InviteToGroupPayload>,
) -> Result<StatusCode, AppError> {
    let inviter_id = claims.sub;

    if inviter_id == payload.user_to_invite_id {
        return Err(AppError::CannotInviteSelf);
    }
    
    let mut tx = app_state.db_pool.begin().await?;

    let is_inviter_a_member: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = ? AND group_id = ?)")
        .bind(inviter_id)
        .bind(group_id)
        .fetch_one(&mut *tx).await?;

    if !is_inviter_a_member.0 {
        return Err(AppError::MissingPermissions);
    }

    let is_already_member: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = ? AND group_id = ?)")
        .bind(payload.user_to_invite_id).bind(group_id)
        .fetch_one(&mut *tx).await?;

    if is_already_member.0 {
        return Err(AppError::UserAlreadyInGroup);
    }

    let result = sqlx::query!(
        "INSERT INTO group_invitations (group_id, inviter_id, invited_user_id, status) VALUES (?, ?, ?, 'pending')",
        group_id, inviter_id, payload.user_to_invite_id
    )
    .execute(&mut *tx).await;
    
    match result {
        Ok(_) => {
            tx.commit().await?;
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            if let Some(db_err) = e.as_database_error() {
                if db_err.is_unique_violation() { return Err(AppError::InvitationAlreadyExists); }
                if db_err.is_foreign_key_violation() { return Err(AppError::UserOrGroupNotFound); }
            }
            Err(e.into())
        }
    }
}

pub async fn get_pending_invitations(
    claims: Claims,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<Invitation>>, AppError> {
    sqlx::query_as!(
        Invitation,
        r#"
        SELECT gi.id as "id!: uuid::Uuid", g.id as "group_id!: uuid::Uuid", g.name as "group_name", u.username as "inviter_username"
        FROM group_invitations gi
        JOIN groups g ON gi.group_id = g.id
        JOIN users u ON gi.inviter_id = u.id
        WHERE gi.invited_user_id = ? AND gi.status = 'pending'
        "#,
        claims.sub
    )
    .fetch_all(&app_state.db_pool)
    .await
    .map(Json)
    .map_err(Into::into)
}

pub async fn accept_invitation(
    claims: Claims,
    State(app_state): State<AppState>,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<Group>, AppError> {
    let user_id = claims.sub;
    let mut tx = app_state.db_pool.begin().await?;

    let invitation = sqlx::query!(
        "SELECT group_id as \"group_id!: uuid::Uuid\" FROM group_invitations WHERE id = ? AND invited_user_id = ? AND status = 'pending'",
        invitation_id, user_id
    )
    .fetch_optional(&mut *tx).await?
    .ok_or(AppError::InvitationNotFound)?;

    sqlx::query!("UPDATE group_invitations SET status = 'accepted' WHERE id = ?", invitation_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query!("INSERT INTO group_members (user_id, group_id) VALUES (?, ?) ON CONFLICT DO NOTHING", user_id, invitation.group_id)
        .execute(&mut *tx)
        .await?;

    let group = sqlx::query_as!(Group, "SELECT id as \"id!: uuid::Uuid\", name, created_at as \"created_at!: sqlx::types::time::OffsetDateTime\" FROM groups WHERE id = ?", invitation.group_id)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(Json(group))
}

pub async fn decline_invitation(
    claims: Claims,
    State(app_state): State<AppState>,
    Path(invitation_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query!(
        "UPDATE group_invitations SET status = 'declined' WHERE id = ? AND invited_user_id = ? AND status = 'pending'",
        invitation_id, claims.sub
    )
    .execute(&app_state.db_pool).await?;

    if result.rows_affected() == 0 {
        return Err(AppError::InvitationNotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- Handler non protetti e WebSocket ---

pub async fn get_group_by_name(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Group>, AppError> {
    sqlx::query_as!(Group, "SELECT id as \"id!: uuid::Uuid\", name, created_at as \"created_at!: sqlx::types::time::OffsetDateTime\" FROM groups WHERE name = ?", name)
        .fetch_optional(&app_state.db_pool)
        .await?
        .map(Json)
        .ok_or(AppError::GroupNotFound)
}

pub async fn get_group_messages(
    claims: Claims,
    State(app_state): State<AppState>,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Vec<WsServerMessage>>, AppError> {
    let is_member: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = ? AND group_id = ?)"
    )
    .bind(claims.sub)
    .bind(group_id)
    .fetch_one(&app_state.db_pool)
    .await?;

    if !is_member.0 {
        return Err(AppError::MissingPermissions);
    }

    // --- INIZIO MODIFICA ---

    // 1. Recupera gli ultimi 100 messaggi in ordine cronologico inverso
    let mut messages = sqlx::query_as!(
        WsServerMessage,
        r#"
        SELECT
            m.user_id as "sender_id!: uuid::Uuid",
            u.username as "sender_username",
            m.content
        FROM group_messages m
        JOIN users u ON m.user_id = u.id
        WHERE m.group_id = ?
        ORDER BY m.created_at DESC
        LIMIT 100
        "#,
        group_id
    )
    .fetch_all(&app_state.db_pool)
    .await?;

    // 2. Inverti la lista per ripristinare l'ordine cronologico corretto (dal più vecchio al più nuovo)
    messages.reverse();

    // --- FINE MODIFICA ---

    Ok(Json(messages))
}

pub async fn chat_handler(
    ws: WebSocketUpgrade,
    State(app_state): State<AppState>,
    Path(group_id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let token = match params.get("token") {
        Some(t) => t,
        None => return (StatusCode::UNAUTHORIZED, "Missing token").into_response(),
    };

    let claims = match decode::<Claims>(
        token,
        &DecodingKey::from_secret(app_state.jwt_secret.as_ref()),
        &Validation::default(),
    ) {
        Ok(token_data) => token_data.claims,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid token").into_response(),
    };
    
    ws.on_upgrade(move |socket| {
        handle_socket(socket, app_state.db_pool, app_state.chat_state, group_id, claims.sub)
    })
}

async fn handle_socket(socket: WebSocket, db_pool: Pool<Sqlite>, chat_state: ChatState, group_id: Uuid, user_id: Uuid) {
    let tx = chat_state.entry(group_id).or_insert_with(|| broadcast::channel(100).0).clone();
    let mut rx = tx.subscribe();

    let username = sqlx::query_scalar!("SELECT username FROM users WHERE id = ?", user_id)
        .fetch_one(&db_pool).await.unwrap_or_else(|_| "Sconosciuto".to_string());

    let (mut sender, mut receiver) = socket.split();

    let recv_username = username.clone();
    let recv_db_pool = db_pool.clone();

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let msg: WsClientMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(_) => continue,
            };
            
            if let Err(e) = sqlx::query!(
                "INSERT INTO group_messages (group_id, user_id, content) VALUES (?, ?, ?)",
                group_id, user_id, msg.content
            )
            .execute(&recv_db_pool)
            .await {
                tracing::error!("Failed to save message to DB: {}", e);
                continue;
            }

            let server_msg = WsServerMessage {
                sender_id: user_id,
                sender_username: recv_username.clone(),
                content: msg.content.clone(),
            };
            
            if tx.send(serde_json::to_string(&server_msg).unwrap()).is_err() { break; }
        }
    });

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() { break; }
        }
    });

    tokio::select! {
        _ = (&mut recv_task) => send_task.abort(),
        _ = (&mut send_task) => recv_task.abort(),
    };
    
    chat_state.remove_if(&group_id, |_, channel| channel.receiver_count() == 0);
}