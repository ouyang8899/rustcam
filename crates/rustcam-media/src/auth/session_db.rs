use async_trait::async_trait;
use rbs::Value;
use uuid::Uuid;

use crate::error::{AppError, Result};

use super::auth_db::PostgresAuthRepository;
use super::models::{NewSession, UserSession, UserSessionWithUser};
use super::session::SessionRepository;

const SESSION_COLUMNS: &str = "id, user_id, client_type, client_name, client_ip, user_agent, \
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, \
    to_char(last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_seen_at";

/// 与 users 联表时的限定列名，避免 id / user_id 列歧义。
const SESSION_COLUMNS_JOINED: &str = "s.id, s.user_id, s.client_type, s.client_name, s.client_ip, s.user_agent, \
    to_char(s.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, \
    to_char(s.last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_seen_at";

fn parse_session(row: &Value) -> Result<UserSession> {
    let parse_time = |key: &str| {
        row[key]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now)
    };
    let parse_uuid = |key: &str| {
        Uuid::parse_str(row[key].as_str().unwrap_or(""))
            .map_err(|e| AppError::Internal(format!("invalid {}: {}", key, e)))
    };
    Ok(UserSession {
        id: parse_uuid("id")?,
        user_id: parse_uuid("user_id")?,
        client_type: row["client_type"].as_str().unwrap_or("web").to_string(),
        client_name: row["client_name"].as_str().map(String::from),
        client_ip: row["client_ip"].as_str().map(String::from),
        user_agent: row["user_agent"].as_str().map(String::from),
        created_at: parse_time("created_at"),
        last_seen_at: parse_time("last_seen_at"),
    })
}

fn optional_text(value: &Option<String>) -> Value {
    match value {
        Some(v) => rbs::value![v.clone()],
        None => Value::Null,
    }
}

impl PostgresAuthRepository {
    /// 活跃会话数量；给出 user_id 时只统计该用户。
    async fn count_active(&self, user_id: Option<Uuid>) -> Result<i64> {
        let (sql, args) = match user_id {
            Some(id) => (
                "SELECT COUNT(*) AS c FROM user_sessions WHERE user_id = $1::uuid AND revoked_at IS NULL",
                vec![rbs::value![id.to_string()]],
            ),
            None => (
                "SELECT COUNT(*) AS c FROM user_sessions WHERE revoked_at IS NULL",
                vec![],
            ),
        };
        let rows: Vec<Value> = self.rb.exec_decode(sql, args).await?;
        Ok(rows
            .first()
            .and_then(|row| row["c"].as_i64())
            .unwrap_or(0))
    }
}

#[async_trait]
impl SessionRepository for PostgresAuthRepository {
    async fn create(&self, session: &NewSession) -> Result<()> {
        self.rb
            .exec(
                "INSERT INTO user_sessions (id, user_id, client_type, client_name, client_ip, user_agent, created_at, last_seen_at) \
                 VALUES ($1::uuid, $2::uuid, $3::varchar, $4::varchar, $5::varchar, $6::text, NOW(), NOW())",
                vec![
                    rbs::value![session.id.to_string()],
                    rbs::value![session.user_id.to_string()],
                    rbs::value![session.client_type.clone()],
                    optional_text(&session.client_name),
                    optional_text(&session.client_ip),
                    optional_text(&session.user_agent),
                ],
            )
            .await?;
        Ok(())
    }

    async fn find_active(&self, id: Uuid) -> Result<Option<UserSession>> {
        let rows: Vec<Value> = self
            .rb
            .exec_decode(
                &format!(
                    "SELECT {} FROM user_sessions WHERE id = $1::uuid AND revoked_at IS NULL",
                    SESSION_COLUMNS
                ),
                vec![rbs::value![id.to_string()]],
            )
            .await?;
        rows.first().map(parse_session).transpose()
    }

    async fn list_active_for_user(&self, user_id: Uuid) -> Result<Vec<UserSession>> {
        let rows: Vec<Value> = self
            .rb
            .exec_decode(
                &format!(
                    "SELECT {} FROM user_sessions WHERE user_id = $1::uuid AND revoked_at IS NULL ORDER BY created_at, id",
                    SESSION_COLUMNS
                ),
                vec![rbs::value![user_id.to_string()]],
            )
            .await?;
        rows.iter().map(parse_session).collect()
    }

    async fn list_active_for_user_paged(
        &self,
        user_id: Uuid,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<UserSession>, i64)> {
        let rows: Vec<Value> = self
            .rb
            .exec_decode(
                &format!(
                    "SELECT {} FROM user_sessions WHERE user_id = $1::uuid AND revoked_at IS NULL \
                     ORDER BY last_seen_at DESC, id LIMIT $2 OFFSET $3",
                    SESSION_COLUMNS
                ),
                vec![
                    rbs::value![user_id.to_string()],
                    rbs::value![limit],
                    rbs::value![offset],
                ],
            )
            .await?;
        let total = self.count_active(Some(user_id)).await?;
        Ok((rows.iter().map(parse_session).collect::<Result<Vec<_>>>()?, total))
    }

    async fn list_all_active_paged(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<UserSessionWithUser>, i64)> {
        let rows: Vec<Value> = self
            .rb
            .exec_decode(
                &format!(
                    "SELECT {}, u.username FROM user_sessions s \
                     JOIN users u ON u.id = s.user_id \
                     WHERE s.revoked_at IS NULL \
                     ORDER BY s.last_seen_at DESC, s.id LIMIT $1 OFFSET $2",
                    SESSION_COLUMNS_JOINED
                ),
                vec![rbs::value![limit], rbs::value![offset]],
            )
            .await?;
        let total = self.count_active(None).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(UserSessionWithUser {
                session: parse_session(row)?,
                username: row["username"].as_str().unwrap_or("").to_string(),
            });
        }
        Ok((items, total))
    }

    async fn touch(&self, id: Uuid) -> Result<()> {
        self.rb
            .exec(
                "UPDATE user_sessions SET last_seen_at = NOW() WHERE id = $1::uuid AND revoked_at IS NULL",
                vec![rbs::value![id.to_string()]],
            )
            .await?;
        Ok(())
    }

    async fn revoke(&self, id: Uuid) -> Result<bool> {
        let result = self
            .rb
            .exec(
                "UPDATE user_sessions SET revoked_at = NOW() WHERE id = $1::uuid AND revoked_at IS NULL",
                vec![rbs::value![id.to_string()]],
            )
            .await?;
        Ok(result.rows_affected > 0)
    }
}
