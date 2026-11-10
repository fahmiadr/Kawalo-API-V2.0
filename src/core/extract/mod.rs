//! Request extractors: validated JSON and query strings, and the authenticated user.

pub mod auth;
pub mod json;
pub mod numeric;
pub mod query;
pub mod schema;
pub mod validate;

pub use auth::{Auth, AuthContext, AuthedJson, CurrentUser, authenticate_token};
pub use json::ValidatedJson;
pub use numeric::Numeric;
pub use query::ValidatedQuery;
pub use schema::Body;
pub use validate::{Validate, ValidationErrors};
