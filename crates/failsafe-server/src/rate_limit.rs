use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window,
        }
    }

    pub fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut entries = self.inner.lock().expect("rate limiter lock poisoned");
        let history = entries.entry(key.to_owned()).or_default();
        history.retain(|instant| now.duration_since(*instant) < self.window);
        if history.len() >= self.max_requests {
            return false;
        }
        history.push(now);
        true
    }
}

pub async fn login_rate_limit_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    rate_limit(&state.login_limiter, request, next).await
}

pub async fn pairing_rate_limit_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    rate_limit(&state.pairing_limiter, request, next).await
}

async fn rate_limit(limiter: &RateLimiter, request: Request<Body>, next: Next) -> Response {
    let key = request.uri().path().to_owned();

    if limiter.allow(&key) {
        next.run(request).await
    } else {
        (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(failsafe_core::api::ApiError {
                error: "rate limit exceeded".to_owned(),
            }),
        )
            .into_response()
    }
}
