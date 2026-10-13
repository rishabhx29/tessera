//! Getting a file down without ever letting a half-written file look real.
//!
//! A model file that is truncated, or half of one that was interrupted, is
//! worse than a missing one: it loads, and produces garbage. So a download
//! lands on a temporary name, is hashed as it streams, and is renamed into
//! place only after it matches its pin. At every instant the models directory
//! contains either the whole verified file or nothing.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

/// Reads a few hundred MB over whatever connection the user has, so the
/// timeout is generous; it exists to catch a dead socket, not a slow link.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(900);

/// How much to read at a time. Big enough that the syscall stops mattering,
/// small enough that the buffer is not a cache of the whole file.
const CHUNK: usize = 256 * 1024;

/// Lowercase hex. Written out because the digest types do not format
/// themselves, and a checksum is not a place to be clever.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// A checksum mismatch. Its own type because it is the one failure a user
/// must act on rather than retry around: something is wrong with the bytes
/// arriving, and the engine will not run them.
#[derive(Debug)]
pub struct ChecksumMismatch {
    pub name: String,
    pub expected: String,
    pub found: String,
}

impl std::fmt::Display for ChecksumMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} failed verification: expected sha256 {}, got {}. \
             The download is not what the manifest pins, so it has been deleted \
             and nothing was installed. Check the network or the mirror, then retry.",
            self.name, self.expected, self.found
        )
    }
}

impl std::error::Error for ChecksumMismatch {}

/// The temporary name a download lands on while it is in flight. The pid is in
/// the name so two Tessera instances downloading at once write to different
/// files rather than interleaving into one.
fn temp_path(dest: &Path) -> PathBuf {
    dest.with_extension(format!("{}.part", std::process::id()))
}

/// Whether a leftover `.part` belongs to a process that is still running.
fn is_ours(name: &str) -> bool {
    name.contains(&format!(".{}.part", std::process::id()))
}

/// An agent with a timeout. One per process: connections are pooled across the
/// files of a set.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .new_agent()
}

/// Fetch `url` into `dest`, verifying it against `expected_sha256` and
/// `expected_bytes` before the file appears under its real name.
///
/// The byte count is checked while streaming, so a connection that dies
/// mid-transfer is reported as a truncated download rather than as a checksum
/// failure: the two have different causes and different fixes.
///
/// Returns the sha256 that was written. The temporary file is removed on every
/// failure path, so an aborted download leaves nothing behind for the next run
/// to trip over.
pub fn download_verified(
    url: &str,
    name: &str,
    dest: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
) -> Result<String> {
    let parent = dest
        .parent()
        .with_context(|| format!("{} has no parent directory", dest.display()))?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let temp = temp_path(dest);
    let (digest, len) = match stream_to_temp(&temp, url) {
        Ok(found) => found,
        Err(err) => {
            let _ = std::fs::remove_file(&temp);
            return Err(err);
        }
    };
    if digest != expected_sha256 {
        let _ = std::fs::remove_file(&temp);
        // Say what arrived, not just that it was wrong: a short read and a
        // tampered file are different problems.
        let found = if len != expected_bytes {
            format!("{len} bytes, sha256 {digest}")
        } else {
            digest
        };
        return Err(ChecksumMismatch {
            name: name.to_string(),
            expected: format!("{expected_bytes} bytes, sha256 {expected_sha256}"),
            found,
        }
        .into());
    }
    std::fs::rename(&temp, dest).with_context(|| {
        format!(
            "moving the verified download {} into place at {}",
            temp.display(),
            dest.display()
        )
    })?;
    Ok(digest)
}

/// Stream one URL to the temporary path, hashing on the way. Returns the hex
/// digest and the byte count of what arrived.
fn stream_to_temp(temp: &Path, url: &str) -> Result<(String, u64)> {
    let response = agent()
        .get(url)
        .call()
        .with_context(|| format!("fetching {url}"))?;
    let mut reader = response.into_body().into_reader();
    let mut file =
        std::fs::File::create(temp).with_context(|| format!("creating {}", temp.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        let read = reader
            .read(&mut buf)
            .with_context(|| format!("downloading {url}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        std::io::Write::write_all(&mut file, &buf[..read])
            .with_context(|| format!("writing {}", temp.display()))?;
        total += read as u64;
    }
    // Durability before the rename: a file that survives the rename but not a
    // crash is a file that was never really downloaded.
    file.sync_all()
        .with_context(|| format!("flushing {}", temp.display()))?;
    if total == 0 {
        bail!("{url} returned no bytes");
    }
    Ok((hex(hasher.finalize().as_slice()), total))
}

/// The sha256 of a file already on disk, or `None` if it cannot be read.
pub fn digest_of(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buf[..read]),
            Err(_) => return None,
        }
    }
    Some(hex(hasher.finalize().as_slice()))
}

/// Remove the leftovers of an interrupted download, so a crash costs disk for a
/// moment and nothing else.
///
/// Partials belonging to this process are left alone: they may be in flight. A
/// partial belonging to *another* Tessera is indistinguishable from an abandoned
/// one, so it goes — the consequence is that a second instance's download fails
/// and is retried, never that a wrong file is installed, because nothing is ever
/// installed without matching its pin.
pub fn sweep_partials(dir: &Path) -> Result<usize> {
    let mut swept = 0;
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_partial = entry.path().extension().is_some_and(|ext| ext == "part");
        if is_partial && !is_ours(&name) {
            std::fs::remove_file(entry.path())?;
            swept += 1;
        }
    }
    Ok(swept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_left_by_a_previous_run_is_swept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.onnx.1234.part"), b"half a file").unwrap();
        std::fs::write(dir.path().join("model.onnx"), b"the real thing").unwrap();
        assert_eq!(sweep_partials(dir.path()).unwrap(), 1);
        assert!(!dir.path().join("model.onnx.1234.part").exists());
        assert!(
            dir.path().join("model.onnx").exists(),
            "a real model was swept"
        );
    }

    #[test]
    fn the_digest_matches_a_known_vector_and_a_missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        // SHA-256 of "abc" (FIPS 180-4 sample vector).
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            digest_of(&path).as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(digest_of(&dir.path().join("nope")), None);
    }

    #[test]
    fn hex_is_lowercase_and_zero_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn a_failed_fetch_leaves_no_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("model.onnx");
        let err = download_verified(
            "http://127.0.0.1:1/never",
            "model.onnx",
            &dest,
            &"a".repeat(64),
            16,
        );
        assert!(err.is_err());
        assert!(
            !dest.exists(),
            "a failed download left its real name behind"
        );
        assert_eq!(sweep_partials(dir.path()).unwrap(), 0);
    }

    #[test]
    fn the_mismatch_message_says_what_to_do() {
        let mismatch = ChecksumMismatch {
            name: "siglip-vision.int8.onnx".into(),
            expected: "aaaa".into(),
            found: "bbbb".into(),
        };
        let text = mismatch.to_string();
        assert!(text.contains("siglip-vision.int8.onnx"));
        assert!(text.contains("aaaa") && text.contains("bbbb"));
        assert!(
            text.contains("deleted"),
            "a silent failure is the one to avoid"
        );
    }
}
