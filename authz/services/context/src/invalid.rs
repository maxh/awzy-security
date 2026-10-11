//! Input awzy-context won't store, and why (#224).

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// What's wrong with a request's input, in a sentence the caller can act on. Answered as 400.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Invalid(pub String);

impl From<&str> for Invalid {
    fn from(s: &str) -> Self {
        Invalid(s.to_string())
    }
}

impl From<String> for Invalid {
    fn from(s: String) -> Self {
        Invalid(s)
    }
}

impl IntoResponse for Invalid {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.0).into_response()
    }
}
