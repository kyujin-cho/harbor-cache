//! Activity log routes

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use harbor_db::repository::ActivityLogQuery;

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{ActivityLogResponse, ActivityLogsListResponse, ActivityLogsQuery};

// ==================== Activity Log Routes ====================

/// GET /api/v1/logs (Admin only)
async fn list_activity_logs(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Query(query): Query<ActivityLogsQuery>,
) -> Result<Json<ActivityLogsListResponse>, ApiError> {
    let db_query = ActivityLogQuery {
        action: query.action,
        resource_type: query.resource_type,
        user_id: query.user_id,
        start_date: query.start_date,
        end_date: query.end_date,
        offset: query.offset,
        limit: query.limit.min(100), // Cap at 100
    };

    let (logs, total) = state.db.list_activity_logs(db_query).await?;

    let response_logs: Vec<ActivityLogResponse> = logs
        .into_iter()
        .map(|log| ActivityLogResponse {
            id: log.id,
            timestamp: log.timestamp.to_rfc3339(),
            action: log.action,
            resource_type: log.resource_type,
            resource_id: log.resource_id,
            user_id: log.user_id,
            username: log.username,
            details: log.details,
            ip_address: log.ip_address,
        })
        .collect();

    Ok(Json(ActivityLogsListResponse {
        logs: response_logs,
        total,
        offset: query.offset,
        limit: query.limit,
    }))
}

/// GET /api/v1/logs/actions (Admin only) - Get distinct action types
async fn get_action_types(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, ApiError> {
    let actions = state.db.get_activity_action_types().await?;
    Ok(Json(actions))
}

/// GET /api/v1/logs/resource-types (Admin only) - Get distinct resource types
async fn get_resource_types(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, ApiError> {
    let resource_types = state.db.get_activity_resource_types().await?;
    Ok(Json(resource_types))
}

/// Create activity log routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/logs", get(list_activity_logs))
        .route("/api/v1/logs/actions", get(get_action_types))
        .route("/api/v1/logs/resource-types", get(get_resource_types))
}
