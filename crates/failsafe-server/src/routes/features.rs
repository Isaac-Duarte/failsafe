use axum::{Json, Router, routing::get};
use failsafe_core::api::FeaturesListResponse;
use failsafe_feature_registry::catalog;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_features))
}

async fn list_features() -> Json<FeaturesListResponse> {
    Json(FeaturesListResponse {
        features: catalog(),
    })
}
