use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

// Definisci il tuo tipo di errore custom con tutte le varianti necessarie
#[derive(Debug)]
pub enum AppError {
    DatabaseError(sqlx::Error),
    JwtError(jsonwebtoken::errors::Error),
    PasswordHashError(bcrypt::BcryptError), // Errore specifico per bcrypt

    // Errori di Logica/Input
    InvalidInput(String),
    WrongCredentials,
    UsernameExists,
    UserNotFound,
    GroupNotFound,
    UserOrGroupNotFound, // Per violazioni di Foreign Key generiche
    InvitationNotFound,
    InvitationAlreadyExists,
    UserAlreadyInGroup,
    MissingPermissions,
    CannotInviteSelf,
}

// Implementa `IntoResponse` per convertire l'errore in una risposta HTTP
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::DatabaseError(e) => {
                tracing::error!("Database error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "An internal server error occurred".to_string())
            }
            AppError::JwtError(_) => (StatusCode::UNAUTHORIZED, "Invalid authentication token".to_string()),
            AppError::PasswordHashError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to process request".to_string()),
            AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::WrongCredentials => (StatusCode::UNAUTHORIZED, "Invalid username or password".to_string()),
            AppError::UsernameExists => (StatusCode::CONFLICT, "Username already exists".to_string()),
            AppError::UserNotFound => (StatusCode::NOT_FOUND, "User not found".to_string()),
            AppError::GroupNotFound => (StatusCode::NOT_FOUND, "Group not found".to_string()),
            AppError::UserOrGroupNotFound => (StatusCode::NOT_FOUND, "The specified user or group does not exist".to_string()),
            AppError::InvitationNotFound => (StatusCode::NOT_FOUND, "Invitation not found or has already been handled".to_string()),
            AppError::InvitationAlreadyExists => (StatusCode::CONFLICT, "An invitation for this user to this group already exists".to_string()),
            AppError::UserAlreadyInGroup => (StatusCode::CONFLICT, "User is already a member of this group".to_string()),
            AppError::MissingPermissions => (StatusCode::FORBIDDEN, "You do not have permission to perform this action".to_string()),
            AppError::CannotInviteSelf => (StatusCode::BAD_REQUEST, "You cannot invite yourself to a group".to_string()),
        };

        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}

// Implementazioni di `From` per usare `?`

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::DatabaseError(e)
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        AppError::JwtError(e)
    }
}

// AGGIUNTA: Implementazione `From` per l'errore di bcrypt
impl From<bcrypt::BcryptError> for AppError {
    fn from(e: bcrypt::BcryptError) -> Self {
        AppError::PasswordHashError(e)
    }
}