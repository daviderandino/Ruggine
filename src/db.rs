use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::env;

/// Crea e restituisce un pool di connessioni al database PostgreSQL.
/// La funzione legge la stringa di connessione dalla variabile d'ambiente DATABASE_URL.
pub async fn create_db_pool() -> Result<Pool<Postgres>, sqlx::Error> {
    // Leggi la variabile d'ambiente DATABASE_URL
    let db_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in your .env file");

    // Crea un pool di connessioni
    PgPoolOptions::new()
        .max_connections(10) // Imposta un numero massimo di connessioni
        .connect(&db_url)
        .await
}