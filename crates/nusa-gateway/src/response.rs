use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;

/// Build a response with a guaranteed-valid status code.
pub fn status_response(status: StatusCode, body: impl Into<Body>) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(body.into())
        .unwrap()
}
