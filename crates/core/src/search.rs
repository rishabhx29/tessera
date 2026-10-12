//! Lexical search: split a query into terms, escape each one, intersect the
//! per-term file sets, and show a snippet around the first match.
//!
//! Escaping is the safety rule from the architecture: every term is quoted
//! before it reaches MATCH so operators and punctuation in user input are
//! literal text, never FTS5 syntax. Multi-word queries match across the
//! file's whole text — the terms may live in different tiles.

use crate::store::Store;

/// A search result: the file plus a snippet around the first matching term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub kind: String,
    pub snippet: String,
}

/// Quote one term for FTS5 MATCH: literal text, embedded quotes doubled.
pub fn escape_term(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// A ~N-character window around the first term found in `text`.
/// Case-insensitive; if no term appears, the text's start is shown.
pub fn snippet_around(text: &str, terms: &[String], window: usize) -> String {
    let lower = text.to_lowercase();
    let hit = terms
        .iter()
        .filter_map(|t| lower.find(&t.to_lowercase()))
        .min()
        .unwrap_or(0);
    let start = hit.saturating_sub(window / 3).min(text.len());
    let end = (start + window).min(text.len());
    let mut out = String::new();
    if start > 0 {
        out.push_str("...");
    }
    out.push_str(&text[start..end]);
    if end < text.len() {
        out.push_str("...");
    }
    out.replace('\n', " ")
}

/// Search the corpus: every term must appear somewhere in the file
/// (across tiles, not within one tile).
pub fn search(store: &Store, terms: &[String]) -> anyhow::Result<Vec<Hit>> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut sets = Vec::with_capacity(terms.len());
    for term in terms {
        sets.push(store.file_ids_matching(&escape_term(term))?);
    }
    let mut ids: Vec<i64> = sets
        .first()
        .map(|s| s.iter().copied().collect::<Vec<i64>>())
        .unwrap_or_default();
    for set in &sets[1..] {
        ids.retain(|id| set.contains(id));
    }
    ids.sort_unstable();

    let mut hits = Vec::with_capacity(ids.len());
    for id in ids {
        let (path, kind) = store.file_row(id)?;
        let texts = store.tile_texts(id)?;
        let joined = texts.join(" ");
        let snippet = snippet_around(&joined, terms, 240);
        hits.push(Hit {
            path,
            kind,
            snippet,
        });
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_quoted_literal() {
        assert_eq!(escape_term("inv0ice"), "\"inv0ice\"");
        // operators stay literal, never FTS5 syntax
        assert_eq!(escape_term("AND"), "\"AND\"");
        assert_eq!(escape_term("a*b"), "\"a*b\"");
        // embedded quotes are doubled
        assert_eq!(escape_term("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn snippet_windows_around_the_match() {
        let text = "prefix ".to_string()
            + &"x".repeat(300)
            + " INVOICE from the airline "
            + &"y".repeat(300);
        let terms = vec!["invoice".to_string()];
        let s = snippet_around(&text, &terms, 60);
        assert!(s.contains("INVOICE"));
        assert!(s.starts_with("...") && s.ends_with("..."));
        assert!(s.chars().count() <= 66); // window + ellipses
    }

    #[test]
    fn snippet_without_match_shows_the_start() {
        let s = snippet_around("hello world", &["zebra".into()], 20);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn snippet_collapses_newlines() {
        let s = snippet_around("line one\nline two", &["two".into()], 40);
        assert!(!s.contains('\n'));
    }
}
