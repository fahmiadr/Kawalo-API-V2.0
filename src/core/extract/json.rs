//! `ValidatedJson<T>`: request body with the Elysia parsing and error behaviour.

use axum::extract::{FromRequest, Multipart, Request};
use axum::http::{StatusCode, header};
use bytes::Bytes;
use serde_json::{Map, Value};

use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::schema::{Body, FILE_PART_KEY};

/// Validates a request body against [`Body::SCHEMA`] and deserialises it. Unknown fields are
/// ignored, as Elysia strips them.
///
/// The body is parsed by content type exactly like Elysia's default parser: JSON, URL-encoded
/// forms (repeated keys become arrays, every value is text) and multipart forms become an object
/// that is then validated; a `text/*` or `application/o*` body counts as an empty object; any
/// other or missing content type leaves no body at all.
///
/// Failures match the TypeScript server:
/// * malformed or empty JSON, or a broken multipart body: 400 `BAD_REQUEST`
/// * schema mismatch or no body: 422 `VALIDATION_ERROR` with TypeBox's wording
///
/// ```ignore
/// async fn create(user: CurrentUser, ValidatedJson(body): ValidatedJson<CreateDto>) -> ApiResult<Out> { .. }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedJson<T>(pub T);

/// How Elysia's generated parser classifies a request body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyKind {
    Json,
    UrlEncoded,
    Multipart,
    /// `text/*` and `application/octet-stream`: a value without properties.
    Opaque,
    Absent,
}

impl BodyKind {
    /// Elysia strips parameters after `;`, then looks at the 13th character (the one after
    /// `application/`) without trimming or lower-casing, so `APPLICATION/JSON` is not JSON and
    /// `application/xml` is a URL-encoded form.
    fn of(content_type: Option<&str>) -> Self {
        let media = content_type
            .and_then(|c| c.split(';').next())
            .unwrap_or_default();
        match media.as_bytes().get(12) {
            Some(b'j') => Self::Json,
            Some(b'x') => Self::UrlEncoded,
            Some(b'r') => Self::Multipart,
            Some(b'o') => Self::Opaque,
            _ if media.starts_with('t') => Self::Opaque,
            _ => Self::Absent,
        }
    }
}

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: Body,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());
        let kind = BodyKind::of(content_type);

        let value = if kind == BodyKind::Multipart {
            let mut multipart = Multipart::from_request(req, state)
                .await
                .map_err(|_| ApiError::malformed_json())?;
            multipart_object(&mut multipart).await?
        } else {
            let bytes = Bytes::from_request(req, state).await.map_err(|rejection| {
                let status = rejection.status();
                if status == StatusCode::PAYLOAD_TOO_LARGE {
                    ApiError::new(status, ErrorCode::BadRequest, "Ukuran data terlalu besar")
                } else {
                    ApiError::malformed_json()
                }
            })?;
            match kind {
                BodyKind::Json => parse_json(&bytes)?,
                BodyKind::UrlEncoded => form_object(&bytes),
                BodyKind::Opaque | BodyKind::Multipart => Value::Object(Map::new()),
                BodyKind::Absent => Value::Null,
            }
        };

        T::parse(value).map(Self).map_err(ApiError::invalid_input)
    }
}

fn parse_json(bytes: &[u8]) -> Result<Value, ApiError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(ApiError::malformed_json());
    }
    serde_json::from_slice(bytes).map_err(|err| {
        use serde_json::error::Category;
        match err.classify() {
            Category::Data => ApiError::invalid_input(err.to_string()),
            Category::Syntax | Category::Eof | Category::Io => ApiError::malformed_json(),
        }
    })
}

/// Groups repeated keys into arrays; a single value stays as it is.
fn group_fields(fields: impl IntoIterator<Item = (String, Value)>) -> Map<String, Value> {
    let mut object = Map::new();
    for (key, value) in fields {
        if key == "__proto__" {
            continue;
        }
        match object.get_mut(&key) {
            None => {
                object.insert(key, value);
            }
            Some(Value::Array(items)) => items.push(value),
            Some(first) => *first = Value::Array(vec![first.take(), value]),
        }
    }
    object
}

fn form_object(bytes: &[u8]) -> Value {
    let fields = form_urlencoded::parse(bytes)
        .map(|(key, value)| (key.into_owned(), Value::String(value.into_owned())));
    Value::Object(group_fields(fields))
}

/// Multipart fields as Elysia reads them: file parts are opaque objects, and a lone text field
/// that looks like a JSON object or array is parsed.
///
/// Elysia also nests `a.b` and `a[0]` keys; those are not supported, so such a field is kept
/// under its literal name and cannot reach a DTO field.
async fn multipart_object(multipart: &mut Multipart) -> Result<Value, ApiError> {
    let mut fields: Vec<(String, Value)> = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::malformed_json())?
    {
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };
        let value = if field.file_name().is_some() {
            Value::Object(Map::from_iter([(
                FILE_PART_KEY.to_owned(),
                Value::Bool(true),
            )]))
        } else {
            Value::String(field.text().await.map_err(|_| ApiError::malformed_json())?)
        };
        fields.push((name, value));
    }
    let mut object = group_fields(fields);
    for value in object.values_mut() {
        if let Value::String(text) = value
            && text.starts_with(['{', '['])
            && let Ok(parsed @ (Value::Object(_) | Value::Array(_))) = serde_json::from_str(text)
        {
            *value = parsed;
        }
    }
    Ok(Value::Object(object))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::extract::schema::{Field, Kind, NO_LIMIT, Schema};
    use axum::body::Body;
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use serde_json::{Value, json};

    #[derive(Debug, Deserialize)]
    struct Dto {
        email: String,
        note: Option<String>,
    }

    impl crate::core::extract::Body for Dto {
        const SCHEMA: Schema = Schema::new(&[
            Field::required("email", Kind::email(NO_LIMIT)),
            Field::optional("note", Kind::text(0, NO_LIMIT)),
        ]);
    }

    async fn extract(content_type: Option<&str>, body: &str) -> Result<Dto, (StatusCode, Value)> {
        let mut builder = Request::builder().method("POST").uri("/");
        if let Some(ct) = content_type {
            builder = builder.header(header::CONTENT_TYPE, ct);
        }
        let req = builder.body(Body::from(body.to_owned())).expect("request");
        match ValidatedJson::<Dto>::from_request(req, &()).await {
            Ok(ValidatedJson(dto)) => Ok(dto),
            Err(err) => {
                let response = axum::response::IntoResponse::into_response(err);
                let status = response.status();
                let bytes = response
                    .into_body()
                    .collect()
                    .await
                    .expect("body")
                    .to_bytes();
                Err((status, serde_json::from_slice(&bytes).expect("json")))
            }
        }
    }

    const JSON: Option<&str> = Some("application/json");

    #[tokio::test]
    async fn valid_body_with_unknown_fields_passes() {
        let dto = extract(JSON, r#"{"email":"a@b.co","zzz":1}"#)
            .await
            .expect("ok");
        assert_eq!((dto.email.as_str(), dto.note), ("a@b.co", None));
        assert!(
            extract(
                Some("application/json; charset=utf-8"),
                r#"{"email":"a@b.co"}"#
            )
            .await
            .is_ok()
        );
    }

    #[tokio::test]
    async fn malformed_and_empty_json_are_400() {
        for body in ["{bad", "", "  ", r#"{"email":"a@b.co"} x"#] {
            let (status, json) = extract(JSON, body).await.expect_err(body);
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
            assert_eq!(json["error_code"], "BAD_REQUEST");
            assert_eq!(json["message"], "Format data JSON tidak valid");
        }
    }

    #[tokio::test]
    async fn schema_and_constraint_failures_are_422_with_details() {
        let (status, json) = extract(JSON, "{}").await.expect_err("missing");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["error_code"], "VALIDATION_ERROR");
        assert_eq!(json["message"], "Format data masukan tidak valid");
        assert_eq!(
            json["details"],
            "/email: Expected property 'email' to be string but found: undefined"
        );

        let (_, json) = extract(JSON, r#"{"email":"nope"}"#)
            .await
            .expect_err("format");
        assert_eq!(json["details"], "/email: Property 'email' should be email");

        let (_, json) = extract(JSON, r#"{"email":5}"#).await.expect_err("type");
        assert_eq!(
            json["details"],
            "/email: Expected property 'email' to be string but found: 5"
        );
        let (_, json) = extract(JSON, r#"{"email":"a@b.co","note":null}"#)
            .await
            .expect_err("null");
        assert_eq!(
            json["details"],
            "/note: Expected property 'note' to be string but found: null"
        );

        let (status, json) = extract(JSON, "null").await.expect_err("null");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["error_code"], json!("VALIDATION_ERROR"));
    }

    #[tokio::test]
    async fn missing_or_unknown_content_types_leave_no_body() {
        for ct in [
            None,
            Some(""),
            Some("APPLICATION/JSON"),
            Some("application/vnd.api+json"),
            Some("image/png"),
        ] {
            let (status, json) = extract(ct, r#"{"email":"a@b.co"}"#)
                .await
                .expect_err("no body");
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{ct:?}");
            assert_eq!(json["details"], ": Expected object", "{ct:?}");
        }
    }

    #[tokio::test]
    async fn text_and_binary_bodies_count_as_an_empty_object() {
        for ct in [
            "text/plain",
            "text/html; charset=utf-8",
            "application/octet-stream",
        ] {
            let (status, json) = extract(Some(ct), r#"{"email":"a@b.co"}"#)
                .await
                .expect_err(ct);
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{ct}");
            assert_eq!(
                json["details"],
                "/email: Expected property 'email' to be string but found: undefined",
                "{ct}"
            );
        }
    }

    #[tokio::test]
    async fn form_bodies_are_parsed_like_elysia() {
        const FORM: Option<&str> = Some("application/x-www-form-urlencoded");
        let dto = extract(FORM, "email=a%40b.co&note=hi+there&zzz=1")
            .await
            .expect("form");
        assert_eq!(
            (dto.email.as_str(), dto.note.as_deref()),
            ("a@b.co", Some("hi there"))
        );
        // `application/xml` shares the 13th character with the form type.
        assert!(
            extract(Some("application/xml"), "email=a%40b.co")
                .await
                .is_ok()
        );
        // A repeated key is an array, which a text field rejects.
        let (status, json) = extract(FORM, "email=a@b.co&email=c@d.co")
            .await
            .expect_err("array");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["error_code"], "VALIDATION_ERROR");
        let (_, json) = extract(FORM, "").await.expect_err("empty");
        assert_eq!(
            json["details"],
            "/email: Expected property 'email' to be string but found: undefined"
        );
    }

    #[tokio::test]
    async fn multipart_bodies_are_parsed_like_elysia() {
        let ct = "multipart/form-data; boundary=XX";
        let part = |name: &str, value: &str| {
            format!("--XX\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
        };
        let body = format!("{}{}--XX--\r\n", part("email", "a@b.co"), part("note", "n"));
        let dto = extract(Some(ct), &body).await.expect("multipart");
        assert_eq!(
            (dto.email.as_str(), dto.note.as_deref()),
            ("a@b.co", Some("n"))
        );

        let (status, json) = extract(Some(ct), "garbage").await.expect_err("broken");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["error_code"], "BAD_REQUEST");
    }
}
