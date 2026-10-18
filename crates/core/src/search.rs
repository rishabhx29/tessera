//! Lexical search: split a query into terms, tolerate lookalike typos,
//! intersect the per-term file sets, and show a snippet around the first match.
//!
//! Two safety rules from the architecture:
//! - every term is quoted before it reaches MATCH, so operators and
//!   punctuation in user input are literal text, never FTS5 syntax;
//! - trigram matching is *substring* matching. Typo tolerance is therefore a
//!   separate, explicit mechanism: each term is expanded into its lookalike
//!   variants (`0`↔`o`, `1`↔`l`↔`i`, `5`↔`s`, `8`↔`b`) and the variants are
//!   OR-ed — so `inv0ice` matches `invoice` because `invoice` is one of its
//!   variants. (gyotaku folds lookalikes into a second index at write time,
//!   which also survives OCR misreads in the corpus; that mechanism is a
//!   planned follow-up, noted in docs/product-definition.md.)
//!
//! Multi-term queries are AND across the file's whole text — the terms may
//! live in different tiles.

use std::collections::{HashMap, HashSet};

use crate::store::{Filter, Store};

/// A search result: the file plus a snippet around the first matching term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub kind: String,
    pub snippet: String,
    /// The page the snippet sits on, when the file has pages (a PDF). The page
    /// of the tile that matched, not of the file: "page 4" is where the words
    /// are.
    pub page: Option<u32>,
}

/// Character classes whose members are commonly confused for one another,
/// including by OCR and by people. Kept to single characters so query-side
/// expansion stays bounded. (gyotaku also folds `m`↔`rn` and `w`↔`vv`; those
/// need the write-time folded index, not query expansion.)
const LOOKALIKES: [&str; 4] = ["0o", "1li", "5s", "8b"];

/// Quote one term for FTS5 MATCH: literal text, embedded quotes doubled.
pub fn escape_term(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// All lookalike variants of a term, including the original. A typo changes
/// one character, so each position contributes at most one variant per
/// confusable partner — the variant count stays linear in the term length.
/// (`inv0ice` → `invoice`, `sh1p` → `ship`, `c0de` → `code`.)
pub fn lookalike_variants(term: &str) -> Vec<String> {
    let chars: Vec<char> = term.chars().collect();
    let mut variants = vec![term.to_string()];
    for (i, ch) in chars.iter().enumerate() {
        let lower = ch.to_ascii_lowercase();
        let Some(class) = LOOKALIKES.iter().find(|c| c.contains(lower)) else {
            continue;
        };
        for alt in class.chars() {
            if alt == lower {
                continue;
            }
            let mut variant = chars.clone();
            variant[i] = alt;
            variants.push(variant.into_iter().collect());
        }
    }
    variants.sort_unstable();
    variants.dedup();
    variants
}

/// The FTS5 MATCH expression for one term: its variants, quoted and OR-ed.
pub fn term_match_expr(term: &str) -> String {
    lookalike_variants(term)
        .iter()
        .map(|v| escape_term(v))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Byte offset of the first case-insensitive occurrence of `needle` in
/// `text`, in `text`'s own coordinates. Lowercasing can change byte lengths
/// (e.g. `İ`), so the lowercase string's offsets are mapped back per character
/// instead of being reused directly.
fn find_case_insensitive(text: &str, needle: &str) -> Option<usize> {
    let needle = needle.to_lowercase();
    if needle.is_empty() {
        return Some(0);
    }
    let mut lower = String::with_capacity(text.len());
    let mut offsets: Vec<(usize, usize)> = Vec::new(); // (offset in `lower`, offset in `text`)
    for (orig, ch) in text.char_indices() {
        for lc in ch.to_lowercase() {
            offsets.push((lower.len(), orig));
            lower.push(lc);
        }
    }
    lower.find(&needle).and_then(|pos| {
        offsets
            .iter()
            .rev()
            .find(|(lp, _)| *lp <= pos)
            .map(|(_, orig)| *orig)
    })
}

/// Move a byte offset back to the nearest character boundary at or before it.
fn floor_boundary(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Move a byte offset forward to the nearest character boundary at or after it.
fn ceil_boundary(text: &str, mut i: usize) -> usize {
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// A ~`window`-byte excerpt around the first term found in `text`,
/// case-insensitive, sliced on character boundaries. Without a match, the
/// text's start is shown.
pub fn snippet_around(text: &str, terms: &[&str], window: usize) -> String {
    let hit = terms
        .iter()
        .filter_map(|t| find_case_insensitive(text, t))
        .min()
        .unwrap_or(0);
    let start = floor_boundary(text, hit.saturating_sub(window / 3).min(text.len()));
    let end = ceil_boundary(text, (start + window).min(text.len()));
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

/// The lexical arm's result for fusion: files matching every term — across
/// tiles, not within one tile, the same contract as [`search`] — best first,
/// each with the tile that carried it.
///
/// Ranking is defined for every shape a query can take, because a query that
/// cannot be ranked must still be *ordered by something*:
///
/// - terms FTS5 can index (three characters or more) are ranked by BM25, and a
///   file's position is its *worst* position across the per-term lists: every
///   term must match, so the weakest match is what the file honestly earned;
/// - terms under three characters cannot be trigrams at all, so they narrow
///   the result (a `LIKE` scan) and contribute no order;
/// - a query of only short terms has no order to give, and comes back by file
///   id.
pub fn lexical_ranked(
    store: &Store,
    terms: &[&str],
    filter: &Filter,
) -> anyhow::Result<Vec<(i64, Option<i64>)>> {
    /// Below three characters there is no trigram to match, so FTS5 cannot
    /// answer and the fallback scans. The tokenizer's limit, not a choice.
    const TRIGRAM_CHARS: usize = 3;

    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let (long, short): (Vec<&str>, Vec<&str>) = terms
        .iter()
        .partition(|term| term.chars().count() >= TRIGRAM_CHARS);

    // Per term: position, and the tile that earned it. A file absent from any
    // one list is not a result at all, so the first list is the seed and each
    // further list narrows by intersection.
    let mut ranked: Option<Vec<(i64, Option<i64>, usize)>> = None;
    for term in &long {
        let list = store.ranked_ids_for_expr(&term_match_expr(term), filter)?;
        match &mut ranked {
            None => {
                ranked = Some(
                    list.into_iter()
                        .enumerate()
                        .map(|(position, (file_id, tile_id))| (file_id, Some(tile_id), position))
                        .collect(),
                );
            }
            Some(kept) => {
                let position_of: HashMap<i64, usize> = list
                    .into_iter()
                    .enumerate()
                    .map(|(position, (file_id, _tile))| (file_id, position))
                    .collect();
                kept.retain_mut(|(file_id, _tile, worst)| {
                    let Some(position) = position_of.get(file_id) else {
                        return false;
                    };
                    // Every term must match, so the weakest match is the rank
                    // the file earned. The tile stays the one the first list
                    // matched: a snippet searches the tile for any term, so
                    // any matched tile shows the right thing.
                    *worst = (*worst).max(*position);
                    true
                });
            }
        }
        if ranked.as_ref().is_some_and(|kept| kept.is_empty()) {
            return Ok(Vec::new());
        }
    }

    for term in &short {
        let list = store.files_containing(term, filter)?;
        let allowed: HashSet<i64> = list.iter().map(|(file_id, _)| *file_id).collect();
        match &mut ranked {
            Some(kept) => kept.retain(|(file_id, _, _)| allowed.contains(file_id)),
            None => {
                // Only short terms so far: intersect the scans, ordered by
                // file id, carrying the first scan's tile.
                ranked = Some(
                    list.into_iter()
                        .map(|(file_id, tile_id)| (file_id, Some(tile_id), 0))
                        .collect(),
                );
            }
        }
    }

    let mut kept = ranked.unwrap_or_default();
    kept.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)));
    Ok(kept
        .into_iter()
        .map(|(file_id, tile_id, _)| (file_id, tile_id))
        .collect())
}

/// Search the corpus: every term must appear somewhere in the file (across
/// tiles, not within one tile), tolerating lookalike typos per term.
pub fn search(store: &Store, terms: &[&str]) -> anyhow::Result<Vec<Hit>> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut sets = Vec::with_capacity(terms.len());
    for term in terms {
        sets.push(store.file_ids_matching(&term_match_expr(term))?);
    }
    let mut ids: Vec<i64> = sets
        .first()
        .map(|s| s.iter().copied().collect::<Vec<i64>>())
        .unwrap_or_default();
    for set in &sets[1..] {
        ids.retain(|id| set.contains(id));
    }
    ids.sort_unstable();

    let files = store.files_by_ids(&ids)?;
    Ok(files
        .into_iter()
        .map(|file| {
            // The snippet is built here, not in the store: choosing the window
            // around a match is a decision about presentation. It comes from
            // the first tile that contains a term, because the tile is what
            // knows its page — a result that names a page must name the page
            // the words are on. A lookalike-only hit (matched through the FTS
            // variants, not the literal text) falls back to the first tile.
            let tile = file
                .tiles
                .iter()
                .find(|tile| {
                    terms
                        .iter()
                        .any(|term| find_case_insensitive(&tile.text, term).is_some())
                })
                .or_else(|| file.tiles.first());
            let (snippet, page) = match tile {
                Some(tile) => (snippet_around(&tile.text, terms, 240), tile.page),
                None => (String::new(), None),
            };
            Hit {
                path: file.path,
                kind: file.kind,
                snippet,
                page,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_quoted_literal() {
        assert_eq!(escape_term("inv0ice"), "\"inv0ice\"");
        assert_eq!(escape_term("AND"), "\"AND\"");
        assert_eq!(escape_term("a*b"), "\"a*b\"");
        assert_eq!(escape_term("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn lookalikes_cover_the_common_confusions() {
        assert!(lookalike_variants("inv0ice").contains(&"invoice".to_string()));
        assert!(lookalike_variants("c0de").contains(&"code".to_string()));
        assert!(lookalike_variants("5hip").contains(&"ship".to_string()));
        assert!(lookalike_variants("p1ne").contains(&"pine".to_string()));
        // the original is always among the variants
        assert!(lookalike_variants("inv0ice").contains(&"inv0ice".to_string()));
    }

    #[test]
    fn expansion_stays_linear_in_term_length() {
        let term = "0011a5b8";
        let variants = lookalike_variants(term);
        assert!(
            variants.len() <= 1 + term.chars().count() * 3,
            "{variants:?}"
        );
    }

    #[test]
    fn match_expr_ors_the_variants() {
        let expr = term_match_expr("inv0ice");
        assert!(expr.contains("\"invoice\""));
        assert!(expr.contains("\"inv0ice\""));
        assert!(expr.contains(" OR "));
    }

    #[test]
    fn find_case_insensitive_maps_offsets_across_case_changes() {
        // 'İ' lowercases to two chars; offsets must not shift with it
        let text = "İstanbul invoice";
        assert_eq!(find_case_insensitive(text, "invoice"), Some(10));
        assert_eq!(find_case_insensitive(text, "INVOICE"), Some(10));
    }

    #[test]
    fn snippet_never_panics_on_multibyte_boundaries() {
        let text = "İ".repeat(80) + " INVOICE";
        let s = snippet_around(&text, &["invoice"], 40);
        assert!(s.contains("INVOICE"));
        // and slicing around a multibyte char stays on boundaries
        let s2 = snippet_around("héllo wörld", &["wörld"], 12);
        assert!(s2.contains("wörld"));
    }

    #[test]
    fn snippet_windows_and_collapses_newlines() {
        let text = "prefix ".to_string() + &"x".repeat(300) + " INVOICE " + &"y".repeat(300);
        let s = snippet_around(&text, &["invoice"], 60);
        assert!(s.contains("INVOICE"));
        assert!(s.starts_with("...") && s.ends_with("..."));
        let s = snippet_around("line one\nline two", &["two"], 40);
        assert!(!s.contains('\n'));
        let s = snippet_around("hello world", &["zebra"], 20);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn a_hit_carries_the_page_its_match_sat_on() {
        use crate::index::skip::FileFacts;
        use crate::index::{Kind, Tile};
        use crate::store::Store;

        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        let facts = FileFacts {
            size: 10,
            mtime: 1,
            placeholder: false,
        };
        let report = corpus.path().join("report.pdf");
        store
            .index_file(
                corpus.path(),
                &report,
                Kind::Pdf,
                &facts,
                Some("abc"),
                &[
                    Tile {
                        seq: 0,
                        text: "report.pdf\na cover page".into(),
                        page: Some(1),
                    },
                    Tile {
                        seq: 1,
                        text: "the invoice from the airline".into(),
                        page: Some(4),
                    },
                ],
            )
            .unwrap();
        let note = corpus.path().join("note.txt");
        store
            .index_file(
                corpus.path(),
                &note,
                Kind::Text,
                &facts,
                Some("def"),
                &[Tile {
                    seq: 0,
                    text: "note.txt\nthe invoice from the bank".into(),
                    page: None,
                }],
            )
            .unwrap();

        let hits = search(&store, &["invoice"]).unwrap();
        let pdf = hits.iter().find(|h| h.kind == "pdf").unwrap();
        assert_eq!(
            pdf.page,
            Some(4),
            "the page comes from the tile the term is on, not the file's first tile"
        );
        assert!(pdf.snippet.contains("invoice"));
        let text = hits.iter().find(|h| h.kind == "text").unwrap();
        assert_eq!(text.page, None, "a format without pages says so");
    }

    /// A small indexed corpus for the fusion arm's ranking tests: one tile per
    /// text, no files on disk — the same shape the store's own tests use.
    fn index(store: &Store, name: &str, texts: &[&str]) {
        use crate::index::skip::FileFacts;
        use crate::index::{Kind, Tile};

        let tiles: Vec<Tile> = texts
            .iter()
            .enumerate()
            .map(|(seq, text)| Tile {
                seq: seq as u32,
                text: (*text).to_string(),
                page: None,
            })
            .collect();
        store
            .index_file(
                std::path::Path::new("/corpus"),
                std::path::Path::new(&format!("/corpus/{name}")),
                Kind::Text,
                &FileFacts {
                    size: 10,
                    mtime: 1,
                    placeholder: false,
                },
                Some("h"),
                &tiles,
            )
            .unwrap();
    }

    /// Every term is a vote: a file must hold all of them, and its rank is the
    /// worst position any one of them gave it — the weakest match is what the
    /// file honestly earned.
    #[test]
    fn lexical_ranked_intersects_files_and_ranks_by_the_weakest_term() {
        use crate::store::Store;

        let data = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        // Inserted a, b, c, d — id order is deliberately not the answer.
        index(&store, "a.txt", &["a.txt\nalpha alpha alpha alpha beta"]);
        index(&store, "b.txt", &["b.txt\nalpha beta"]);
        index(&store, "c.txt", &["c.txt\nbeta beta beta beta alpha"]);
        index(&store, "d.txt", &["d.txt\nalpha only here"]);
        let id = |name: &str| {
            store
                .file_id(std::path::Path::new(&format!("/corpus/{name}")))
                .unwrap()
        };

        let ranked = lexical_ranked(&store, &["alpha", "beta"], &Filter::default()).unwrap();
        assert_eq!(
            ranked
                .iter()
                .map(|(file_id, _)| *file_id)
                .collect::<Vec<_>>(),
            vec![id("b.txt"), id("a.txt"), id("c.txt")],
            "b is second on both terms; a is first on alpha and last on beta, \
             so alpha's lead cannot outvote beta's verdict"
        );
        assert!(
            ranked.iter().all(|(_, tile)| tile.is_some()),
            "every ranked file names the tile a snippet is built from"
        );
    }

    /// Terms under three characters cannot be trigrams: they narrow the result
    /// by scanning, and they never pretend the files they find are unfindable.
    #[test]
    fn lexical_ranked_scans_for_terms_too_short_to_index() {
        use crate::store::Store;

        let data = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        index(&store, "a.txt", &["a.txt\ninvoice from go travel"]);
        index(&store, "b.txt", &["b.txt\ninvoice only"]);
        index(&store, "c.txt", &["c.txt\nwe go now"]);
        let id = |name: &str| {
            store
                .file_id(std::path::Path::new(&format!("/corpus/{name}")))
                .unwrap()
        };

        let ranked = lexical_ranked(&store, &["invoice", "go"], &Filter::default()).unwrap();
        assert_eq!(
            ranked
                .iter()
                .map(|(file_id, _)| *file_id)
                .collect::<Vec<_>>(),
            vec![id("a.txt")],
            "b holds invoice but not go, c holds go but not invoice"
        );

        let ranked = lexical_ranked(&store, &["go"], &Filter::default()).unwrap();
        assert_eq!(
            ranked
                .iter()
                .map(|(file_id, _)| *file_id)
                .collect::<Vec<_>>(),
            vec![id("a.txt"), id("c.txt")],
            "a query of only short terms is still a query"
        );
    }

    /// Multi-term AND runs across the file's tiles, not within one tile — the
    /// contract `search` already had, kept by the ranked arm.
    #[test]
    fn lexical_ranked_matches_terms_across_tiles() {
        use crate::store::Store;

        let data = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        index(
            &store,
            "e.txt",
            &["e.txt\nalpha", "beta lives on another tile"],
        );
        index(&store, "f.txt", &["f.txt\nalpha on the same tile as beta"]);

        let ranked = lexical_ranked(&store, &["alpha", "beta"], &Filter::default()).unwrap();
        assert_eq!(ranked.len(), 2, "both files hold both terms, tiles or not");
    }
}
