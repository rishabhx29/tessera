//! First-run model delivery: pinned downloads, checksum verification, and the
//! version bookkeeping that keeps vector spaces honest.
//!
//! The rules this module exists to enforce, from ADR-0004, ADR-0008 and
//! SECURITY.md:
//!
//! - **Nothing runs unverified.** A file that does not hash to its pin is never
//!   written to the models directory. The pin is compiled into the binary, so
//!   it cannot be edited at runtime — a checksum you can change is not a pin.
//! - **One network access, ever.** Models are fetched once on first use. After
//!   that the engine never touches the network again, and never at query time.
//! - **A native OS engine needs no download.** Consumers ask for the sets they
//!   need by name, so an engine that ships with the operating system asks for
//!   nothing and does no network I/O at all.
//! - **Atomic.** A download lands on a temporary name and is renamed into place
//!   only after verification, so a half-written model never looks real.
//! - **A changed pin invalidates its own arm, and only that arm.** The version
//!   token lives in the index's `meta` table next to the vectors it describes.

pub mod fetch;
pub mod manifest;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::store::{Arm, Store};

pub use manifest::{set_version, Manifest, ModelFile, ModelSet};

/// What one `ensure` did. Returned rather than printed: this module has no
/// business deciding how a download looks on screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallReport {
    /// Files fetched this run.
    pub downloaded: Vec<String>,
    /// Files already present and matching their pin — the warm path.
    pub already_present: Vec<String>,
    /// Files that were present but did not match their pin, and were replaced.
    /// Reported rather than hidden: something else touched the models dir.
    pub replaced: Vec<String>,
    /// Bytes fetched this run.
    pub bytes: u64,
}

impl InstallReport {
    /// Whether this run needed the network at all.
    pub fn touched_network(&self) -> bool {
        !self.downloaded.is_empty()
    }

    /// One line per file, for a CLI or a settings screen.
    pub fn lines(&self) -> Vec<String> {
        self.replaced
            .iter()
            .map(|name| format!("{name} did not match its pin; reinstalled"))
            .chain(
                self.downloaded
                    .iter()
                    .map(|name| format!("downloaded {name}")),
            )
            .chain(
                self.already_present
                    .iter()
                    .map(|name| format!("verified {name}")),
            )
            .collect()
    }
}

/// The state of one model set on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetStatus {
    pub name: String,
    pub arm: Option<Arm>,
    /// Files present and matching their pin.
    pub installed: Vec<String>,
    /// Files missing, or present but not matching.
    pub missing: Vec<String>,
    pub version: String,
}

impl SetStatus {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

/// The models directory, and what it takes to fill it.
#[derive(Debug, Clone)]
pub struct Models {
    dir: PathBuf,
    manifest: Manifest,
}

impl Models {
    /// The models directory with the manifest compiled into this build.
    pub fn open(dir: &Path) -> Result<Self> {
        Ok(Self::with_manifest(dir, Manifest::pinned()?))
    }

    /// The models directory with an explicit manifest. Tests use this to point
    /// at a local server; production has one manifest and one call.
    pub fn with_manifest(dir: &Path, manifest: Manifest) -> Self {
        Self {
            dir: dir.to_path_buf(),
            manifest,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Where a file inside a set lives.
    pub fn path_of(&self, file: &ModelFile) -> PathBuf {
        self.dir.join(&file.name)
    }

    /// Make sure the named sets are installed and verified.
    ///
    /// Asks for the sets a consumer needs and nothing else, which is how a
    /// native OCR engine stays download-free. An unknown name is an error: a
    /// typo must not silently install nothing and look like success.
    pub fn ensure(&self, sets: &[&str]) -> Result<InstallReport> {
        let chosen = self.manifest.select(sets)?;
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating the models dir {}", self.dir.display()))?;
        // An interrupted run leaves a temporary file; it is never reusable, so
        // it goes before anything else reads the directory.
        fetch::sweep_partials(&self.dir)?;

        let mut report = InstallReport::default();
        for set in chosen {
            for file in &set.files {
                self.ensure_file(file, &mut report)?;
            }
        }
        Ok(report)
    }

    /// Every set, installed or not.
    pub fn status(&self) -> Result<Vec<SetStatus>> {
        self.manifest
            .sets()
            .iter()
            .map(|set| {
                let mut installed = Vec::new();
                let mut missing = Vec::new();
                for file in &set.files {
                    if self.is_verified(file) {
                        installed.push(file.name.clone());
                    } else {
                        missing.push(file.name.clone());
                    }
                }
                Ok(SetStatus {
                    name: set.name.clone(),
                    arm: set.arm,
                    installed,
                    missing,
                    version: set_version(set),
                })
            })
            .collect()
    }

    /// The names of every set in the manifest, for a CLI that installs all.
    pub fn set_names(&self) -> Vec<&str> {
        self.manifest
            .sets()
            .iter()
            .map(|s| s.name.as_str())
            .collect()
    }

    /// Whether one file is present and matches its pin.
    ///
    /// Read from disk every time, not remembered: the question "is this still
    /// the file we pinned?" has to keep being true after someone fills the disk
    /// or restores a backup over the directory. Callers reach this from
    /// indexing, never from a query — a search over the lexical arm does not
    /// touch this module at all.
    pub fn is_verified(&self, file: &ModelFile) -> bool {
        let path = self.path_of(file);
        matches!(fetch::digest_of(&path), Some(digest) if digest == file.sha256)
    }

    fn ensure_file(&self, file: &ModelFile, report: &mut InstallReport) -> Result<()> {
        if self.is_verified(file) {
            report.already_present.push(file.name.clone());
            return Ok(());
        }
        let path = self.path_of(file);
        // A file that is present but does not match its pin is never used. It
        // is also not left blocking the user: a pin moving is routine, and
        // making every user hand-delete files to get past it is a bug report
        // waiting to happen. It is replaced, and the report says so.
        if path.exists() {
            report.replaced.push(file.name.clone());
            std::fs::remove_file(&path)
                .with_context(|| format!("removing the unusable {}", path.display()))?;
        }
        let mut last: Option<anyhow::Error> = None;
        for source in file.sources() {
            match fetch::download_verified(source, &file.name, &path, &file.sha256, file.bytes) {
                Ok(digest) => {
                    report.downloaded.push(file.name.clone());
                    report.bytes += file.bytes;
                    debug_assert_eq!(digest, file.sha256);
                    return Ok(());
                }
                Err(err) => last = Some(err),
            }
        }
        let last =
            last.unwrap_or_else(|| anyhow::anyhow!("{} has no source to download from", file.name));
        Err(last).with_context(|| format!("installing {}", file.name))
    }

    /// The path of one file in a named set, **only if the whole set is installed
    /// and verified**.
    ///
    /// This is what an engine asks for before it loads anything. Two rules, and
    /// both come from a model run on half a set producing output that looks like
    /// text but is noise:
    ///
    /// - every file in the set must match its pin, not just the one being loaded;
    /// - the answer names the command that fixes it, because "file not found" for
    ///   a file that was never downloaded is a dead end.
    pub fn verified_path(&self, set: &str, file: &str) -> Result<PathBuf> {
        let set_record = self
            .manifest
            .set(set)
            .with_context(|| format!("no model set named {set:?} in the manifest"))?;
        let wanted = set_record
            .files
            .iter()
            .find(|candidate| candidate.name == file)
            .with_context(|| format!("model set {set:?} has no file named {file:?}"))?;
        let missing: Vec<&str> = set_record
            .files
            .iter()
            .filter(|candidate| !self.is_verified(candidate))
            .map(|candidate| candidate.name.as_str())
            .collect();
        if !missing.is_empty() {
            bail!(
                "the {set:?} models are not installed or do not match their pins ({} missing or \
                 unverified). Install them with: tessera models install --set {set}",
                missing.join(", ")
            );
        }
        Ok(self.path_of(wanted))
    }

    /// Whether every file of a named set is installed and verified. Cheaper than
    /// constructing the engine to find out, and the question a menu or a setup
    /// screen actually asks.
    pub fn is_set_ready(&self, set: &str) -> bool {
        self.manifest
            .set(set)
            .is_some_and(|set| set.files.iter().all(|file| self.is_verified(file)))
    }

    /// The arms whose vectors are not the ones their pinned model would produce,
    /// with the version now pinned.
    ///
    /// This is the question a caller actually has — "what has to be re-embedded
    /// before this index can be searched?" — so it is asked once, here, where
    /// the pins live. An arm with no vectors at all counts: never embedded is
    /// outstanding work in exactly the same sense as embedded by the wrong
    /// model.
    ///
    /// OCR models are skipped: they produce text, not vectors, so changing them
    /// invalidates nothing.
    pub fn stale_arms(&self, store: &Store) -> Result<Vec<StaleArm>> {
        let mut stale = Vec::new();
        for set in self.manifest.sets() {
            let Some(arm) = set.arm else { continue };
            let pinned = set_version(set);
            match store.arm_version(arm)? {
                Some(recorded) if recorded == pinned => {}
                recorded => stale.push(StaleArm {
                    arm,
                    pinned,
                    built_by: recorded,
                }),
            }
        }
        Ok(stale)
    }
}

/// An arm whose vectors are not the ones its pinned model would produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleArm {
    pub arm: Arm,
    /// The version the manifest pins now.
    pub pinned: String,
    /// The version the existing vectors were built by, or `None` if this arm has
    /// never been embedded.
    pub built_by: Option<String>,
}

impl StaleArm {
    /// Whether this arm holds vectors from a model we no longer pin, as opposed
    /// to holding none at all.
    pub fn is_from_a_different_model(&self) -> bool {
        self.built_by.is_some()
    }
}
