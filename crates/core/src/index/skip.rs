//! Skip rules: the decisions taken about a file *before* a single byte of it is
//! read. Two of them, both from the indexing policy in product-definition 3.3:
//!
//! - a cloud placeholder (OneDrive "files on demand") is recorded as skipped
//!   and **never hydrated** — indexing a cloud-backed Pictures folder must not
//!   download the cloud;
//! - a file above the size cap is recorded as skipped and counted, not
//!   chewed.
//!
//! Both are decided from metadata alone, and both land in the `skip_reason`
//! column, because "search can't find my scanned invoice" has to be
//! answerable from the diagnostics.

use std::path::Path;

use anyhow::{Context, Result};

/// Files larger than this are recorded as skipped rather than indexed.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// What the filesystem says about a file without reading its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileFacts {
    pub size: u64,
    /// Nanoseconds since the Unix epoch. Nanoseconds rather than seconds
    /// because second resolution cannot tell two edits in the same second
    /// apart, and a change that leaves the metadata identical is a change the
    /// cheap evidence would miss forever.
    pub mtime: i64,
    /// The file exists only as a cloud placeholder: its bytes are not on this
    /// machine, and reading them would fetch them from the network.
    pub placeholder: bool,
}

/// Why a file was deliberately not indexed. The string is what lands in the
/// `skip_reason` column and in the `tessera stats` breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// A cloud-backed placeholder. Never hydrated.
    CloudPlaceholder,
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::CloudPlaceholder => "cloud_placeholder",
            SkipReason::TooLarge => "too_large",
        }
    }
}

/// The skip verdict for a file, if any.
///
/// A placeholder is reported as a placeholder even when it is also oversized:
/// that is the reason a person can act on. Both checks are metadata-only, so
/// the answer arrives without opening the file.
pub fn skip_reason(facts: &FileFacts) -> Option<SkipReason> {
    if facts.placeholder {
        return Some(SkipReason::CloudPlaceholder);
    }
    if facts.size > MAX_FILE_BYTES {
        return Some(SkipReason::TooLarge);
    }
    None
}

/// `FILE_ATTRIBUTE_OFFLINE`: the file's data is not present on this machine.
pub const ATTR_OFFLINE: u32 = 0x0001_0000;
/// `FILE_ATTRIBUTE_RECALL_ON_OPEN`: *opening* it downloads it.
pub const ATTR_RECALL_ON_OPEN: u32 = 0x0004_0000;
/// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`: *reading* it downloads it.
pub const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// Whether a Windows file-attribute word marks a Files On-Demand placeholder.
///
/// Kept pure and platform-independent so the rule is tested on every platform;
/// only the call that produces the word is Windows-specific. These three bits
/// are the difference between "read this file" and "download this file".
pub fn attributes_mark_placeholder(attributes: u32) -> bool {
    attributes & (ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS) != 0
}

/// Where a file's facts come from. A trait because a Files On-Demand
/// placeholder cannot be conjured up by a test on demand: the alternative would
/// be shipping the skip rule untested and calling that trust.
pub trait FileProbe {
    /// Facts for `path`, or an error when the path cannot be examined at all.
    fn facts(&self, path: &Path) -> Result<FileFacts>;
}

/// The real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProbe;

impl FileProbe for SystemProbe {
    fn facts(&self, path: &Path) -> Result<FileFacts> {
        // symlink_metadata, never metadata: following a link is a read of the
        // thing it points at, which is exactly what this module promises not
        // to do.
        let meta = std::fs::symlink_metadata(path)
            .with_context(|| format!("examining {}", path.display()))?;
        Ok(FileFacts {
            size: meta.len(),
            mtime: mtime_nanos(&meta),
            placeholder: is_placeholder(&meta),
        })
    }
}

fn mtime_nanos(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

#[cfg(windows)]
fn is_placeholder(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    attributes_mark_placeholder(meta.file_attributes())
}

/// Windows is the platform where cloud placeholders appear in an indexed
/// corpus (OneDrive, and any Files On-Demand provider). macOS evicts files to
/// zero-byte stubs with a `com.apple.fileprovider.placeholder` xattr, which
/// std cannot read — that check lands with the macOS work in ticket 06 rather
/// than being guessed at here. Linux has no equivalent, so a zero-byte file is
/// an empty file and is indexed as one.
#[cfg(not(windows))]
fn is_placeholder(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(size: u64, placeholder: bool) -> FileFacts {
        FileFacts {
            size,
            mtime: 0,
            placeholder,
        }
    }

    #[test]
    fn an_ordinary_file_is_not_skipped() {
        assert_eq!(skip_reason(&facts(1234, false)), None);
        assert_eq!(skip_reason(&facts(0, false)), None);
    }

    #[test]
    fn the_cap_is_inclusive_at_the_limit() {
        assert_eq!(skip_reason(&facts(MAX_FILE_BYTES, false)), None);
        assert_eq!(
            skip_reason(&facts(MAX_FILE_BYTES + 1, false)),
            Some(SkipReason::TooLarge)
        );
        assert_eq!(MAX_FILE_BYTES, 50 * 1024 * 1024, "product-definition 3.3");
    }

    #[test]
    fn a_placeholder_is_skipped_and_never_hydrated() {
        assert_eq!(
            skip_reason(&facts(0, true)),
            Some(SkipReason::CloudPlaceholder)
        );
        // even when it is also oversized, the actionable reason wins
        assert_eq!(
            skip_reason(&facts(MAX_FILE_BYTES * 2, true)),
            Some(SkipReason::CloudPlaceholder)
        );
        assert_eq!(SkipReason::CloudPlaceholder.as_str(), "cloud_placeholder");
        assert_eq!(SkipReason::TooLarge.as_str(), "too_large");
    }

    #[test]
    fn every_recall_attribute_marks_a_placeholder() {
        assert!(attributes_mark_placeholder(ATTR_OFFLINE));
        assert!(attributes_mark_placeholder(ATTR_RECALL_ON_OPEN));
        assert!(attributes_mark_placeholder(ATTR_RECALL_ON_DATA_ACCESS));
        // the bits Windows sets on a real hydrated file
        assert!(!attributes_mark_placeholder(0x0020_0000 | 0x0080_0000));
        assert!(!attributes_mark_placeholder(0));
    }

    #[cfg(windows)]
    #[test]
    fn a_real_file_is_not_a_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "hello").unwrap();
        let facts = SystemProbe.facts(&path).unwrap();
        assert_eq!(facts.size, 5);
        assert!(!facts.placeholder);
        assert!(facts.mtime > 0);
    }
}
