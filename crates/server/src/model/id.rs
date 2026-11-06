/// Generates a 64-bit identifier from the OS-seeded CSPRNG, as 16 hex chars.
pub fn generate_id() -> String {
    let mut bytes = [0u8; 8];
    rand::fill(&mut bytes);
    hex::encode(bytes)
}

/// Paste and comment identifiers are 16 lowercase hex characters.
pub fn is_valid_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_valid_and_distinct() {
        let a = generate_id();
        let b = generate_id();
        assert!(is_valid_id(&a));
        assert!(is_valid_id(&b));
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_malformed_ids() {
        assert!(is_valid_id("0123456789abcdef"));
        assert!(!is_valid_id("0123456789ABCDEF"));
        assert!(!is_valid_id("0123456789abcde"));
        assert!(!is_valid_id("0123456789abcdeg"));
        assert!(!is_valid_id("../../etc/passwd"));
        assert!(!is_valid_id(""));
    }
}
