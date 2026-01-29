//! Upstream registry operations

use chrono::Utc;
use sqlx::Row;

use crate::error::DbError;
use crate::models::{
    NewUpstream, NewUpstreamRoute, UpdateUpstream, Upstream, UpstreamRoute,
};
use crate::repository::Database;

impl Database {
    // ==================== Upstream Operations ====================

    /// Insert a new upstream
    pub async fn insert_upstream(&self, upstream: NewUpstream) -> Result<Upstream, DbError> {
        let now = Utc::now();

        // If this is being set as default, unset any existing default
        if upstream.is_default {
            sqlx::query("UPDATE upstreams SET is_default = 0 WHERE is_default = 1")
                .execute(&self.pool)
                .await?;
        }

        let result = sqlx::query(
            r#"
            INSERT INTO upstreams (name, display_name, url, registry, username, password,
                                   skip_tls_verify, priority, enabled, cache_isolation,
                                   is_default, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(&upstream.name)
        .bind(&upstream.display_name)
        .bind(&upstream.url)
        .bind(&upstream.registry)
        .bind(&upstream.username)
        .bind(&upstream.password)
        .bind(upstream.skip_tls_verify)
        .bind(upstream.priority)
        .bind(upstream.enabled)
        .bind(upstream.cache_isolation.as_str())
        .bind(upstream.is_default)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = result.get("id");

        Ok(Upstream {
            id,
            name: upstream.name,
            display_name: upstream.display_name,
            url: upstream.url,
            registry: upstream.registry,
            username: upstream.username,
            password: upstream.password,
            skip_tls_verify: upstream.skip_tls_verify,
            priority: upstream.priority,
            enabled: upstream.enabled,
            cache_isolation: upstream.cache_isolation,
            is_default: upstream.is_default,
            created_at: now,
            updated_at: now,
        })
    }

    /// Get an upstream by ID
    pub async fn get_upstream(&self, id: i64) -> Result<Option<Upstream>, DbError> {
        let result = sqlx::query(
            r#"
            SELECT id, name, display_name, url, registry, username, password,
                   skip_tls_verify, priority, enabled, cache_isolation, is_default,
                   created_at, updated_at
            FROM upstreams
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        result
            .map(|row| Upstream::try_from(&row).map_err(DbError::from))
            .transpose()
    }

    /// Get an upstream by name
    pub async fn get_upstream_by_name(&self, name: &str) -> Result<Option<Upstream>, DbError> {
        let result = sqlx::query(
            r#"
            SELECT id, name, display_name, url, registry, username, password,
                   skip_tls_verify, priority, enabled, cache_isolation, is_default,
                   created_at, updated_at
            FROM upstreams
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        result
            .map(|row| Upstream::try_from(&row).map_err(DbError::from))
            .transpose()
    }

    /// Get the default upstream
    pub async fn get_default_upstream(&self) -> Result<Option<Upstream>, DbError> {
        let result = sqlx::query(
            r#"
            SELECT id, name, display_name, url, registry, username, password,
                   skip_tls_verify, priority, enabled, cache_isolation, is_default,
                   created_at, updated_at
            FROM upstreams
            WHERE is_default = 1 AND enabled = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        result
            .map(|row| Upstream::try_from(&row).map_err(DbError::from))
            .transpose()
    }

    /// List all upstreams
    pub async fn list_upstreams(&self) -> Result<Vec<Upstream>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, url, registry, username, password,
                   skip_tls_verify, priority, enabled, cache_isolation, is_default,
                   created_at, updated_at
            FROM upstreams
            ORDER BY priority ASC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| Upstream::try_from(row).map_err(DbError::from))
            .collect()
    }

    /// List enabled upstreams
    pub async fn list_enabled_upstreams(&self) -> Result<Vec<Upstream>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, url, registry, username, password,
                   skip_tls_verify, priority, enabled, cache_isolation, is_default,
                   created_at, updated_at
            FROM upstreams
            WHERE enabled = 1
            ORDER BY priority ASC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| Upstream::try_from(row).map_err(DbError::from))
            .collect()
    }

    /// Update an upstream
    pub async fn update_upstream(
        &self,
        id: i64,
        update: UpdateUpstream,
    ) -> Result<Option<Upstream>, DbError> {
        let now = Utc::now();

        // If setting as default, unset any existing default
        if update.is_default == Some(true) {
            sqlx::query("UPDATE upstreams SET is_default = 0 WHERE is_default = 1 AND id != ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }

        // Build dynamic update query
        let mut updates = vec!["updated_at = ?".to_string()];
        let mut has_updates = false;

        if update.display_name.is_some() {
            updates.push("display_name = ?".to_string());
            has_updates = true;
        }
        if update.url.is_some() {
            updates.push("url = ?".to_string());
            has_updates = true;
        }
        if update.registry.is_some() {
            updates.push("registry = ?".to_string());
            has_updates = true;
        }
        if update.username.is_some() {
            updates.push("username = ?".to_string());
            has_updates = true;
        }
        if update.password.is_some() {
            updates.push("password = ?".to_string());
            has_updates = true;
        }
        if update.skip_tls_verify.is_some() {
            updates.push("skip_tls_verify = ?".to_string());
            has_updates = true;
        }
        if update.priority.is_some() {
            updates.push("priority = ?".to_string());
            has_updates = true;
        }
        if update.enabled.is_some() {
            updates.push("enabled = ?".to_string());
            has_updates = true;
        }
        if update.cache_isolation.is_some() {
            updates.push("cache_isolation = ?".to_string());
            has_updates = true;
        }
        if update.is_default.is_some() {
            updates.push("is_default = ?".to_string());
            has_updates = true;
        }

        if !has_updates {
            return self.get_upstream(id).await;
        }

        let sql = format!("UPDATE upstreams SET {} WHERE id = ?", updates.join(", "));
        let mut query = sqlx::query(&sql);

        // Bind updated_at first
        query = query.bind(now.to_rfc3339());

        // Bind optional fields in the same order as updates
        if let Some(ref v) = update.display_name {
            query = query.bind(v);
        }
        if let Some(ref v) = update.url {
            query = query.bind(v);
        }
        if let Some(ref v) = update.registry {
            query = query.bind(v);
        }
        if let Some(ref v) = update.username {
            query = query.bind(v.clone());
        }
        if let Some(ref v) = update.password {
            query = query.bind(v.clone());
        }
        if let Some(v) = update.skip_tls_verify {
            query = query.bind(v);
        }
        if let Some(v) = update.priority {
            query = query.bind(v);
        }
        if let Some(v) = update.enabled {
            query = query.bind(v);
        }
        if let Some(ref v) = update.cache_isolation {
            query = query.bind(v.as_str());
        }
        if let Some(v) = update.is_default {
            query = query.bind(v);
        }

        // Bind the id
        query = query.bind(id);

        query.execute(&self.pool).await?;

        self.get_upstream(id).await
    }

    /// Delete an upstream
    pub async fn delete_upstream(&self, id: i64) -> Result<bool, DbError> {
        // First delete associated routes
        sqlx::query("DELETE FROM upstream_routes WHERE upstream_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        let result = sqlx::query("DELETE FROM upstreams WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get upstream count
    pub async fn get_upstream_count(&self) -> Result<i64, DbError> {
        let result = sqlx::query("SELECT COUNT(*) as count FROM upstreams")
            .fetch_one(&self.pool)
            .await?;
        Ok(result.get("count"))
    }

    // ==================== Upstream Route Operations ====================

    /// Insert a new upstream route
    pub async fn insert_upstream_route(
        &self,
        route: NewUpstreamRoute,
    ) -> Result<UpstreamRoute, DbError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            INSERT INTO upstream_routes (upstream_id, pattern, priority, created_at)
            VALUES (?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(route.upstream_id)
        .bind(&route.pattern)
        .bind(route.priority)
        .bind(now.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = result.get("id");

        Ok(UpstreamRoute {
            id,
            upstream_id: route.upstream_id,
            pattern: route.pattern,
            priority: route.priority,
            created_at: now,
        })
    }

    /// Get routes for an upstream
    pub async fn get_upstream_routes(&self, upstream_id: i64) -> Result<Vec<UpstreamRoute>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, upstream_id, pattern, priority, created_at
            FROM upstream_routes
            WHERE upstream_id = ?
            ORDER BY priority ASC
            "#,
        )
        .bind(upstream_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| UpstreamRoute::try_from(row).map_err(DbError::from))
            .collect()
    }

    /// List all upstream routes
    pub async fn list_upstream_routes(&self) -> Result<Vec<UpstreamRoute>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, upstream_id, pattern, priority, created_at
            FROM upstream_routes
            ORDER BY priority ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| UpstreamRoute::try_from(row).map_err(DbError::from))
            .collect()
    }

    /// Delete an upstream route
    pub async fn delete_upstream_route(&self, id: i64) -> Result<bool, DbError> {
        let result = sqlx::query("DELETE FROM upstream_routes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete all routes for an upstream
    pub async fn delete_upstream_routes(&self, upstream_id: i64) -> Result<i64, DbError> {
        let result = sqlx::query("DELETE FROM upstream_routes WHERE upstream_id = ?")
            .bind(upstream_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as i64)
    }
}
