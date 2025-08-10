use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::env;

/// Crea e restituisce un pool di connessioni al database SQLite.
/// La funzione legge la stringa di connessione dalla variabile d'ambiente DATABASE_URL.
pub async fn create_db_pool() -> Result<Pool<Sqlite>, sqlx::Error> {
    // Leggi la variabile d'ambiente DATABASE_URL
    let db_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in your .env file");

    // Crea un pool di connessioni
    let pool = SqlitePoolOptions::new()
        .max_connections(10) // Imposta un numero massimo di connessioni
        .connect(&db_url)
        .await?;

    // SQLite: attiva le FK
    sqlx::query("PRAGMA foreign_keys = ON;").execute(&pool).await?;

    Ok(pool)
}