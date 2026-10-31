//! SHA-256 helpers and the file check shared by both corpus tiers.

use std::fmt;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;
    digest
        .iter()
        .fold(String::with_capacity(digest.len() * 2), |mut acc, byte| {
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

/// Streams `path` and returns its lowercase hex SHA-256 and byte length.
pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut len = 0u64;
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("reading {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        len += u64::try_from(n).context("read length does not fit in u64")?;
    }
    Ok((hex(&hasher.finalize()), len))
}

/// Whether a file on disk matches its pinned size and hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    /// Size and SHA-256 both match.
    Ok,
    /// The file does not exist.
    Missing,
    /// The file has a different length than pinned.
    SizeMismatch {
        /// Pinned size in bytes.
        expected: u64,
        /// Size found on disk.
        actual: u64,
    },
    /// The file has the pinned length but different content.
    HashMismatch {
        /// Pinned SHA-256.
        expected: String,
        /// SHA-256 found on disk.
        actual: String,
    },
}

impl FileStatus {
    /// True only for [`FileStatus::Ok`].
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok => f.write_str("ok"),
            Self::Missing => f.write_str("missing"),
            Self::SizeMismatch { expected, actual } => {
                write!(f, "size mismatch: expected {expected} bytes, found {actual}")
            }
            Self::HashMismatch { expected, actual } => {
                write!(f, "sha256 mismatch: expected {expected}, found {actual}")
            }
        }
    }
}

/// Compares the file at `path` with a pinned size and SHA-256.
pub fn check_file(path: &Path, expected_sha256: &str, expected_size: u64) -> Result<FileStatus> {
    let (actual, size) = match sha256_file(path) {
        Ok(found) => found,
        Err(err) if is_not_found(&err) => return Ok(FileStatus::Missing),
        Err(err) => return Err(err),
    };
    Ok(compare(expected_sha256, expected_size, &actual, size))
}

/// Compares in-memory bytes with a pinned size and SHA-256.
pub fn check_bytes(bytes: &[u8], expected_sha256: &str, expected_size: u64) -> FileStatus {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    compare(expected_sha256, expected_size, &sha256_hex(bytes), size)
}

fn compare(expected_sha256: &str, expected_size: u64, actual_sha256: &str, actual_size: u64) -> FileStatus {
    if actual_size != expected_size {
        FileStatus::SizeMismatch {
            expected: expected_size,
            actual: actual_size,
        }
    } else if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        FileStatus::HashMismatch {
            expected: expected_sha256.to_owned(),
            actual: actual_sha256.to_owned(),
        }
    } else {
        FileStatus::Ok
    }
}

fn is_not_found(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|io| io.kind() == ErrorKind::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    const ABC_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn sha256_of_known_vectors() {
        assert_eq!(sha256_hex(b"abc"), ABC_SHA);
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // smoke: output is always 64 lowercase hex chars
        assert_eq!(sha256_hex(b"x").len(), 64);
    }

    #[test]
    fn file_matching_hash_and_size_is_ok() {
        let dir = TempDir::new("hash-match");
        let file = dir.write("a.bin", b"abc");
        assert_eq!(sha256_file(&file).expect("hash"), (ABC_SHA.to_owned(), 3));
        assert_eq!(check_file(&file, ABC_SHA, 3).expect("check"), FileStatus::Ok);
        // smoke: uppercase pins are accepted too
        assert!(
            check_file(&file, &ABC_SHA.to_uppercase(), 3)
                .expect("check")
                .is_ok()
        );
    }

    #[test]
    fn file_with_other_content_is_a_hash_mismatch() {
        let dir = TempDir::new("hash-mismatch");
        let file = dir.write("a.bin", b"abd");
        let status = check_file(&file, ABC_SHA, 3).expect("check");
        assert_eq!(
            status,
            FileStatus::HashMismatch {
                expected: ABC_SHA.to_owned(),
                actual: sha256_hex(b"abd")
            }
        );
        assert!(!status.is_ok());
        assert!(
            status
                .to_string()
                .starts_with("sha256 mismatch: expected ba7816bf")
        );
    }

    #[test]
    fn file_with_other_length_is_a_size_mismatch() {
        let dir = TempDir::new("size-mismatch");
        let file = dir.write("a.bin", b"abcd");
        let status = check_file(&file, ABC_SHA, 3).expect("check");
        assert_eq!(
            status,
            FileStatus::SizeMismatch {
                expected: 3,
                actual: 4
            }
        );
        assert_eq!(status.to_string(), "size mismatch: expected 3 bytes, found 4");
    }

    #[test]
    fn absent_file_is_missing_not_an_error() {
        let dir = TempDir::new("hash-missing");
        let status = check_file(&dir.path().join("nope.bin"), ABC_SHA, 3).expect("check");
        assert_eq!(status, FileStatus::Missing);
        assert_eq!(status.to_string(), "missing");
    }

    #[test]
    fn bytes_check_mirrors_file_check() {
        assert_eq!(check_bytes(b"abc", ABC_SHA, 3), FileStatus::Ok);
        assert_eq!(
            check_bytes(b"abc", ABC_SHA, 4),
            FileStatus::SizeMismatch {
                expected: 4,
                actual: 3
            }
        );
        assert!(matches!(
            check_bytes(b"abd", ABC_SHA, 3),
            FileStatus::HashMismatch { .. }
        ));
    }
}
