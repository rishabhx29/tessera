//! A guard: no private path ever enters this repository.
//!
//! The benchmarks in this crate run against real user corpora — the developer's
//! screenshots, and at one point a phone backup containing 6,077 personal
//! photographs. Their *content* is never committed and their paths never are,
//! because SECURITY.md says the index contains the extracted text of a user's
//! files and anything attached to a report or a commit must be sanitised.
//!
//! The risk is not deliberate. It is that a benchmark prints a corpus path or an
//! OCR string on a miss, someone quotes command output into a commit message,
//    and a private filename ships in a public repository. That is the failure
//! this test prevents: it scans the repository for the two directories the
//! private corpora lived in and fails if either string appears anywhere.
//!
//! It is cheap and it is run by `cargo test --all`, so the guard holds for as
//! long as the repo does.

/// The two private corpora this guard covers, assembled at runtime so neither
/// is spelled out anywhere. One is the developer's own screenshot folder, the
/// other was a phone backup of 6,077 photographs. Neither may appear in source,
/// tests, docs, or any other file this test can read (SECURITY.md: the index
/// holds the extracted text of a user's files, and nothing private ships).
///
/// Built at runtime rather than written whole, because this file is *inside* the
/// repository the sweep covers — a `concat!` still leaves the assembled literal
/// findable in the source, so no literal is written anywhere.
fn backup_root() -> String {
    ["Back", "up ", "vi", "vo"].concat()
}

/// The other root, assembled the same way.
fn screenshots_root() -> String {
    ["OneDrive", "\\Picture", "s\\Screen", "shots"].concat()
}

/// The markers the sweep looks for, assembled at runtime so none of them appears
/// literally in any file the sweep reads.
fn private_markers() -> Vec<String> {
    vec![backup_root(), screenshots_root()]
}

#[test]
fn no_private_corpus_path_appears_in_the_repository() {
    let root = repo_root();
    let mut checked = 0usize;
    let mut pending = vec![root.clone()];

    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // `.git` holds objects and history, which this test cannot read
                // and must not try to; `target` is build output. Neither is
                // source, so neither is scanned.
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if matches!(name.as_ref(), ".git" | "target" | "node_modules") {
                    continue;
                }
                pending.push(path);
                continue;
            }
            // Only files a text search can read: binaries would just be noise,
            // and an image in a commit would be a different problem entirely.
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !matches!(
                extension.as_str(),
                "rs" | "md" | "toml" | "json" | "yml" | "yaml" | "txt" | "lock"
            ) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            checked += 1;
            for marker in private_markers() {
                assert!(
                    !text.contains(marker.as_str()),
                    "{} names a private corpus ({marker:?}). A developer's own files are not \
                     repository content (SECURITY.md). Point a benchmark at a corpus with an \
                     environment variable, and never write the path down.",
                    path.strip_prefix(&root).unwrap_or(&path).display()
                );
            }
        }
    }

    assert!(
        checked >= 40,
        "the sweep should have covered the repository, saw {checked} files"
    );
    // The guard itself must be hunting for something real: a list that was
    // quietly emptied would still pass the sweep above.
    for marker in private_markers() {
        assert!(
            marker.len() > 6,
            "the guard's private-path markers look empty or wrong: {marker:?}"
        );
    }
    assert!(
        private_markers().len() >= 2,
        "the guard covers more than one corpus"
    );
}

/// Feeds the guard a sample of the shape it must catch, so the sweep is proven
/// to match rather than assumed to. The sample is built from the same fragments
/// the markers use, so no private path is written into this file.
#[test]
fn the_guard_would_actually_catch_a_private_path() {
    let sample = format!(
        "something in {} and something in {}",
        backup_root(),
        screenshots_root()
    );
    let catches: Vec<bool> = {
        let markers = private_markers();
        markers
            .iter()
            .map(|marker| sample.contains(marker.as_str()))
            .collect()
    };
    assert!(
        catches.iter().all(|c| *c),
        "the guard no longer matches the shape it was built for: {catches:?}"
    );
}

/// The repository root: this test's own directory with `crates/core/tests`
/// stripped off.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("a repo root two levels up")
        .to_path_buf()
}
