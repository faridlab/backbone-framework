//! Request body extractors.
//!
//! [`JsonOrForm`] accepts a request body as EITHER `application/json` OR
//! `application/x-www-form-urlencoded` (defaulting to JSON when the
//! `Content-Type` is absent/unknown), so endpoints built with
//! [`crate::http::BackboneCrudHandler`] — and any handler using it — work with
//! both content types. Drop-in replacement for `Json(x): Json<T>` /
//! `Form(x): Form<T>` in handler signatures and route closures.

use axum::{
    async_trait,
    extract::{FromRequest, Request},
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
    Form, Json, RequestExt,
};

/// Body extractor that accepts JSON or url-encoded form data.
pub struct JsonOrForm<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for JsonOrForm<T>
where
    S: Send + Sync,
    Json<T>: FromRequest<()>,
    Form<T>: FromRequest<()>,
    T: 'static,
{
    type Rejection = Response;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let is_form = req
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.starts_with("application/x-www-form-urlencoded"))
            .unwrap_or(false);

        if is_form {
            let Form(value) = req.extract().await.map_err(IntoResponse::into_response)?;
            Ok(Self(value))
        } else {
            // JSON (and any other / missing content-type, leniently)
            let Json(value) = req.extract().await.map_err(IntoResponse::into_response)?;
            Ok(Self(value))
        }
    }
}
