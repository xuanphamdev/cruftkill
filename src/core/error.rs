//! Typed error for the nodemoduleskiller core library.
//!
//! Library code returns `Result<T, NpkillError>`. The binary uses
//! `anyhow::Result` and converts via `?`.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum NpkillError {
    /// A delete or scan path resolves outside its declared root.
    #[error("path is not within scan root: {0}")]
    PathEscape(PathBuf),

    /// Wraps `std::io::Error` from filesystem operations.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The supplied root path is missing, not a directory, or unreadable.
    #[error("invalid root: {0}")]
    InvalidRoot(String),

    /// Size calculation exceeded the per-folder timeout.
    #[error("size calculation timed out for: {0}")]
    SizeTimeout(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_escape_includes_path_in_message() {
        let e = NpkillError::PathEscape(PathBuf::from("/etc/passwd"));
        assert!(e.to_string().contains("/etc/passwd"));
    }

    #[test]
    fn invalid_root_includes_reason() {
        let e = NpkillError::InvalidRoot("does not exist".into());
        assert!(e.to_string().contains("does not exist"));
    }

    #[test]
    fn size_timeout_includes_path() {
        let e = NpkillError::SizeTimeout(PathBuf::from("/big/tree"));
        assert!(e.to_string().contains("/big/tree"));
    }

    #[test]
    fn io_error_converts_via_from() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let e: NpkillError = io.into();
        match e {
            NpkillError::Io(_) => {}
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
