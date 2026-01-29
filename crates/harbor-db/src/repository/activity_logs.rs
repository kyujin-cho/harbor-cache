//! Activity log operations

use chrono::Utc;
use sqlx::Row;

use crate::error::DbError;
use crate::models::{ActivityLog, NewActivityLog};
use crate::repository::Database;

/// Query parameters for listing activity logs
#[derive(Debug, Clone, Default)]
pub struct ActivityLogQuery {
    /// Filter by action type
    pub action: Option<String>,
    /// Filter by resource type
    pub resource_type: Option<String>,
    /// Filter by user ID
    pub user_id: Option<i64>,
    /// Filter by start date (RFC3339 format)
    pub start_date: Option<String>,
    /// Filter by end date (RFC3339 format)
    pub end_date: Option<String>,
    /// Pagination offset
    pub offset: i64,
    /// Pagination limit
    pub limit: i64,
}

impl Database {
    /// Insert a new activity log entry
    pub async fn insert_activity_log(&self, log: NewActivityLog) -> Result<ActivityLog, DbError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            INSERT INTO activity_logs (timestamp, action, resource_type, resource_id, user_id, username, details, ip_address)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(now.to_rfc3339())
        .bind(&log.action)
        .bind(&log.resource_type)
        .bind(&log.resource_id)
        .bind(log.user_id)
        .bind(&log.username)
        .bind(&log.details)
        .bind(&log.ip_address)
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = result.get("id");

        Ok(ActivityLog {
            id,
            timestamp: now,
            action: log.action,
            resource_type: log.resource_type,
            resource_id: log.resource_id,
            user_id: log.user_id,
            username: log.username,
            details: log.details,
            ip_address: log.ip_address,
        })
    }

    /// List activity logs with filtering and pagination
    pub async fn list_activity_logs(
        &self,
        query: ActivityLogQuery,
    ) -> Result<(Vec<ActivityLog>, i64), DbError> {
        let mut conditions = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(action) = &query.action {
            conditions.push("action = ?");
            params.push(action.clone());
        }
        if let Some(resource_type) = &query.resource_type {
            conditions.push("resource_type = ?");
            params.push(resource_type.clone());
        }
        if let Some(user_id) = query.user_id {
            conditions.push("user_id = ?");
            params.push(user_id.to_string());
        }
        if let Some(start_date) = &query.start_date {
            conditions.push("timestamp >= ?");
            params.push(start_date.clone());
        }
        if let Some(end_date) = &query.end_date {
            conditions.push("timestamp <= ?");
            params.push(end_date.clone());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Get total count
        let count_sql = format!("SELECT COUNT(*) as count FROM activity_logs {}", where_clause);
        let mut count_query = sqlx::query(&count_sql);
        for param in &params {
            count_query = count_query.bind(param);
        }
        let count_row = count_query.fetch_one(&self.pool).await?;
        let total: i64 = count_row.get("count");

        // Get logs
        let sql = format!(
            r#"
            SELECT id, timestamp, action, resource_type, resource_id, user_id, username, details, ip_address
            FROM activity_logs
            {}
            ORDER BY timestamp DESC
            LIMIT ? OFFSET ?
            "#,
            where_clause
        );

        let mut logs_query = sqlx::query(&sql);
        for param in &params {
            logs_query = logs_query.bind(param);
        }
        logs_query = logs_query.bind(query.limit).bind(query.offset);

        let rows = logs_query.fetch_all(&self.pool).await?;
        let logs: Result<Vec<ActivityLog>, _> = rows
            .iter()
            .map(|row| ActivityLog::try_from(row).map_err(DbError::from))
            .collect();

        Ok((logs?, total))
    }

    /// Get distinct action types from activity logs
    pub async fn get_activity_action_types(&self) -> Result<Vec<String>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT action
            FROM activity_logs
            ORDER BY action
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|row| row.get("action")).collect())
    }

    /// Get distinct resource types from activity logs
    pub async fn get_activity_resource_types(&self) -> Result<Vec<String>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT resource_type
            FROM activity_logs
            ORDER BY resource_type
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|row| row.get("resource_type")).collect())
    }

    /// Clean up old activity logs (keep last N days)
    pub async fn cleanup_old_activity_logs(&self, days: i64) -> Result<u64, DbError> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let result = sqlx::query("DELETE FROM activity_logs WHERE timestamp < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
