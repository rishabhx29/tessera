//! Content hashing: the `sha1` column is what proves two paths hold the same
//! bytes, so a moved file updates one row instead of being indexed twice, and
//! a file whose mtime moved but whose content did not is not re-extracted.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use sha1::Digest;

/// Lowercase hex sha1 of a byte slice.
pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Lowercase hex sha1 of a file's contents, read in chunks so that indexing a
/// near-cap file does not pull the whole thing into memory.
pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("opening {} to hash it", path.display()))?;
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buf)
            .with_context(|| format!("hashing {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// The hash of one file, read at most once.
///
/// Change detection wants the hash only when size and mtime disagree, and the
/// move check wants it for a file it has never seen. Asking both questions
/// must not cost two reads of a 40 MB screenshot.
pub struct LazyHash<'a> {
    path: &'a Path,
    value: Option<String>,
}

impl<'a> LazyHash<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self { path, value: None }
    }

    /// The file's hash, reading it the first time only.
    pub fn get(&mut self) -> Result<&str> {
        if self.value.is_none() {
            self.value = Some(sha1_file(self.path)?);
        }
        Ok(self.value.as_deref().unwrap_or_default())
    }

    /// Whether anything has been read yet.
    pub fn is_read(&self) -> bool {
        self.value.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha1_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn a_span_of_multiple_chunks_hashes_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        // Larger than the 64 KiB read buffer, so the loop really iterates.
        let body = "tessera".repeat(40_000);
        std::fs::write(&path, &body).unwrap();
        assert_eq!(sha1_file(&path).unwrap(), sha1_hex(body.as_bytes()));
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_hash() {
        let dir = tempfile::tempdir().unwrap();
        assert!(sha1_file(&dir.path().join("nope.txt")).is_err());
    }

    #[test]
    fn a_lazy_hash_reads_once_and_answers_the_same_twice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "original").unwrap();
        let mut lazy = LazyHash::new(&path);
        assert!(!lazy.is_read(), "asking before needing must not read");
        let first = lazy.get().unwrap().to_string();
        assert!(lazy.is_read());
        // The file changes underneath: a cached hash is still the hash of
        // what was read, which is the point.
        std::fs::write(&path, "different").unwrap();
        assert_eq!(lazy.get().unwrap(), first);
        assert_eq!(lazy.get().unwrap(), sha1_hex(b"original"));
    }

    #[test]
    fn a_lazy_hash_of_a_missing_file_stays_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.txt");
        assert!(LazyHash::new(&path).get().is_err());
        let mut lazy = LazyHash::new(&path);
        assert!(lazy.get().is_err());
        assert!(
            lazy.get().is_err(),
            "a failure must not be cached as a hash"
        );
    }
}
