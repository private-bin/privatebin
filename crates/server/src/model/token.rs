use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Delete token as computed by PrivateBin:
/// `hash_hmac('sha256', $pasteid, $serversalt)`, hex encoded.
pub fn delete_token(salt: &[u8], paste_id: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(salt).expect("HMAC accepts any key length");
    mac.update(paste_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Compares a client-supplied token in constant time.
pub fn verify_delete_token(salt: &[u8], paste_id: &str, token: &str) -> bool {
    let expected = delete_token(salt, paste_id);
    expected.as_bytes().ct_eq(token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_hmac() {
        // Reference: python3 -c "import hmac,hashlib;
        //   print(hmac.new(b'salt', b'0123456789abcdef', hashlib.sha256).hexdigest())"
        assert_eq!(
            delete_token(b"salt", "0123456789abcdef"),
            "cdb0d80ac35289efe26301c00ed28c01c7ba2b862876c3469387a7dbc3abe83a"
        );
    }

    #[test]
    fn verifies() {
        let token = delete_token(b"k", "0123456789abcdef");
        assert!(verify_delete_token(b"k", "0123456789abcdef", &token));
        assert!(!verify_delete_token(b"k", "0123456789abcdee", &token));
        assert!(!verify_delete_token(b"other", "0123456789abcdef", &token));
        assert!(!verify_delete_token(b"k", "0123456789abcdef", ""));
        assert!(!verify_delete_token(
            b"k",
            "0123456789abcdef",
            &token.to_uppercase()
        ));
    }
}
