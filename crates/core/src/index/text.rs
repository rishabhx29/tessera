//! The text pipeline: txt, md, csv, log.

use std::path::Path;

use super::{Kind, Pipeline, Tile};

const EXTENSIONS: &[&str] = &["txt", "md", "csv", "log"];

/// Chunking rules from the architecture: ~200 words per tile with a 50-word
/// overlap, so a phrase split across a boundary is still findable.
const WORDS_PER_TILE: usize = 200;
const WORD_OVERLAP: usize = 50;

/// Splits text into word-window chunks with overlap.
pub fn chunk_text(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let step = WORDS_PER_TILE - WORD_OVERLAP;
    let mut chunks = Vec::new();
    let mut start = 0usize;
    loop {
        let end = (start + WORDS_PER_TILE).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += step;
    }
    chunks
}

/// Plain-text files: read, chunk, emit tiles.
pub struct TextPipeline;

impl Pipeline for TextPipeline {
    fn accepts(&self, path: &Path) -> Option<Kind> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())?
            .to_ascii_lowercase();
        EXTENSIONS.contains(&ext.as_str()).then_some(Kind::Text)
    }

    fn process(&self, path: &Path) -> anyhow::Result<Vec<Tile>> {
        use anyhow::Context;
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Ok(chunk_text(&text)
            .into_iter()
            .enumerate()
            .map(|(seq, text)| Tile {
                seq: seq as u32,
                text,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts(name: &str) -> Option<Kind> {
        TextPipeline.accepts(Path::new(name))
    }

    #[test]
    fn claims_the_text_extensions_and_names_the_kind() {
        assert_eq!(accepts("a.txt"), Some(Kind::Text));
        assert_eq!(accepts("b.MD"), Some(Kind::Text)); // case-insensitive
        assert_eq!(accepts("c.csv"), Some(Kind::Text));
        assert_eq!(accepts("d.log"), Some(Kind::Text));
        assert_eq!(accepts("e.png"), None);
        assert_eq!(accepts("f.pdf"), None);
        assert_eq!(accepts("noext"), None);
    }

    #[test]
    fn short_text_is_one_tile() {
        let chunks = chunk_text("one two three");
        assert_eq!(chunks, vec!["one two three"]);
    }

    #[test]
    fn empty_text_is_no_tiles() {
        assert!(chunk_text("").is_empty());
        assert!(chunk_text("   \n\t ").is_empty());
    }

    #[test]
    fn long_text_chunks_with_overlap() {
        let words: Vec<String> = (0..450).map(|i| format!("w{i}")).collect();
        let text = words.join(" ");
        let chunks = chunk_text(&text);
        assert_eq!(chunks.len(), 3);
        // overlap: the second chunk starts 150 words in, not 200
        assert!(chunks[1].starts_with("w150 "));
        assert!(chunks[0].starts_with("w0 "));
        assert!(chunks[2].contains("w449"));
    }

    #[test]
    fn boundary_length_text_is_not_duplicated() {
        let words: Vec<String> = (0..210).map(|i| format!("w{i}")).collect();
        let text = words.join(" ");
        let chunks = chunk_text(&text);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[1].starts_with("w150 "));
    }
}
