//! PrivateBin v2 data model: identifiers, delete tokens, request validation
//! and the records persisted by the storage backends.

mod id;
mod paste;
mod token;

pub use id::{generate_id, is_valid_id};
pub use paste::{
    Comment, NewComment, NewPaste, Paste, ValidationLimits, validate_comment, validate_paste,
};
pub use token::{delete_token, verify_delete_token};
