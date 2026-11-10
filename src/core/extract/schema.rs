//! Declarative request-body schemas that fail with the exact text of Elysia's TypeBox validator.
//!
//! A DTO describes its fields once (type, limits, optional or nullable) and implements [`Body`].
//! [`Schema::check`] walks the parsed JSON in schema order, the way TypeBox reports errors, so the
//! `details` of a 422 match the TypeScript server in development. Everything that is accepted or
//! rejected, and the order in which errors appear, follows TypeBox 0.34 as Elysia 1.4 drives it:
//!
//! * `null` (or a missing body) is not an object; any other value (number, string, array) is checked as an object
//!   without properties, so it fails on the first required field and passes when all are optional.
//! * Fields are visited in declaration order. A required field that is absent fails like a field of
//!   the wrong type, with `found: undefined`. Unknown properties are ignored.
//! * A nullable field answers every violation of its value with one `should be one of` message.
//! * At most [`MAX_ERRORS`] errors are listed.
//!
//! ```
//! use kawalo_api::core::extract::schema::{Field, Kind, NO_LIMIT, Schema};
//! use serde_json::json;
//!
//! const LOGIN: Schema = Schema::new(&[
//!     Field::required("email", Kind::email(NO_LIMIT)),
//!     Field::required("password", Kind::text(1, NO_LIMIT)),
//!     Field::nullable("note", Kind::text(0, 8)),
//! ]);
//!
//! let errors = LOGIN.check(&json!({"email": 5, "note": "far too long"})).unwrap_err();
//! assert_eq!(
//!     errors.to_string(),
//!     "/email: Expected property 'email' to be string but found: 5, \
//!      /password: Expected property 'password' to be string but found: undefined, \
//!      /note: Property 'note' should be one of: 'string', 'null'"
//! );
//! ```

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::numeric::js_number_to_string;
use super::validate::ValidationErrors;

/// Elysia lists at most this many TypeBox errors.
pub const MAX_ERRORS: usize = 64;

/// Upper bound that never fails (`maxLength` or `maxItems` left out).
pub const NO_LIMIT: usize = usize::MAX;

/// Marks a multipart file part, which JavaScript prints as `[object File]`.
pub(crate) const FILE_PART_KEY: &str = "\0file";

/// A request body type with its schema.
pub trait Body: DeserializeOwned {
    const SCHEMA: Schema;

    /// Checks `value` against [`SCHEMA`](Self::SCHEMA), then deserialises it.
    ///
    /// # Errors
    /// The `details` text of the 422 answer.
    fn parse(mut value: Value) -> Result<Self, String> {
        Self::SCHEMA.check(&value).map_err(|e| e.to_string())?;
        // Anything but `null` that passed counts as an object without properties.
        if !value.is_object() {
            value = Value::Object(Map::new());
        }
        // Reaching an error here means a DTO field disagrees with its schema.
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

/// The fields of a body object, in the order TypeBox reports them.
#[derive(Debug, Clone, Copy)]
pub struct Schema {
    fields: &'static [Field],
    defaults: bool,
}

impl Schema {
    pub const fn new(fields: &'static [Field]) -> Self {
        Self {
            fields,
            defaults: false,
        }
    }

    /// For schemas with a `default:` somewhere: Elysia then replaces a missing or `null` body
    /// with an empty object instead of reporting `Expected object`.
    pub const fn with_defaults(mut self) -> Self {
        self.defaults = true;
        self
    }

    /// # Errors
    /// Every violation, in schema order.
    pub fn check(&self, body: &Value) -> Result<(), ValidationErrors> {
        if body.is_null() && !self.defaults {
            return Err(ValidationErrors::root("Expected object"));
        }
        let object = body.as_object();
        let mut errors = ValidationErrors::new();
        for field in self.fields {
            field.check(&mut errors, object.and_then(|o| o.get(field.name)));
        }
        errors.truncate(MAX_ERRORS);
        errors.into_result()
    }
}

/// How a field may appear in the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presence {
    Required,
    /// `t.Optional(..)`: may be absent, not `null`.
    Optional,
    /// `t.Optional(t.Nullable(..))`.
    Nullable,
}

#[derive(Debug, Clone, Copy)]
pub struct Field {
    name: &'static str,
    kind: Kind,
    presence: Presence,
}

impl Field {
    pub const fn required(name: &'static str, kind: Kind) -> Self {
        Self::new(name, kind, Presence::Required)
    }

    pub const fn optional(name: &'static str, kind: Kind) -> Self {
        Self::new(name, kind, Presence::Optional)
    }

    pub const fn nullable(name: &'static str, kind: Kind) -> Self {
        Self::new(name, kind, Presence::Nullable)
    }

    const fn new(name: &'static str, kind: Kind, presence: Presence) -> Self {
        Self {
            name,
            kind,
            presence,
        }
    }

    fn check(&self, errors: &mut ValidationErrors, value: Option<&Value>) {
        match (value, self.presence) {
            (None, Presence::Optional | Presence::Nullable)
            | (Some(Value::Null), Presence::Nullable) => {}
            (_, Presence::Nullable) => {
                let mut own = ValidationErrors::new();
                self.kind.check(&mut own, self.name, value);
                if !own.is_empty() {
                    errors.push(
                        self.name,
                        format!(
                            "Property '{}' should be one of: '{}', 'null'",
                            self.name,
                            self.kind.union_label()
                        ),
                    );
                }
            }
            _ => self.kind.check(errors, self.name, value),
        }
    }
}

/// The type of a value and its limits; lengths count UTF-16 units like JavaScript.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Text {
        min: usize,
        max: usize,
        format: Format,
    },
    Number {
        min: f64,
        max: f64,
    },
    /// A union of string literals.
    OneOf(&'static [&'static str]),
    List {
        item: &'static Kind,
        min: usize,
        max: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    None,
    Email,
    /// Pattern `^[0-9]+$`.
    Digits,
}

impl Kind {
    pub const fn text(min: usize, max: usize) -> Self {
        Self::Text {
            min,
            max,
            format: Format::None,
        }
    }

    pub const fn email(max: usize) -> Self {
        Self::Text {
            min: 0,
            max,
            format: Format::Email,
        }
    }

    pub const fn digits() -> Self {
        Self::Text {
            min: 0,
            max: NO_LIMIT,
            format: Format::Digits,
        }
    }

    pub const fn number(min: f64, max: f64) -> Self {
        Self::Number { min, max }
    }

    pub const fn list(item: &'static Kind, min: usize, max: usize) -> Self {
        Self::List { item, min, max }
    }

    /// The JSON Schema `type`; a union of literals has none.
    fn type_name(&self) -> &'static str {
        match self {
            Self::Text { .. } => "string",
            Self::Number { .. } => "number",
            Self::OneOf(_) => "undefined",
            Self::List { .. } => "array",
        }
    }

    /// What Elysia prints for this member of a nullable union: its `format`, else its `type`.
    fn union_label(&self) -> &'static str {
        match self {
            Self::Text {
                format: Format::Email,
                ..
            } => "email",
            other => other.type_name(),
        }
    }

    fn check(&self, errors: &mut ValidationErrors, path: &str, value: Option<&Value>) {
        match (self, value) {
            (Self::Text { min, max, format }, Some(Value::String(text))) => {
                errors.length(path, text, *min..=*max);
                match format {
                    Format::None => {}
                    Format::Email => errors.email(path, text),
                    Format::Digits => errors.digits(path, text),
                }
            }
            (Self::Number { min, max }, Some(Value::Number(number))) => {
                errors.range(path, number.as_f64().unwrap_or(f64::NAN), *min..=*max);
            }
            (Self::OneOf(allowed), Some(Value::String(text)))
                if allowed.contains(&text.as_str()) => {}
            (Self::OneOf(allowed), _) => {
                let labels = vec!["'undefined'"; allowed.len()].join(", ");
                errors.push(
                    path,
                    format!(
                        "Property '{}' should be one of: {labels}",
                        path.replace('/', ".")
                    ),
                );
            }
            (Self::List { item, min, max }, Some(Value::Array(items))) => {
                if items.len() < *min {
                    errors.push(
                        path,
                        format!("Expected array length to be greater or equal to {min}"),
                    );
                }
                if items.len() > *max {
                    errors.push(
                        path,
                        format!("Expected array length to be less or equal to {max}"),
                    );
                }
                for (index, element) in items.iter().enumerate() {
                    item.check(errors, &format!("{path}/{index}"), Some(element));
                }
            }
            (Self::List { .. }, _) => errors.push(path, "Expected array"),
            (kind, found) => errors.push(
                path,
                format!(
                    "Expected property '{}' to be {} but found: {}",
                    path.replace('/', "."),
                    kind.type_name(),
                    js_string(found)
                ),
            ),
        }
    }
}

/// JavaScript `String(value)`; `None` is `undefined`.
fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_owned(),
        Some(Value::Null) => "null".to_owned(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => js_number_to_string(number.as_f64().unwrap_or(f64::NAN)),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(map)) if map.contains_key(FILE_PART_KEY) => "[object File]".to_owned(),
        Some(Value::Object(_)) => "[object Object]".to_owned(),
    }
}
