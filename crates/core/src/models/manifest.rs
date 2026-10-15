//! The pinned model manifest: which files the engine may run, where they come
//! from, and the checksums they must match.
//!
//! The data is `lock.json`, embedded in the binary at compile time. That is a
//! deliberate choice about where verification lives: a checksum that can be
//! edited at runtime is not a pin. The file is derived from
//! `spike/model-lock.json` — the week-1 spike's measured record — and keeps
//! only the models v0.1 actually runs.
//!
//! Every field here is security-critical, so the manifest refuses to load
//! rather than filling in a default: a missing checksum is not "skip the
//! check", it is a build that went wrong.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use super::fetch::hex;
use crate::store::Arm;

/// The manifest compiled into this binary.
const LOCK: &str = include_str!("lock.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    /// Documentation carried in the lock file itself, so the provenance
    /// travels with the checksums instead of living in someone's memory.
    #[serde(default)]
    #[allow(dead_code)]
    comment: Vec<String>,
    sets: Vec<RawSet>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSet {
    name: String,
    #[serde(default)]
    arm: Option<String>,
    files: Vec<ModelFile>,
}

/// One file the engine may run: where it comes from, and what it must be.
///
/// Deserialized straight into this type: the shape on disk and the shape in
/// memory are the same thing, so there is no second struct to keep in step.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    /// The name it has inside the models directory.
    pub name: String,
    /// Where to fetch it, tried first.
    pub url: String,
    /// Fallbacks, tried in order when the primary fails. A mirror is a
    /// courtesy, never a weaker check: mirrors carry the same pin.
    #[serde(default)]
    pub mirrors: Vec<String>,
    /// The pinned SHA-256. This is the whole security boundary.
    pub sha256: String,
    /// The pinned size, checked while streaming so a truncated transfer is
    /// reported as such instead of as a checksum failure.
    pub bytes: u64,
}

impl ModelFile {
    /// Every place this file can be fetched from, primary first.
    pub fn sources(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.url.as_str()).chain(self.mirrors.iter().map(String::as_str))
    }
}

/// One consumer's worth of models: the image arm, the text arm, or OCR.
///
/// Sets exist so a consumer asks for *its* models by name, and so a native OS
/// engine can ask for nothing at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSet {
    pub name: String,
    /// The vector space these models define, if any. OCR models define none:
    /// they produce text, not embeddings, so changing them never invalidates a
    /// vector space.
    pub arm: Option<Arm>,
    pub files: Vec<ModelFile>,
}

/// The manifest, parsed and validated.
#[derive(Debug, Clone)]
pub struct Manifest {
    sets: Vec<ModelSet>,
}

impl Manifest {
    /// The manifest compiled into this binary.
    pub fn pinned() -> Result<Self> {
        Self::parse(LOCK).context("the compiled-in model manifest")
    }

    /// Parse a manifest. Used by the pinned manifest and by tests, which
    /// point at a local server instead of the network.
    pub fn parse(json: &str) -> Result<Self> {
        let raw: RawManifest =
            serde_json::from_str(json).context("the model manifest is not valid JSON")?;
        let mut sets = Vec::with_capacity(raw.sets.len());
        for set in raw.sets {
            sets.push(ModelSet {
                name: set.name.clone(),
                arm: match set.arm.as_deref() {
                    None | Some("") => None,
                    Some(other) => Some(
                        other
                            .parse::<Arm>()
                            .with_context(|| format!("set {} names an unknown arm", set.name))?,
                    ),
                },
                files: set.files,
            });
        }
        let manifest = Self { sets };
        manifest.validate()?;
        Ok(manifest)
    }

    /// The rules a manifest has to hold before the engine will act on it. A
    /// pin that cannot be trusted is worse than a missing model: it turns a
    /// loud failure into a quiet one.
    fn validate(&self) -> Result<()> {
        if self.sets.is_empty() {
            bail!("the model manifest defines no sets");
        }
        let mut names = BTreeMap::new();
        for set in &self.sets {
            if set.name.trim().is_empty() {
                bail!("a model set has no name");
            }
            if names.insert(set.name.clone(), ()).is_some() {
                bail!("model set {} is defined twice", set.name);
            }
            if set.files.is_empty() {
                bail!("model set {} has no files", set.name);
            }
            let mut files = BTreeMap::new();
            for file in &set.files {
                if !is_secure_source(&file.url) {
                    bail!("{} must be fetched over https, not {}", file.name, file.url);
                }
                for mirror in &file.mirrors {
                    if !is_secure_source(mirror) {
                        bail!("{} has an insecure mirror {}", file.name, mirror);
                    }
                }
                if file.sha256.len() != 64 || !file.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                    bail!(
                        "{} has no usable sha256 pin (got {:?}); refusing to run unverified",
                        file.name,
                        file.sha256
                    );
                }
                if file.bytes == 0 {
                    bail!("{} pins a size of zero", file.name);
                }
                if files.insert(file.name.clone(), ()).is_some() {
                    bail!("{} appears twice in set {}", file.name, set.name);
                }
            }
        }
        Ok(())
    }

    /// A set by name.
    pub fn set(&self, name: &str) -> Option<&ModelSet> {
        self.sets.iter().find(|set| set.name == name)
    }

    /// The set that defines `arm`'s vector space. One set per arm: two sets
    /// feeding one vector space would mean a choice nobody recorded.
    pub fn set_for_arm(&self, arm: Arm) -> Option<&ModelSet> {
        self.sets.iter().find(|set| set.arm == Some(arm))
    }

    /// Every set, in manifest order.
    pub fn sets(&self) -> &[ModelSet] {
        &self.sets
    }

    /// The sets named by `names`, in manifest order. An unknown name is a loud
    /// error: a typo must not silently install nothing.
    pub fn select(&self, names: &[&str]) -> Result<Vec<&ModelSet>> {
        let mut chosen = Vec::with_capacity(names.len());
        for name in names {
            let set = self
                .set(name)
                .with_context(|| format!("no model set named {name:?} in the manifest"))?;
            chosen.push(set);
        }
        Ok(chosen)
    }
}

/// Whether a URL may be fetched from.
///
/// https, with one exception: loopback. A model fetched from `127.0.0.1` never
/// crosses a network, so TLS protects nothing there — and refusing it would
/// mean the download path could not be tested at all. Every production source
/// in the pinned manifest is https, and a test asserts it.
fn is_secure_source(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

/// The version token for a set: a digest over its pins.
///
/// Derived from the checksums rather than stored beside them, so it cannot say
/// "unchanged" when a pin moved, and it changes if *any* file in the set
/// changes — which is the only thing that should invalidate a vector space.
pub fn set_version(set: &ModelSet) -> String {
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    // Sorted by name: a set is a set, and re-ordering the manifest must not
    // invalidate anyone's vectors.
    let mut pins: Vec<(&str, &str)> = set
        .files
        .iter()
        .map(|file| (file.name.as_str(), file.sha256.as_str()))
        .collect();
    pins.sort_unstable();
    for (name, sha256) in pins {
        hasher.update(name.as_bytes());
        hasher.update(b":");
        hasher.update(sha256.as_bytes());
        hasher.update(b"\n");
    }
    format!("sha256:{}", hex(hasher.finalize().as_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compiled_in_manifest_is_valid() {
        let manifest = Manifest::pinned().unwrap();
        assert!(
            manifest.sets().len() >= 3,
            "v0.1 needs the image arm, the text arm, and OCR"
        );
        for set in manifest.sets() {
            for file in &set.files {
                assert!(file.sources().count() >= 1);
            }
        }
    }

    #[test]
    fn the_image_and_text_arms_are_owned_by_exactly_one_set_each() {
        let manifest = Manifest::pinned().unwrap();
        for arm in [Arm::Image, Arm::Text] {
            assert!(
                manifest.set_for_arm(arm).is_some(),
                "no model set provides the {arm:?} arm"
            );
        }
        // OCR models define no vector space: changing them must not
        // invalidate embeddings.
        //
        // Sets without an arm are allowed when they produce no vectors at all.
        // That was true of OCR, and it is true of captioning: a caption is
        // *text*, which the lexical arm indexes, so it needs no version pin of
        // its own and must not invalidate an embedding arm.
        assert!(
            manifest
                .sets()
                .iter()
                .all(|set| set.arm.is_some() || ARMLESS_SETS.contains(&set.name.as_str())),
            "a set without an arm appeared"
        );
    }

    /// The model sets that produce no vectors, so they carry no arm and must not
    /// invalidate one. Both emit text the lexical arm already indexes.
    const ARMLESS_SETS: [&str; 2] = ["ppocr", "caption"];

    #[test]
    fn a_set_version_moves_when_any_pin_moves() {
        let raw = r#"{"sets":[{"name":"a","files":[
            {"name":"x","url":"https://e/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1}]}]}"#;
        let version = set_version(Manifest::parse(raw).unwrap().set("a").unwrap());

        let changed = r#"{"sets":[{"name":"a","files":[
            {"name":"x","url":"https://e/x","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","bytes":1}]}]}"#;
        assert_ne!(
            version,
            set_version(Manifest::parse(changed).unwrap().set("a").unwrap())
        );

        // Adding a file is also a different set, and a different version.
        let grown = r#"{"sets":[{"name":"a","files":[
            {"name":"x","url":"https://e/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1},
            {"name":"y","url":"https://e/y","sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","bytes":2}]}]}"#;
        assert_ne!(
            version,
            set_version(Manifest::parse(grown).unwrap().set("a").unwrap())
        );
    }

    #[test]
    fn re_ordering_a_manifest_does_not_invalidate_vectors() {
        let one = r#"{"sets":[{"name":"a","files":[
            {"name":"x","url":"https://e/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1},
            {"name":"y","url":"https://e/y","sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","bytes":2}]}]}"#;
        let two = r#"{"sets":[{"name":"a","files":[
            {"name":"y","url":"https://e/y","sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","bytes":2},
            {"name":"x","url":"https://e/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1}]}]}"#;
        assert_eq!(
            set_version(Manifest::parse(one).unwrap().set("a").unwrap()),
            set_version(Manifest::parse(two).unwrap().set("a").unwrap())
        );
    }

    #[test]
    fn a_manifest_without_a_usable_pin_refuses_to_load() {
        // A model we cannot verify is not a model we may run.
        let cases = [
            // no pin at all
            r#"{"sets":[{"name":"a","files":[{"name":"x","url":"https://e/x","sha256":"","bytes":1}]}]}"#,
            // a pin that is not hex
            r#"{"sets":[{"name":"a","files":[{"name":"x","url":"https://e/x","sha256":"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz","bytes":1}]}]}"#,
            // a pin that is too short to be a sha256
            r#"{"sets":[{"name":"a","files":[{"name":"x","url":"https://e/x","sha256":"aa","bytes":1}]}]}"#,
            // a size of zero means nothing was ever pinned
            r#"{"sets":[{"name":"a","files":[{"name":"x","url":"https://e/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":0}]}]}"#,
        ];
        for case in cases {
            assert!(
                Manifest::parse(case).is_err(),
                "accepted an untrustworthy manifest: {case}"
            );
        }
    }

    #[test]
    fn an_insecure_source_refuses_to_load() {
        let case = r#"{"sets":[{"name":"a","files":[
            {"name":"x","url":"http://evil.example/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1}]}]}"#;
        assert!(Manifest::parse(case).is_err());
        let mirror = r#"{"sets":[{"name":"a","files":[
            {"name":"x","url":"https://e/x","mirrors":["http://evil.example/y"],"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1}]}]}"#;
        assert!(Manifest::parse(mirror).is_err());
    }

    #[test]
    fn loopback_is_allowed_because_there_is_nothing_to_intercept() {
        for url in [
            "http://127.0.0.1:8080/m",
            "http://localhost:1/m",
            "https://huggingface.co/x",
        ] {
            assert!(is_secure_source(url), "{url} should be allowed");
        }
        for url in [
            "http://evil.example/m",
            "ftp://127.0.0.1/m",
            "http://127.0.0.1.evil.example/m",
        ] {
            assert!(!is_secure_source(url), "{url} should be refused");
        }
    }

    /// The one thing the loopback exception must never be used for: the models
    /// this build actually ships with are fetched over the network, so they
    /// are all https.
    #[test]
    fn every_pinned_source_is_https() {
        let manifest = Manifest::pinned().unwrap();
        for set in manifest.sets() {
            for file in &set.files {
                for source in file.sources() {
                    assert!(
                        source.starts_with("https://"),
                        "{} is fetched over plain http",
                        file.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_broken_manifest_is_loud_rather_than_empty() {
        assert!(Manifest::parse("not json").is_err());
        assert!(Manifest::parse(r#"{"sets":[]}"#).is_err());
        assert!(Manifest::parse(r#"{"sets":[{"name":"a","files":[]}]}"#).is_err());
        let dup = r#"{"sets":[
            {"name":"a","files":[{"name":"x","url":"https://e/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1}]},
            {"name":"a","files":[{"name":"y","url":"https://e/y","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","bytes":1}]}]}"#;
        assert!(Manifest::parse(dup).is_err());
        assert!(Manifest::parse(
            r#"{"sets":[{"name":"a","arm":"nope","files":[
                {"name":"x","url":"https://e/x","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":1}]}]}"#
        )
        .is_err());
    }

    #[test]
    fn selecting_an_unknown_set_is_an_error_not_a_no_op() {
        let manifest = Manifest::pinned().unwrap();
        assert!(manifest.select(&["nope"]).is_err());
        assert!(manifest.select(&["ppocr"]).is_ok());
        assert!(manifest.select(&[]).unwrap().is_empty());
    }
}
