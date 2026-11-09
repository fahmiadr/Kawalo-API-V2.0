//! Identifier generation, ported from `uuid.util.ts`.

use uuid::Uuid;

/// Random UUIDv4 in canonical hex form.
pub fn generate_uuid_v4() -> String {
    Uuid::new_v4().to_string()
}

/// UUIDv7 (RFC 9562): 48-bit millisecond timestamp, then random bits.
pub fn generate_uuid_v7() -> String {
    Uuid::now_v7().to_string()
}

/// `{prefix}_{uuidv7}`, for example `inc_01925b4a-...`. The constant prefix keeps B-tree order.
pub fn generate_prefixed_uuid_v7(prefix: &str) -> String {
    format!("{prefix}_{}", generate_uuid_v7())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v7_has_version_and_variant_bits() {
        let id = generate_uuid_v7();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "7");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"), "{id}");
    }

    #[test]
    fn uuid_v7_sorts_by_time() {
        let first = generate_uuid_v7();
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(first < generate_uuid_v7());
    }

    #[test]
    fn prefixed_id_keeps_the_prefix() {
        assert!(generate_prefixed_uuid_v7("feed").starts_with("feed_"));
        assert_eq!(generate_uuid_v4().len(), 36);
    }
}
