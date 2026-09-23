use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: String,
    pub content: String,
    pub created_at: String,
}

pub async fn open(path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create app data directory")?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context("open chatlink.db")?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("apply SQLite migrations")?;
    Ok(pool)
}

pub async fn open_memory() -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .context("open temporary in-memory SQLite")?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn insert_message(
    pool: &SqlitePool,
    id: &str,
    sender: &str,
    content: &str,
    status: &str,
) -> Result<bool> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let result = sqlx::query(
        "INSERT INTO messages(id, sender_device_id, content, status, created_at) VALUES(?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(id)
    .bind(sender)
    .bind(content)
    .bind(status)
    .bind(now)
    .execute(pool)
    .await
    .context("store message")?;

    if result.rows_affected() == 0 {
        let existing = sqlx::query_as::<_, (String, String)>(
            "SELECT sender_device_id, content FROM messages WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("check duplicate message")?;
        anyhow::ensure!(
            existing.as_ref().is_some_and(|(old_sender, old_content)| old_sender.as_str() == sender && old_content.as_str() == content),
            "message id already exists with different content"
        );
    }
    Ok(result.rows_affected() == 1)
}

pub async fn mark_delivered(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    sqlx::query("UPDATE messages SET status = 'delivered', delivered_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
        .context("mark message delivered")?;
    Ok(())
}

pub async fn pending_delivery(pool: &SqlitePool) -> Result<Vec<StoredMessage>> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, content, created_at FROM messages WHERE status = 'pending_delivery' ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .context("load pending delivery messages")?;
    Ok(rows
        .into_iter()
        .map(|(id, content, created_at)| StoredMessage {
            id,
            content,
            created_at,
        })
        .collect())
}

pub async fn certificate_setup_complete(pool: &SqlitePool) -> Result<bool> {
    let value = sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings WHERE key = 'certificate_setup_complete'",
    )
    .fetch_optional(pool)
    .await
    .context("read certificate setup status")?;
    Ok(value.as_deref() == Some("true"))
}

pub async fn set_certificate_setup_complete(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_settings(key, value) VALUES('certificate_setup_complete', 'true') ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .execute(pool)
    .await
    .context("save certificate setup status")?;
    Ok(())
}
