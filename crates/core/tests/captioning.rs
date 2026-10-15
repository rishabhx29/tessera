//! Does captioning actually answer "search by description"?
//!
//! The requirement, stated plainly: an image showing something — say a man with
//! an ocean behind him — should be retrievable by *describing* it in words,
//! including words that appear nowhere in the picture. No OCR, no text in the
//! file, just "a man standing in front of the ocean".
//!
//! Captioning is the candidate for this: the model writes a sentence about the
//! image at index time, that sentence is indexed as text, and the lexical arm —
//! already measured at 0.965 on real files — does the retrieval. The question
//! this test answers is whether the caption is faithful enough that a person's
//! description matches it.
//!
//! ```
//! tessera models install --set caption
//! cargo test --release -p tessera-core --test captioning -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use tessera_core::vectors::caption::Captioner;

/// The names a person would plausibly type, matched case-insensitively against
/// whatever the model wrote.
fn mentions(caption: &str, words: &[String]) -> Vec<String> {
    let lower = caption.to_lowercase();
    words
        .iter()
        .filter(|word| lower.contains(&word.to_lowercase()))
        .map(|word| word.to_string())
        .collect()
}

fn label_words(label: &str) -> Vec<String> {
    label
        .split_whitespace()
        .filter(|w| w.len() > 3 && !COMMON.contains(&w.to_lowercase().as_str()))
        .map(|w| w.to_string())
        .collect()
}

#[test]
#[ignore = "needs the 502 MB caption model set"]
fn a_description_of_what_is_in_a_photo_finds_that_photo() {
    // Load, or report honestly that the models are not there. An `--ignored` run
    // that cannot do its job must not come back green.
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        });
    let models = tessera_core::models::Models::open(&dir).expect("models dir");
    // The error is reported, not swallowed: "not installed" and "installed but
    // would not load" need different fixes, and a silent panic names neither.
    let captioner = match Captioner::new(&models, 4) {
        Ok(captioner) => captioner,
        Err(err) => panic!(
            "the captioning arm could not start: {err:#}\nFix with: tessera models install --set \
             caption"
        ),
    };

    // The stock corpus, one labelled photo at a time.
    let folder = std::env::var("TESSERA_PHOTOS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(home).join("photos")
        });
    assert!(folder.is_dir(), "no photo folder at {}", folder.display());

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("jpg" | "jpeg" | "png")
            )
        })
        .collect();
    paths.sort();
    assert!(paths.len() >= 5, "{} photos is too few", paths.len());

    let mut hits = 0usize;
    let mut useful = 0usize;
    let n = paths.len();

    for path in &paths {
        let Ok(caption) = captioner.caption(path) else {
            eprintln!(
                "  {} -> no caption produced",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            continue;
        };
        let caption = caption.trim().to_string();
        let label = path
            .file_stem()
            .map(|s| s.to_string_lossy().replace(['-', '_'], " "))
            .unwrap_or_default();

        // The words the label says should be in the caption. This is the
        // acceptance criterion in the ticket's own terms: the description a
        // person would type is derivable from what the model wrote.
        let found = mentions(&caption, &label_words(&label));

        let ok = !found.is_empty();
        hits += usize::from(ok);
        useful += usize::from(!caption.trim().is_empty());

        eprintln!(
            "  {} {label:<40} -> {caption:?}{}",
            if ok { "HIT " } else { "miss" },
            if found.is_empty() {
                String::new()
            } else {
                format!("  (shared: {found:?})")
            }
        );
    }

    eprintln!("\n== captioning ==\nlabelled photos {n}\ncaption produced {useful}\nshares a word with its label {hits}");

    // The bar is low and stated as such: at least some captions must be faithful
    // enough that a human description overlaps them. A model that captioned
    // every image "a photo" would score zero here, which is what this catches.
    assert!(
        hits * 3 >= n,
        "only {hits} of {n} captions shared any word with their label: {:.2} is too few for \
         description search to work",
        hits as f64 / n as f64
    );
}

const COMMON: [&str; 6] = ["with", "from", "over", "white", "black", "here"];
