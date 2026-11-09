//! `GET /.well-known/jwks.json`, mounted at the root by [`build_app`](crate::build_app).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};

use crate::core::state::AppState;

pub async fn jwks(State(state): State<AppState>) -> Response {
    let mut response = Json(state.keys.jwks()).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    response
}
