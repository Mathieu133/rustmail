use crate::db::repr::BlockedUser;
use crate::prelude::errors::*;
use chrono::Utc;
use serenity::all::UserId;
use sqlx::{Row, SqlitePool};

pub async fn block_user(
    user_id: UserId,
    user_name: &str,
    blocked_by: UserId,
    expires_at: Option<i64>,
    reason: Option<&str>,
    silent: bool,
    pool: &SqlitePool,
) -> ModmailResult<()> {
    sqlx::query(
        r#"
        INSERT INTO blocked_users
            (user_id, user_name, blocked_by, blocked_at, expires_at, reason, silent)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(user_id) DO UPDATE SET
            user_name = excluded.user_name,
            blocked_by = excluded.blocked_by,
            blocked_at = excluded.blocked_at,
            expires_at = excluded.expires_at,
            reason = excluded.reason,
            silent = excluded.silent
        "#,
    )
    .bind(user_id.to_string())
    .bind(user_name)
    .bind(blocked_by.to_string())
    .bind(Utc::now().timestamp())
    .bind(expires_at)
    .bind(reason)
    .bind(silent)
    .execute(pool)
    .await
    .map_err(|e| {
        eprintln!("Failed to block user {user_id}: {e:?}");
        validation_failed("Failed to block user")
    })?;

    Ok(())
}

pub async fn unblock_user(user_id: UserId, pool: &SqlitePool) -> ModmailResult<bool> {
    let active = get_active_block(user_id, pool).await?.is_some();

    sqlx::query("DELETE FROM blocked_users WHERE user_id = ?")
        .bind(user_id.to_string())
        .execute(pool)
        .await
        .map_err(|e| {
            eprintln!("Failed to unblock user {user_id}: {e:?}");
            validation_failed("Failed to unblock user")
        })?;

    Ok(active)
}

pub async fn get_active_block(
    user_id: UserId,
    pool: &SqlitePool,
) -> ModmailResult<Option<BlockedUser>> {
    let row = sqlx::query(
        r#"
        SELECT expires_at, silent
        FROM blocked_users
        WHERE user_id = ? AND (expires_at IS NULL OR expires_at > ?)
        "#,
    )
    .bind(user_id.to_string())
    .bind(Utc::now().timestamp())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        eprintln!("Failed to fetch block for user {user_id}: {e:?}");
        validation_failed("Failed to fetch blocked user")
    })?;

    Ok(row.map(|row| BlockedUser {
        expires_at: row.get("expires_at"),
        silent: row.get("silent"),
    }))
}
