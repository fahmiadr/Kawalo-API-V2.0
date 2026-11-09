//! Query and path validation (TypeBox `minLength`, `maxLength`, `format`, `minimum`, `maximum`)
//! with the 422 detail text, plus the pieces that [`schema`](super::schema) builds body errors
//! from. Request bodies are described by a [`Schema`](super::schema::Schema) instead.
//!
//! ```
//! use kawalo_api::core::extract::{Validate, ValidationErrors};
//!
//! struct Login { email: String, password: String, note: Option<String> }
//!
//! impl Validate for Login {
//!     fn validate(&self) -> Result<(), ValidationErrors> {
//!         let mut errors = ValidationErrors::new();
//!         errors.email("email", &self.email);
//!         errors.length("password", &self.password, 1..);
//!         errors.opt_length("note", self.note.as_deref(), ..=128);
//!         errors.into_result()
//!     }
//! }
//!
//! let bad = Login { email: "nope".into(), password: String::new(), note: None };
//! assert_eq!(
//!     bad.validate().unwrap_err().to_string(),
//!     "/email: Property 'email' should be email, /password: Expected string length greater or equal to 1"
//! );
//! ```

use std::fmt;
use std::ops::{Bound, RangeBounds};
use std::sync::LazyLock;

use regex::Regex;

use super::numeric::Numeric;

/// The `email` format of TypeBox as registered by Elysia.
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*@(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$",
    )
    .expect("static regex is valid")
});

/// Extra checks run after a body or query string deserialised successfully. The default accepts
/// everything, so a DTO without constraints needs only `impl Validate for Dto {}`.
pub trait Validate {
    /// # Errors
    /// All violated constraints, collected so the client sees them together.
    fn validate(&self) -> Result<(), ValidationErrors> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldError {
    path: String,
    message: String,
}

/// Collected constraint violations; displays as `/path: message, /path: message`.
///
/// Numeric query parameters are checked in two stages like Elysia: format errors are listed with
/// the others, but a range violation of the decoded number is only reported (alone, without a
/// path) when everything else passed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationErrors {
    errors: Vec<FieldError>,
    decoded: Vec<FieldError>,
}

impl ValidationErrors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a violation. `path` is a field name (`title`) or a pointer (`photos/0`); the
    /// leading `/` is added.
    pub fn push(&mut self, path: &str, message: impl Into<String>) {
        self.errors.push(FieldError {
            path: format!("/{}", path.trim_start_matches('/')),
            message: message.into(),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.decoded.is_empty()
    }

    /// An optional `t.Numeric` parameter with `bounds`; see the type docs for the two stages.
    pub fn numeric(&mut self, path: &str, value: Option<Numeric>, bounds: impl RangeBounds<f64>) {
        let n = match value {
            None => return,
            Some(Numeric::Invalid) => {
                self.push(
                    path,
                    format!("Property '{path}' should be one of: 'numeric', 'number'"),
                );
                return;
            }
            Some(Numeric::Value(n)) => n,
        };
        let mut range = Self::new();
        range.range(path, n, bounds);
        // The decoded value has no property path, hence the empty path.
        self.decoded
            .extend(range.errors.into_iter().map(|e| FieldError {
                path: String::new(),
                message: e.message,
            }));
    }

    /// `Ok(())` when nothing was recorded.
    ///
    /// # Errors
    /// `self`, when at least one violation was recorded.
    pub fn into_result(self) -> Result<(), Self> {
        if self.is_empty() { Ok(()) } else { Err(self) }
    }

    /// String length within `bounds`, counted in UTF-16 code units like JavaScript (and TypeBox).
    pub fn length(&mut self, path: &str, value: &str, bounds: impl RangeBounds<usize>) {
        let len = value.encode_utf16().count();
        if let Bound::Included(&min) | Bound::Excluded(&min) = bounds.start_bound()
            && len < min
        {
            self.push(
                path,
                format!("Expected string length greater or equal to {min}"),
            );
        }
        let max = match bounds.end_bound() {
            Bound::Included(&max) => Some(max),
            Bound::Excluded(&max) => max.checked_sub(1),
            Bound::Unbounded => None,
        };
        if let Some(max) = max
            && len > max
        {
            self.push(
                path,
                format!("Expected string length less or equal to {max}"),
            );
        }
    }

    /// [`length`](Self::length) for an optional value.
    pub fn opt_length(&mut self, path: &str, value: Option<&str>, bounds: impl RangeBounds<usize>) {
        if let Some(value) = value {
            self.length(path, value, bounds);
        }
    }

    pub fn email(&mut self, path: &str, value: &str) {
        if !EMAIL.is_match(value) {
            self.push(
                path,
                format!("Property '{}' should be email", path.replace('/', ".")),
            );
        }
    }

    /// Number within `bounds`; `NaN` and infinities always fail.
    pub fn range(&mut self, path: &str, value: f64, bounds: impl RangeBounds<f64>) {
        if !value.is_finite() {
            self.push(path, "Expected number");
            return;
        }
        if let Bound::Included(&min) = bounds.start_bound()
            && value < min
        {
            self.push(
                path,
                format!("Expected number to be greater or equal to {min}"),
            );
        }
        if let Bound::Included(&max) = bounds.end_bound()
            && value > max
        {
            self.push(
                path,
                format!("Expected number to be less or equal to {max}"),
            );
        }
    }

    /// An error about the whole value, which has no property path.
    pub(crate) fn root(message: impl Into<String>) -> Self {
        Self {
            errors: vec![FieldError {
                path: String::new(),
                message: message.into(),
            }],
            decoded: Vec::new(),
        }
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        self.errors.truncate(len);
    }

    /// Pattern `^[0-9]+$`.
    pub fn digits(&mut self, path: &str, value: &str) {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            self.push(path, "Expected string to match '^[0-9]+$'");
        }
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shown = if self.errors.is_empty() {
            self.decoded.get(..1).unwrap_or_default()
        } else {
            &self.errors
        };
        for (i, e) in shown.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{}: {}", e.path, e.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

/// Detail text for a query string that failed to deserialise: `/field: reason` (`missing field` errors point at
/// the missing field).
pub(crate) fn describe_path_error<E: fmt::Display>(err: &serde_path_to_error::Error<E>) -> String {
    use serde_path_to_error::Segment;

    let mut pointer = String::new();
    for segment in err.path().iter() {
        match segment {
            Segment::Map { key } => pointer.push_str(&format!("/{key}")),
            Segment::Seq { index } => pointer.push_str(&format!("/{index}")),
            Segment::Enum { variant } => pointer.push_str(&format!("/{variant}")),
            Segment::Unknown => {}
        }
    }
    let message = err.inner().to_string();
    if let Some(field) = message
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next())
    {
        return format!("{pointer}/{field}: Expected required property");
    }
    format!("{pointer}: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: impl FnOnce(&mut ValidationErrors)) -> String {
        let mut errors = ValidationErrors::new();
        f(&mut errors);
        errors.to_string()
    }

    #[test]
    fn length_counts_utf16_units() {
        // One emoji is two UTF-16 units but one char.
        assert_eq!(
            run(|e| e.length("a", "😀", ..=1)),
            "/a: Expected string length less or equal to 1"
        );
        assert_eq!(run(|e| e.length("a", "😀", ..=2)), "");
        assert_eq!(
            run(|e| e.length("a", "", 1..)),
            "/a: Expected string length greater or equal to 1"
        );
        assert_eq!(run(|e| e.length("a", "ok", 1..=128)), "");
    }

    #[test]
    fn email_follows_the_typebox_format() {
        for good in [
            "a@b.co",
            "Satpam1@Kawalo.Id",
            "user.name+tag@sub.domain.org",
        ] {
            assert_eq!(run(|e| e.email("email", good)), "", "{good}");
        }
        for bad in ["notemail", "a@b", "@b.co", "a b@c.co", "a@.co", ""] {
            assert_eq!(
                run(|e| e.email("email", bad)),
                "/email: Property 'email' should be email",
                "{bad}"
            );
        }
    }

    #[test]
    fn range_rejects_out_of_bounds_and_non_finite() {
        assert_eq!(
            run(|e| e.range("limit", 101.0, 0.0..=100.0)),
            "/limit: Expected number to be less or equal to 100"
        );
        assert_eq!(
            run(|e| e.range("offset", -1.0, 0.0..)),
            "/offset: Expected number to be greater or equal to 0"
        );
        assert_eq!(
            run(|e| e.range("limit", f64::NAN, 0.0..=100.0)),
            "/limit: Expected number"
        );
        assert_eq!(run(|e| e.range("limit", 1.5, 0.0..=100.0)), "");
    }

    #[test]
    fn errors_are_joined_and_digits_follow_the_pattern() {
        let out = run(|e| {
            e.length("a", "toolong", ..=3);
            e.digits("ids/1", "12a");
            e.digits("ids/2", "123");
        });
        assert_eq!(
            out,
            "/a: Expected string length less or equal to 3, /ids/1: Expected string to match '^[0-9]+$'"
        );
    }
}
