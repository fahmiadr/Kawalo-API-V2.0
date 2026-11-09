//! Pagination rules, ported from `pagination.util.ts`.

use crate::core::clean::clean_integer_f64;
use crate::core::constants::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

/// Raw `limit`/`offset` as they arrive in a query string (numeric, possibly fractional).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PaginationParams {
    pub limit: Option<f64>,
    pub offset: Option<f64>,
}

/// Normalised paging window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pagination {
    /// `limit == 0`: return everything.
    NoLimit,
    Page {
        take: u32,
        skip: u32,
    },
}

impl Pagination {
    /// Rows to fetch, `None` when unlimited.
    pub fn take(self) -> Option<u32> {
        match self {
            Self::NoLimit => None,
            Self::Page { take, .. } => Some(take),
        }
    }

    /// Rows to skip, `None` when unlimited.
    pub fn skip(self) -> Option<u32> {
        match self {
            Self::NoLimit => None,
            Self::Page { skip, .. } => Some(skip),
        }
    }

    pub fn is_no_limit(self) -> bool {
        self == Self::NoLimit
    }
}

/// Resolves paging parameters.
///
/// `limit == 0` means no limit; a missing or invalid limit becomes [`DEFAULT_PAGE_SIZE`]; a
/// positive limit is clamped to `1..=`[`MAX_PAGE_SIZE`]; `offset` is at least 0. Fractions are
/// truncated.
pub fn resolve_pagination(params: PaginationParams) -> Pagination {
    let limit = clean_integer_f64(params.limit);
    if limit == Some(0) {
        return Pagination::NoLimit;
    }
    let take = limit
        .unwrap_or(i64::from(DEFAULT_PAGE_SIZE))
        .clamp(1, i64::from(MAX_PAGE_SIZE));
    let skip = clean_integer_f64(params.offset)
        .unwrap_or(0)
        .clamp(0, i64::from(u32::MAX));
    // Both values were clamped into u32 range above.
    Pagination::Page {
        take: take as u32,
        skip: skip as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(limit: Option<f64>, offset: Option<f64>) -> Pagination {
        resolve_pagination(PaginationParams { limit, offset })
    }

    #[test]
    fn zero_limit_means_no_limit() {
        let page = resolve(Some(0.0), Some(50.0));
        assert!(page.is_no_limit());
        assert_eq!((page.take(), page.skip()), (None, None));
        assert!(resolve(Some(0.9), None).is_no_limit());
    }

    #[test]
    fn missing_or_invalid_limit_uses_the_default() {
        let expected = Pagination::Page { take: 20, skip: 0 };
        assert_eq!(resolve_pagination(PaginationParams::default()), expected);
        assert_eq!(resolve(Some(f64::NAN), None), expected);
    }

    #[test]
    fn limit_is_clamped_and_offset_floored() {
        assert_eq!(
            resolve(Some(500.0), Some(-3.0)),
            Pagination::Page { take: 100, skip: 0 }
        );
        assert_eq!(
            resolve(Some(-4.0), Some(7.9)),
            Pagination::Page { take: 1, skip: 7 }
        );
        assert_eq!(
            resolve(Some(25.0), Some(40.0)),
            Pagination::Page { take: 25, skip: 40 }
        );
    }
}
