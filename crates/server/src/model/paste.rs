use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use serde_json::{Map, Value};

use super::id::is_valid_id;
use crate::config::Formatter;
use crate::error::ApiError;

/// Standard alphabet, padding optional (PHP's strict `base64_decode` accepts
/// both), trailing bits tolerated.
const B64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

const MAX_IV_BYTES: usize = 16;
const MAX_SALT_BYTES: usize = 16;
const MIN_ITERATIONS: u64 = 10_000;
/// Clients use 100 000. A huge count makes every reader's browser spin in
/// PBKDF2 before the ciphertext can even be authenticated.
const MAX_ITERATIONS: u64 = 1_000_000;

/// A stored paste. `adata` is kept verbatim: clients use its JSON
/// serialization as the AEAD associated data.
#[derive(Debug, Clone, PartialEq)]
pub struct Paste {
    pub id: String,
    pub adata: Value,
    pub ct: String,
    /// Unix timestamp (seconds).
    pub created: i64,
    /// Unix timestamp (seconds); `None` never expires.
    pub expire_at: Option<i64>,
}

impl Paste {
    pub fn discussion(&self) -> bool {
        flag(&self.adata, 2)
    }

    pub fn burn_after_reading(&self) -> bool {
        flag(&self.adata, 3)
    }

    pub fn is_expired(&self, now: i64) -> bool {
        self.expire_at.is_some_and(|at| at <= now)
    }
}

fn flag(adata: &Value, index: usize) -> bool {
    adata.get(index).and_then(Value::as_u64) == Some(1)
}

/// A stored comment.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub id: String,
    pub paste_id: String,
    pub parent_id: String,
    pub adata: Value,
    pub ct: String,
    pub created: i64,
}

/// A validated paste creation request.
#[derive(Debug)]
pub struct NewPaste {
    pub adata: Value,
    pub ct: String,
    /// Requested expire key (unvalidated: unknown keys fall back to the default).
    pub expire: Option<String>,
    pub discussion: bool,
    pub burn_after_reading: bool,
}

/// A validated comment creation request.
#[derive(Debug)]
pub struct NewComment {
    pub adata: Value,
    pub ct: String,
    pub paste_id: String,
    pub parent_id: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ValidationLimits {
    /// Maximum length of `ct` in bytes.
    pub size_limit: u64,
    /// Whether discussions are enabled on this instance.
    pub discussion: bool,
}

/// Validates a paste creation body (`{"v":2,"adata":[...],"ct":...,"meta":{...}}`).
pub fn validate_paste(
    mut body: Map<String, Value>,
    limits: ValidationLimits,
) -> Result<NewPaste, ApiError> {
    let ct = take_ct(&mut body, limits)?;
    check_version(&body)?;

    let adata = body.remove("adata").ok_or(ApiError::InvalidData)?;
    let parts = adata.as_array().ok_or(ApiError::InvalidData)?;
    let [cipher, formatter, discussion, burn] = parts.as_slice() else {
        return Err(ApiError::InvalidData);
    };
    check_cipher_params(cipher)?;
    formatter
        .as_str()
        .and_then(Formatter::parse)
        .ok_or(ApiError::InvalidData)?;
    let discussion = zero_or_one(discussion)?;
    let burn_after_reading = zero_or_one(burn)?;
    if discussion && burn_after_reading {
        return Err(ApiError::InvalidData);
    }
    // adata is authenticated by the client's cipher, so it cannot be
    // rewritten here: refuse instead of silently disabling the discussion.
    if discussion && !limits.discussion {
        return Err(ApiError::DiscussionDisabled);
    }

    let expire = match body.remove("meta") {
        None | Some(Value::Null) => None,
        Some(Value::Object(mut meta)) => match meta.remove("expire") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s),
            Some(_) => return Err(ApiError::InvalidData),
        },
        Some(_) => return Err(ApiError::InvalidData),
    };

    Ok(NewPaste {
        adata,
        ct,
        expire,
        discussion,
        burn_after_reading,
    })
}

/// Validates a comment creation body
/// (`{"v":2,"adata":[cipher params(, pasteid, parentid)],"ct":...,"pasteid":...,"parentid":...}`).
pub fn validate_comment(
    mut body: Map<String, Value>,
    limits: ValidationLimits,
) -> Result<NewComment, ApiError> {
    let paste_id = take_id(&mut body, "pasteid").ok_or(ApiError::InvalidPasteId)?;
    let parent_id = take_id(&mut body, "parentid").ok_or(ApiError::InvalidData)?;
    let ct = take_ct(&mut body, limits)?;
    check_version(&body)?;
    let adata = body.remove("adata").ok_or(ApiError::InvalidData)?;
    check_comment_adata(&adata, &paste_id, &parent_id)?;
    Ok(NewComment {
        adata,
        ct,
        paste_id,
        parent_id,
    })
}

fn take_id(body: &mut Map<String, Value>, key: &str) -> Option<String> {
    match body.remove(key)? {
        Value::String(id) if is_valid_id(&id) => Some(id),
        _ => None,
    }
}

fn take_ct(body: &mut Map<String, Value>, limits: ValidationLimits) -> Result<String, ApiError> {
    let Some(Value::String(ct)) = body.remove("ct") else {
        return Err(ApiError::InvalidData);
    };
    if ct.len() as u64 > limits.size_limit {
        return Err(ApiError::TooLarge(limits.size_limit));
    }
    if ct.is_empty() || B64.decode(&ct).is_err() {
        return Err(ApiError::InvalidData);
    }
    Ok(ct)
}

fn check_version(body: &Map<String, Value>) -> Result<(), ApiError> {
    match body.get("v").and_then(Value::as_f64) {
        Some(2.0) => Ok(()),
        _ => Err(ApiError::InvalidData),
    }
}

fn zero_or_one(value: &Value) -> Result<bool, ApiError> {
    match value.as_u64() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(ApiError::InvalidData),
    }
}

/// `[iv, salt, iterations, keysize, tagsize, "aes", mode, compression]`
fn check_cipher_params(params: &Value) -> Result<(), ApiError> {
    check_cipher_slice(params.as_array().ok_or(ApiError::InvalidData)?)
}

/// Comment adata is either the bare cipher spec (older clients) or the spec
/// followed by `pasteid, parentid` (current clients bind the ids into the
/// AEAD associated data); the ids must then match the request.
fn check_comment_adata(adata: &Value, paste_id: &str, parent_id: &str) -> Result<(), ApiError> {
    let parts = adata.as_array().ok_or(ApiError::InvalidData)?;
    match parts.as_slice() {
        spec @ [_, _, _, _, _, _, _, _] => check_cipher_slice(spec),
        [spec @ .., bound_paste, bound_parent] if parts.len() == 10 => {
            check_cipher_slice(spec)?;
            if bound_paste.as_str() == Some(paste_id) && bound_parent.as_str() == Some(parent_id) {
                Ok(())
            } else {
                Err(ApiError::InvalidData)
            }
        }
        _ => Err(ApiError::InvalidData),
    }
}

fn check_cipher_slice(params: &[Value]) -> Result<(), ApiError> {
    let ok = (|| {
        let [
            iv,
            salt,
            iterations,
            keysize,
            tagsize,
            algo,
            mode,
            compression,
        ] = params
        else {
            return None;
        };
        let check = base64_len_within(iv.as_str()?, MAX_IV_BYTES)
            && base64_len_within(salt.as_str()?, MAX_SALT_BYTES)
            && (MIN_ITERATIONS..=MAX_ITERATIONS).contains(&iterations.as_u64()?)
            && matches!(keysize.as_u64()?, 128 | 192 | 256)
            && matches!(tagsize.as_u64()?, 64 | 96 | 128)
            && algo.as_str()? == "aes"
            && matches!(mode.as_str()?, "gcm" | "ctr" | "cbc")
            && matches!(compression.as_str()?, "zlib" | "none");
        Some(check)
    })();
    if ok == Some(true) {
        Ok(())
    } else {
        Err(ApiError::InvalidData)
    }
}

fn base64_len_within(s: &str, max: usize) -> bool {
    B64.decode(s)
        .is_ok_and(|bytes| !bytes.is_empty() && bytes.len() <= max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LIMITS: ValidationLimits = ValidationLimits {
        size_limit: 1024,
        discussion: true,
    };

    fn paste_body() -> Value {
        json!({
            "v": 2,
            "adata": [
                ["Vp3ZsxwpybbSyzRcEP8kHw==", "w2NsFhOrA0s=", 100000, 256, 128, "aes", "gcm", "zlib"],
                "plaintext", 0, 0
            ],
            "ct": "q4n9WsqW0dk+6r7Y1A8h2Q==",
            "meta": {"expire": "1week"}
        })
    }

    fn validate(body: Value) -> Result<NewPaste, ApiError> {
        validate_paste(body.as_object().unwrap().clone(), LIMITS)
    }

    fn with(mut body: Value, pointer: &str, value: Value) -> Value {
        *body.pointer_mut(pointer).unwrap() = value;
        body
    }

    #[test]
    fn accepts_official_client_paste() {
        let p = validate(paste_body()).unwrap();
        assert_eq!(p.expire.as_deref(), Some("1week"));
        assert!(!p.discussion && !p.burn_after_reading);
    }

    #[test]
    fn accepts_variants() {
        validate(with(paste_body(), "/adata/0/6", json!("ctr"))).unwrap();
        validate(with(paste_body(), "/adata/0/7", json!("none"))).unwrap();
        validate(with(paste_body(), "/adata/1", json!("markdown"))).unwrap();
        validate(with(paste_body(), "/adata/0/3", json!(128))).unwrap();
        validate(with(paste_body(), "/adata/0/4", json!(96))).unwrap();
        // Unpadded base64 and missing meta are tolerated.
        validate(with(paste_body(), "/adata/0/1", json!("w2NsFhOrA0s"))).unwrap();
        let mut body = paste_body();
        body.as_object_mut().unwrap().remove("meta");
        assert!(validate(body).unwrap().expire.is_none());
    }

    #[test]
    fn rejects_invalid_fields() {
        let cases = [
            ("/v", json!(1)),
            ("/v", json!("2")),
            ("/adata/0/0", json!("not base64!")),
            ("/adata/0/0", json!("")),
            ("/adata/0/0", json!("AAAAAAAAAAAAAAAAAAAAAAAA")), // 18 bytes
            ("/adata/0/1", json!("")),
            ("/adata/0/2", json!(1000)),
            ("/adata/0/2", json!("100000")),
            ("/adata/0/3", json!(512)),
            ("/adata/0/4", json!(32)),
            ("/adata/0/5", json!("des")),
            ("/adata/0/6", json!("ecb")),
            ("/adata/0/7", json!("gzip")),
            ("/adata/1", json!("html")),
            ("/adata/2", json!(2)),
            ("/adata/3", json!(true)),
            ("/ct", json!("")),
            ("/ct", json!("%%%")),
            ("/ct", json!(42)),
            ("/meta", json!("1week")),
            ("/meta/expire", json!(300)),
        ];
        for (pointer, value) in cases {
            assert_eq!(
                validate(with(paste_body(), pointer, value.clone())).unwrap_err(),
                ApiError::InvalidData,
                "{pointer} = {value}"
            );
        }
        assert!(validate(with(paste_body(), "/adata", json!([[], "plaintext", 0]))).is_err());
        assert!(validate(json!({"v": 2})).is_err());
    }

    #[test]
    fn rejects_discussion_with_burn() {
        let body = with(paste_body(), "/adata/2", json!(1));
        let body = with(body, "/adata/3", json!(1));
        assert_eq!(validate(body).unwrap_err(), ApiError::InvalidData);
    }

    #[test]
    fn rejects_discussion_when_disabled() {
        let body = with(paste_body(), "/adata/2", json!(1));
        let limits = ValidationLimits {
            discussion: false,
            ..LIMITS
        };
        let err = validate_paste(body.as_object().unwrap().clone(), limits).unwrap_err();
        assert_eq!(err, ApiError::DiscussionDisabled);
    }

    #[test]
    fn enforces_size_limit_on_ct() {
        let ct = "A".repeat(1028);
        let err = validate(with(paste_body(), "/ct", json!(ct))).unwrap_err();
        assert_eq!(err, ApiError::TooLarge(1024));
        let ct = "A".repeat(1024);
        validate(with(paste_body(), "/ct", json!(ct))).unwrap();
    }

    #[test]
    fn validates_comments() {
        let body = json!({
            "v": 2,
            "adata": ["Vp3ZsxwpybbSyzRcEP8kHw==", "w2NsFhOrA0s=", 100000, 256, 128, "aes", "gcm", "zlib"],
            "ct": "q4n9WsqW0dk+6r7Y1A8h2Q==",
            "pasteid": "0123456789abcdef",
            "parentid": "0123456789abcdef"
        });
        let c = validate_comment(body.as_object().unwrap().clone(), LIMITS).unwrap();
        assert_eq!(c.paste_id, "0123456789abcdef");

        // Nested (paste-style) adata is not valid for comments.
        let nested = with(body.clone(), "/adata", paste_body()["adata"].clone());
        assert!(validate_comment(nested.as_object().unwrap().clone(), LIMITS).is_err());

        // Current clients append [pasteid, parentid] to the cipher spec.
        let mut bound = body.clone();
        bound["adata"]
            .as_array_mut()
            .unwrap()
            .extend([json!("0123456789abcdef"), json!("0123456789abcdef")]);
        validate_comment(bound.as_object().unwrap().clone(), LIMITS).unwrap();
        for (pointer, value) in [
            ("/adata/8", json!("fedcba9876543210")),
            ("/adata/9", json!("fedcba9876543210")),
            ("/adata/9", json!(null)),
        ] {
            let mismatched = with(bound.clone(), pointer, value);
            assert_eq!(
                validate_comment(mismatched.as_object().unwrap().clone(), LIMITS).unwrap_err(),
                ApiError::InvalidData
            );
        }
        let mut nine = body.clone();
        nine["adata"]
            .as_array_mut()
            .unwrap()
            .push(json!("0123456789abcdef"));
        assert!(validate_comment(nine.as_object().unwrap().clone(), LIMITS).is_err());

        let bad_parent = with(body.clone(), "/parentid", json!("nope"));
        assert!(validate_comment(bad_parent.as_object().unwrap().clone(), LIMITS).is_err());

        let bad_paste = with(body, "/pasteid", json!("../x"));
        assert_eq!(
            validate_comment(bad_paste.as_object().unwrap().clone(), LIMITS).unwrap_err(),
            ApiError::InvalidPasteId
        );
    }

    #[test]
    fn paste_flags_from_adata() {
        let p = Paste {
            id: "0123456789abcdef".into(),
            adata: json!([[], "plaintext", 1, 0]),
            ct: String::new(),
            created: 0,
            expire_at: Some(10),
        };
        assert!(p.discussion());
        assert!(!p.burn_after_reading());
        assert!(!p.is_expired(9));
        assert!(p.is_expired(10));
    }
}
