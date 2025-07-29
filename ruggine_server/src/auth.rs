use crate::{models::Claims, AppState};
use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Serialize;

/// Struct per una risposta di errore JSON standardizzata.
#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

/// Implementazione dell'estrattore di Axum.
/// Questo permette di usare `Claims` come parametro negli handler.
/// Axum eseguirà questo codice automaticamente per le rotte protette.
#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    // Questo permette di ottenere `AppState` dallo stato dell'applicazione.
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Estrae l'header "Authorization: Bearer <token>"
        let TypedHeader(Authorization(bearer)) =
            TypedHeader::<Authorization<Bearer>>::from_request_parts(parts, state)
                .await
                .map_err(|_| AuthError::InvalidToken)?;

        // 2. Ottiene la chiave segreta per il JWT dallo stato dell'applicazione
        let secret = &AppState::from_ref(state).jwt_secret;

        // 3. Decodifica e valida il token
        let token_data = decode::<Claims>(
            bearer.token(),
            &DecodingKey::from_secret(secret.as_ref()),
            &Validation::default(),
        )
        .map_err(|_| AuthError::InvalidToken)?;

        // 4. Se la validazione ha successo, restituisce le "claims" (i dati dell'utente)
        Ok(token_data.claims)
    }
}

/// Tipo di errore per l'estrattore.
pub enum AuthError {
    InvalidToken,
}

/// Come convertire il nostro `AuthError` in una risposta HTTP.
/// Questo permette di inviare un errore 401 standard in caso di token mancante o non valido.
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Token di autenticazione non valido o mancante."),
        };

        let body = Json(ErrorResponse {
            message: error_message.to_string(),
        });

        (status, body).into_response()
    }
}