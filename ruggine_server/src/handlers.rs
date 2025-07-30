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
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

pub async fn register_user(
    State(app_state): State<AppState>,
    Json(payload): Json<RegisterUserPayload>,
) -> Result<Json<User>, AppError> {
    if payload.password.len() < 8 {
        return Err(AppError::InvalidInput(
            "Password must be at least 8 characters long.".to_string(),
        ));
    }

    // CORREZIONE: Ora puoi usare `?` direttamente grazie all'impl `From`
    let password_hash = hash(payload.password, DEFAULT_COST)?;

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
        "SELECT id, username, password_hash, created_at FROM users WHERE username = $1",
        payload.username
    )
    .fetch_optional(&app_state.db_pool)
    .await?
    .ok_or(AppError::WrongCredentials)?;

    if !verify(&payload.password, &user.password_hash).unwrap_or(false) {
        return Err(AppError::WrongCredentials);
    }

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

    Ok(Json(LoginResponse { token, user }))
}

pub async fn get_user_by_username(
    State(app_state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<User>, AppError> {
    sqlx::query_as!(
        User,
        "SELECT id, username, password_hash, created_at FROM users WHERE username = $1",
        username
    )
    .fetch_optional(&app_state.db_pool)
    .await?
    .map(Json)
    .ok_or(AppError::UserNotFound)
}

pub async fn create_group(
    claims: Claims,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateGroupPayload>,
) -> Result<Json<Group>, AppError> {
    let creator_id = claims.sub;

    let mut tx = app_state.db_pool.begin().await?;

    let new_group = sqlx::query_as!(Group, "INSERT INTO groups (name) VALUES ($1) RETURNING *", payload.name)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query!(
        "INSERT INTO group_members (user_id, group_id) VALUES ($1, $2)",
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

    let is_inviter_a_member: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = $1 AND group_id = $2)")
        .bind(inviter_id)
        .bind(group_id)
        .fetch_one(&mut *tx).await?;

    if !is_inviter_a_member.0 {
        return Err(AppError::MissingPermissions);
    }
    
    let is_already_member: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = $1 AND group_id = $2)")
        .bind(payload.user_to_invite_id).bind(group_id)
        .fetch_one(&mut *tx).await?;

    if is_already_member.0 {
        return Err(AppError::UserAlreadyInGroup);
    }

    let result = sqlx::query!(
        "INSERT INTO group_invitations (group_id, inviter_id, invited_user_id, status) VALUES ($1, $2, $3, 'pending')",
        group_id, inviter_id, payload.user_to_invite_id
    )
    .execute(&mut *tx).await;
    
    match result {
        Ok(_) => {
            tx.commit().await?;
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            // CORREZIONE: Usa le varianti di errore corrette
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
        SELECT gi.id, g.id as "group_id", g.name as "group_name", u.username as "inviter_username"
        FROM group_invitations gi
        JOIN groups g ON gi.group_id = g.id
        JOIN users u ON gi.inviter_id = u.id
        WHERE gi.invited_user_id = $1 AND gi.status = 'pending'
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
        "SELECT group_id FROM group_invitations WHERE id = $1 AND invited_user_id = $2 AND status = 'pending'",
        invitation_id, user_id
    )
    .fetch_optional(&mut *tx).await?
    .ok_or(AppError::InvitationNotFound)?; // CORREZIONE: Usa la variante corretta

    sqlx::query!("UPDATE group_invitations SET status = 'accepted' WHERE id = $1", invitation_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query!("INSERT INTO group_members (user_id, group_id) VALUES ($1, $2) ON CONFLICT DO NOTHING", user_id, invitation.group_id)
        .execute(&mut *tx)
        .await?;

    let group = sqlx::query_as!(Group, "SELECT * FROM groups WHERE id = $1", invitation.group_id)
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
        "UPDATE group_invitations SET status = 'declined' WHERE id = $1 AND invited_user_id = $2 AND status = 'pending'",
        invitation_id, claims.sub
    )
    .execute(&app_state.db_pool).await?;

    if result.rows_affected() == 0 {
        // CORREZIONE: Usa la variante corretta
        return Err(AppError::InvitationNotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_group_by_name(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Group>, AppError> {
    sqlx::query_as!(Group, "SELECT * FROM groups WHERE name = $1", name)
        .fetch_optional(&app_state.db_pool)
        .await?
        .map(Json)
        .ok_or(AppError::GroupNotFound)
}

// L'handler WebSocket rimane invariato
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

async fn handle_socket(socket: WebSocket, db_pool: PgPool, chat_state: ChatState, group_id: Uuid, user_id: Uuid) {
    let tx = chat_state.entry(group_id).or_insert_with(|| broadcast::channel(100).0).clone();
    let mut rx = tx.subscribe();

    let username = sqlx::query_scalar!("SELECT username FROM users WHERE id = $1", user_id)
        .fetch_one(&db_pool).await.unwrap_or_else(|_| "Sconosciuto".to_string());

    let (mut sender, mut receiver) = socket.split();

    let recv_username = username.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let msg: WsClientMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let server_msg = WsServerMessage { sender_id: user_id, sender_username: recv_username.clone(), content: msg.content };
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