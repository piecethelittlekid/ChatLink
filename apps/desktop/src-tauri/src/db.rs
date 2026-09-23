use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub seq: i64,
    pub id: String,
    pub sender: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug)]
pub struct InsertOutcome {
    pub inserted: bool,
    pub message: StoredMessage,
}

#[derive(Debug)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub token_hash: String,
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

fn row_to_message(row: (i64, String, String, String, String, String)) -> StoredMessage {
    StoredMessage {
        seq: row.0,
        id: row.1,
        sender: if row.2 == "windows-main" {
            "windows".into()
        } else {
            "iphone".into()
        },
        content: row.3,
        status: if row.4 == "pending_delivery" {
            "stored".into()
        } else {
            row.4
        },
        created_at: row.5,
    }
}

pub async fn insert_message(
    pool: &SqlitePool,
    id: &str,
    sender: &str,
    content: &str,
    status: &str,
) -> Result<InsertOutcome> {
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
    let row = sqlx::query_as::<_, (i64, String, String, String, String, String)>(
        "SELECT seq, id, sender_device_id, content, status, created_at FROM messages WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .context("load stored message")?;
    anyhow::ensure!(
        row.2 == sender && row.3 == content,
        "message id already exists with different content"
    );
    Ok(InsertOutcome {
        inserted: result.rows_affected() == 1,
        message: row_to_message(row),
    })
}

pub async fn mark_delivered(pool: &SqlitePool, id: &str) -> Result<bool> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let result = sqlx::query(
        "UPDATE messages SET status = 'delivered', delivered_at = ? WHERE id = ? AND sender_device_id = 'windows-main' AND status = 'pending_delivery'",
    )
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .context("mark Windows message delivered")?;
    Ok(result.rows_affected() == 1)
}

pub async fn messages_after(
    pool: &SqlitePool,
    after_seq: i64,
    limit: i64,
) -> Result<Vec<StoredMessage>> {
    let rows = sqlx::query_as::<_, (i64, String, String, String, String, String)>(
        "SELECT seq, id, sender_device_id, content, status, created_at FROM messages WHERE seq > ? ORDER BY seq ASC LIMIT ?",
    )
    .bind(after_seq)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("load messages after cursor")?;
    Ok(rows.into_iter().map(row_to_message).collect())
}

pub async fn recent_messages(
    pool: &SqlitePool,
    before_seq: Option<i64>,
    limit: i64,
) -> Result<Vec<StoredMessage>> {
    let rows = sqlx::query_as::<_, (i64, String, String, String, String, String)>(
        "SELECT seq, id, sender_device_id, content, status, created_at FROM messages WHERE (? IS NULL OR seq < ?) ORDER BY seq DESC LIMIT ?",
    )
    .bind(before_seq)
    .bind(before_seq)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("load recent messages")?;
    Ok(rows.into_iter().rev().map(row_to_message).collect())
}

pub async fn paired_device(pool: &SqlitePool) -> Result<Option<PairedDevice>> {
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, name, token_hash FROM devices LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .context("load paired iPhone")?;
    Ok(row.map(|(id, name, token_hash)| PairedDevice {
        id,
        name,
        token_hash,
    }))
}

pub async fn pair_device(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    token_hash: &str,
) -> Result<bool> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let result = sqlx::query(
        "INSERT INTO devices(id, name, token_hash, paired_at) SELECT ?, ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM devices)",
    )
    .bind(id)
    .bind(name)
    .bind(token_hash)
    .bind(now)
    .execute(pool)
    .await
    .context("pair iPhone")?;
    Ok(result.rows_affected() == 1)
}

pub async fn unpair_device(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM devices")
        .execute(pool)
        .await
        .context("unpair iPhone")?;
    Ok(())
}

pub async fn touch_device(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
        .context("update device last seen")?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn duplicate_insert_keeps_one_message_and_cursor_order() -> Result<()> {
        let pool = open_memory().await?;
        let first = insert_message(
            &pool,
            "01KABCDEFGABCDEFGABCDEFGAB",
            "iphone-main",
            "hello",
            "stored",
        )
        .await?;
        assert!(first.inserted);
        let duplicate =
            insert_message(&pool, &first.message.id, "iphone-main", "hello", "stored").await?;
        assert!(!duplicate.inserted);
        assert_eq!(first.message.seq, duplicate.message.seq);
        assert!(
            insert_message(&pool, &first.message.id, "iphone-main", "changed", "stored")
                .await
                .is_err()
        );
        let second = insert_message(
            &pool,
            "01KABCDEFGABCDEFGABCDEFGAC",
            "windows-main",
            "hi",
            "pending_delivery",
        )
        .await?;
        let after = messages_after(&pool, first.message.seq, 50).await?;
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].seq, second.message.seq);
        assert!(!mark_delivered(&pool, &first.message.id).await?);
        assert!(mark_delivered(&pool, &second.message.id).await?);
        assert_eq!(
            recent_messages(&pool, None, 50).await?[1].status,
            "delivered"
        );
        Ok(())
    }

    #[tokio::test]
    async fn only_one_device_can_pair_and_unpair_preserves_messages() -> Result<()> {
        let pool = open_memory().await?;
        insert_message(
            &pool,
            "01KABCDEFGABCDEFGABCDEFGAB",
            "iphone-main",
            "saved",
            "stored",
        )
        .await?;
        assert!(pair_device(&pool, "iphone-a", "iPhone", "hash-a").await?);
        assert!(!pair_device(&pool, "iphone-b", "iPhone", "hash-b").await?);
        assert_eq!(paired_device(&pool).await?.unwrap().id, "iphone-a");
        unpair_device(&pool).await?;
        assert!(paired_device(&pool).await?.is_none());
        assert_eq!(recent_messages(&pool, None, 50).await?.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn migration_preserves_milestone_three_history() -> Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_initial.sql"))
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO messages(id, sender_device_id, content, status, created_at) VALUES('old-id', 'iphone-main', 'old text', 'stored', '2026-01-01T00:00:00Z')")
            .execute(&pool).await?;
        sqlx::raw_sql(include_str!("../migrations/0002_message_sequence.sql"))
            .execute(&pool)
            .await?;
        let messages = recent_messages(&pool, None, 50).await?;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "old-id");
        assert_eq!(messages[0].content, "old text");
        assert!(messages[0].seq > 0);
        Ok(())
    }
}
