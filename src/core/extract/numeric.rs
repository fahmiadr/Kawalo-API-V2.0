//! Query numbers read like Elysia's `t.Numeric`.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, Visitor};

/// A query parameter read with JavaScript's `Number(text)`: surrounding whitespace, exponents,
/// `0x`/`0o`/`0b` literals, `Infinity`, and blank text as 0. An empty value and anything `Number`
/// turns into `NaN` is [`Invalid`](Self::Invalid).
///
/// Declare DTO fields as `Option<Numeric>` and check them with
/// [`ValidationErrors::numeric`](super::ValidationErrors::numeric); the value is not range
/// checked here because Elysia reports format and range errors in separate stages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Numeric {
    Value(f64),
    Invalid,
}

impl Numeric {
    /// The number, `None` when it was not valid.
    pub fn value(self) -> Option<f64> {
        match self {
            Self::Value(n) => Some(n),
            Self::Invalid => None,
        }
    }

    fn parse(text: &str) -> Self {
        // TypeBox's `numeric` format rejects the empty string although `Number("")` is 0.
        if text.is_empty() {
            return Self::Invalid;
        }
        js_number(text).map_or(Self::Invalid, Self::Value)
    }
}

impl<'de> Deserialize<'de> for Numeric {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Text;

        impl Visitor<'_> for Text {
            type Value = Numeric;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("numeric text")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Numeric, E> {
                Ok(Numeric::parse(text))
            }
        }

        deserializer.deserialize_str(Text)
    }
}

/// JavaScript's `Number(text)`; `None` for what it reads as `NaN`. Blank text is 0.
pub(crate) fn js_number(text: &str) -> Option<f64> {
    let trimmed = text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    match trimmed {
        "" => return Some(0.0),
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let radix = match trimmed.get(..2) {
        Some("0x" | "0X") => Some(16),
        Some("0o" | "0O") => Some(8),
        Some("0b" | "0B") => Some(2),
        _ => None,
    };
    if let Some(radix) = radix {
        let digits = &trimmed[2..];
        // `from_str_radix` would also take a leading sign, which `Number()` does not.
        if !digits.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        // Precision loss above 2^53 matches JavaScript.
        return u128::from_str_radix(digits, radix).ok().map(|n| n as f64);
    }
    // Rust also accepts `inf` and `nan`, which JavaScript reads as NaN.
    if trimmed
        .bytes()
        .any(|b| b.is_ascii_alphabetic() && !matches!(b, b'e' | b'E'))
    {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|n| !n.is_nan())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_javascript_number_rules() {
        for (text, want) in [
            ("5", Some(5.0)),
            ("2.5", Some(2.5)),
            ("5.", Some(5.0)),
            (".5", Some(0.5)),
            ("+5", Some(5.0)),
            ("1e1", Some(10.0)),
            ("0x5", Some(5.0)),
            ("0XfF", Some(255.0)),
            ("-0", Some(0.0)),
            ("0b11", Some(3.0)),
            ("0o7", Some(7.0)),
            (" 2 ", Some(2.0)),
            ("   ", Some(0.0)),
            ("\u{a0}5", Some(5.0)),
            ("-1", Some(-1.0)),
            ("Infinity", Some(f64::INFINITY)),
            ("1e400", Some(f64::INFINITY)),
            ("", Some(0.0)),
            ("abc", None),
            ("NaN", None),
            ("inf", None),
            ("nan", None),
            ("0x", None),
            ("0x+5", None),
            ("0b12", None),
            ("1e", None),
            ("--1", None),
            ("-0x5", None),
            ("1_0", None),
        ] {
            assert_eq!(js_number(text), want, "{text:?}");
        }
    }

    #[test]
    fn an_empty_query_value_is_invalid_but_blank_is_zero() {
        assert_eq!(Numeric::parse(""), Numeric::Invalid);
        assert_eq!(Numeric::parse(" "), Numeric::Value(0.0));
    }
}
