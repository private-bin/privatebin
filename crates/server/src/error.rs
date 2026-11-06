//! API errors, reported to clients the way PrivateBin does: HTTP 200 with
//! `{"status":1,"message":"..."}`.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    InvalidData,
    /// Size limit in bytes.
    TooLarge(u64),
    /// Configured rate limit in seconds.
    RateLimited(u64),
    PasteNotFound,
    InvalidPasteId,
    WrongDeleteToken,
    SavePaste,
    SaveComment,
    DiscussionDisabled,
    /// Comment size limit in bytes.
    CommentTooLarge(u64),
    TooManyComments,
}

impl ApiError {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidData => "Invalid data.".to_owned(),
            Self::TooLarge(limit) => format!(
                "Paste is limited to {} of encrypted data.",
                human_size(*limit)
            ),
            Self::RateLimited(secs) => {
                format!("Please wait {secs} seconds between each post.")
            }
            Self::PasteNotFound => {
                "Paste does not exist, has expired or has been deleted.".to_owned()
            }
            Self::InvalidPasteId => "Invalid paste ID.".to_owned(),
            Self::WrongDeleteToken => "Wrong deletion token. Paste was not deleted.".to_owned(),
            Self::SavePaste => "Error saving paste. Sorry.".to_owned(),
            Self::SaveComment => "Error saving comment. Sorry.".to_owned(),
            Self::DiscussionDisabled => "Discussion is disabled.".to_owned(),
            Self::CommentTooLarge(limit) => format!(
                "Comment is limited to {} of encrypted data.",
                human_size(*limit)
            ),
            Self::TooManyComments => {
                "This discussion is full: no more comments can be added.".to_owned()
            }
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ApiError {}

/// Formats a byte count with binary units, e.g. `10 MiB` or `1.50 KiB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value.fract() == 0.0 {
        format!("{value} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(10 * 1024 * 1024), "10 MiB");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.50 KiB");
    }

    #[test]
    fn messages_match_privatebin() {
        assert_eq!(
            ApiError::TooLarge(10_485_760).message(),
            "Paste is limited to 10 MiB of encrypted data."
        );
        assert_eq!(
            ApiError::RateLimited(10).message(),
            "Please wait 10 seconds between each post."
        );
    }
}
