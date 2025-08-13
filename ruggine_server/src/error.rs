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
            tracing::error!("Errore del database: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Si è verificato un errore interno del server".to_string())
        }
        AppError::JwtError(_) => (StatusCode::UNAUTHORIZED, "Token di autenticazione non valido".to_string()),
        AppError::PasswordHashError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Impossibile elaborare la richiesta".to_string()),
        AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
        AppError::WrongCredentials => (StatusCode::UNAUTHORIZED, "Nome utente o password non validi".to_string()),
        AppError::UsernameExists => (StatusCode::CONFLICT, "Il nome utente esiste già".to_string()),
        AppError::UserNotFound => (StatusCode::NOT_FOUND, "Utente non trovato".to_string()),
        AppError::GroupNotFound => (StatusCode::NOT_FOUND, "Gruppo non trovato".to_string()),
        AppError::UserOrGroupNotFound => (StatusCode::NOT_FOUND, "L'utente o il gruppo specificato non esiste".to_string()),
        AppError::InvitationNotFound => (StatusCode::NOT_FOUND, "Invito non trovato o già gestito".to_string()),
        AppError::InvitationAlreadyExists => (StatusCode::CONFLICT, "Esiste già un invito per questo utente in questo gruppo".to_string()),
        AppError::UserAlreadyInGroup => (StatusCode::CONFLICT, "L'utente è già un membro di questo gruppo".to_string()),
        AppError::MissingPermissions => (StatusCode::FORBIDDEN, "Non hai il permesso per eseguire questa azione".to_string()),
        AppError::CannotInviteSelf => (StatusCode::BAD_REQUEST, "Non puoi invitare te stesso in un gruppo".to_string()),

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