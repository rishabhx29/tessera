//! Weighted Reciprocal Rank Fusion: the one search box over every arm.
//!
//! Three arms answer one query and their answers are merged here — lexical
//! (FTS5/BM25), image (SigLIP), text (MiniLM). They are merged by *rank
//! position*, never by score: BM25 and cosine are incomparable quantities
//! (ADR-0003). What the first cut of that decision got wrong was treating the
//! arms as equals. On files that carry text — screenshots, PDFs, DOCX — the
//! image arm's opinion is mostly noise, and ticket 07 measured what trusting it
//! costs: on 56 real screenshots, an equal voice dropped top-1 from 0.964 to
//! 0.375, a 0.10 voice to 0.857, while a 0.05 vote held it at 0.964
//! (docs/performance.md).
//!
//! So each arm votes with a weight derived from evidence the index already
//! has: how much text the file holds. A file full of text trusts lexical and
//! text arms; a photo with nothing to read trusts the image arm alone; a
//! chart with a few axis labels splits the difference. The weights are
//! constants with names ([`CHOSEN`]), not magic numbers at call sites, so the
//! measurement that chose them is one edit away from being re-run.
//!
//! The arithmetic in this module is pure: ranked lists in, fused order out.
//! The offline sweep in `tests/image_fusion.rs` re-derives its numbers through
//! [`fuse`], and the query path ([`search`]) runs the arms on threads and
//! hands their lists here, so the measured arithmetic is the shipped one.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};

use crate::store::{Filter, Store};
use crate::vectors::{QueryEmbedder, TextEmbedder};

/// One arm that can surface a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    /// FTS5 over tile text: the words the file says.
    Lexical,
    /// SigLIP image↔text: what the file looks like.
    Image,
    /// MiniLM tiles: what the file is about.
    Text,
}

impl Arm {
    /// The word a user sees in a result line. "meaning" is the term the CLI
    /// help already uses for the text arm; "keyword" is what plain search is.
    pub fn name(self) -> &'static str {
        match self {
            Arm::Lexical => "keyword",
            Arm::Image => "image",
            Arm::Text => "meaning",
        }
    }
}

/// The set of arms that voted (non-zero) for one file: every hit says which
/// arms surfaced it, because a result without that word is a claim the user
/// cannot check.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ArmSet(u8);

impl ArmSet {
    fn add(&mut self, arm: Arm) {
        self.0 |= 1 << arm as u8;
    }

    pub fn contains(self, arm: Arm) -> bool {
        self.0 & (1 << arm as u8) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Arm names in a fixed order — keyword, image, meaning — so two equal
    /// sets always print, and compare, the same way.
    pub fn names(self) -> Vec<&'static str> {
        [Arm::Lexical, Arm::Image, Arm::Text]
            .into_iter()
            .filter(|arm| self.contains(*arm))
            .map(Arm::name)
            .collect()
    }
}

/// How much text a file holds, as the index knows it. The bucketing is crude
/// on purpose: the weights it selects were measured at the extremes ("full of
/// text", "nothing to read"), and the middle class exists because a chart with
/// axis labels is neither a document nor a photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEvidence {
    /// No tiles at all: a photo of a dog.
    None,
    /// Some text, but not a document: a chart with axis labels. The shape the
    /// OCR-noise case also falls into, which is stated rather than hidden.
    Little,
    /// A text-bearing file: screenshot, PDF, DOCX, text.
    Full,
}

/// Above this many characters of tile text a file is treated as text-bearing.
/// Chosen to sit above OCR noise and a filename, below a real screenshot's
/// text; unlike the arm weights it has no measurement behind it yet.
pub const LITTLE_CHARS: usize = 120;

/// Bucket a file's summed tile-text length.
pub fn evidence_of(chars: u64) -> TextEvidence {
    match chars {
        0 => TextEvidence::None,
        n if n <= LITTLE_CHARS as u64 => TextEvidence::Little,
        _ => TextEvidence::Full,
    }
}

/// How much a file's text evidence matters: one vote weight per arm per
/// evidence class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmWeights {
    pub lexical: f32,
    pub image: f32,
    pub text: f32,
}

/// The three evidence classes' arm weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub full: ArmWeights,
    pub little: ArmWeights,
    pub none: ArmWeights,
}

/// The classic RRF dampening constant: votes decay with rank position, so a
/// file at rank 0 scores 1/60 and rank 60 scores 1/120 — first place is worth
/// more than second, but not twice as much.
pub const RRF_K: f32 = 60.0;

/// The weights fusion runs with.
///
/// `full.image = 0.05` is the measured correction on screenshots (0.964 top-1
/// at 0.05, where 0.10 had already fallen to 0.857 and an equal voice to 0.375
/// — docs/performance.md). The text arm is a text
/// signal like the lexical arm, built from the same tile text, so it takes the
/// lexical arm's weight in every class. The `little` row splits evenly because
/// nothing has measured it; the day something does, this constant is where the
/// answer goes.
pub const CHOSEN: Weights = Weights {
    full: ArmWeights {
        lexical: 1.0,
        image: 0.05,
        text: 1.0,
    },
    little: ArmWeights {
        lexical: 0.5,
        image: 0.5,
        text: 0.5,
    },
    none: ArmWeights {
        lexical: 0.0,
        image: 1.0,
        text: 0.0,
    },
};

impl Weights {
    /// The per-arm weights for one file's evidence class.
    pub fn for_evidence(&self, evidence: TextEvidence) -> ArmWeights {
        match evidence {
            TextEvidence::Full => self.full,
            TextEvidence::Little => self.little,
            TextEvidence::None => self.none,
        }
    }

    /// One arm's weight for one file.
    pub fn vote(&self, arm: Arm, evidence: TextEvidence) -> f32 {
        let weights = self.for_evidence(evidence);
        match arm {
            Arm::Lexical => weights.lexical,
            Arm::Image => weights.image,
            Arm::Text => weights.text,
        }
    }
}

/// The pictures lens answers by appearance alone. One arm, one weight in every
/// evidence class: how much text a picture carries must not reorder a visual
/// search, because the user did not ask about its text.
const PICTURES: Weights = Weights {
    full: ArmWeights {
        lexical: 0.0,
        image: 1.0,
        text: 0.0,
    },
    little: ArmWeights {
        lexical: 0.0,
        image: 1.0,
        text: 0.0,
    },
    none: ArmWeights {
        lexical: 0.0,
        image: 1.0,
        text: 0.0,
    },
};

fn weights_for(mode: SearchMode) -> &'static Weights {
    match mode {
        SearchMode::Pictures => &PICTURES,
        _ => &CHOSEN,
    }
}

/// The notes a missing model set earns under a lens. One wording, used by the
/// window and the CLI, and never a note about an arm the lens does not run.
pub fn missing_model_notes(
    mode: SearchMode,
    image_available: bool,
    text_available: bool,
) -> Vec<String> {
    let mut notes = Vec::new();
    if mode.runs_image() && !image_available {
        notes.push(if mode == SearchMode::Pictures {
            "picture search is off: the siglip models are not installed".to_string()
        } else {
            "the image arm is off: the siglip models are not installed".to_string()
        });
    }
    if mode.runs_meaning() && !text_available {
        notes.push(if mode == SearchMode::All {
            "the meaning arm is off: the minilm models are not installed".to_string()
        } else {
            "meaning search is off: the minilm models are not installed. keywords still work"
                .to_string()
        });
    }
    notes
}

/// A file as one arm returned it, in the arm's order. `tile_id` is the tile
/// the arm found it in, when the arm ranks positions at all; the image arm
/// ranks files and says `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranked {
    pub file_id: i64,
    pub tile_id: Option<i64>,
}

/// One fused result, in fused order.
#[derive(Debug, Clone, PartialEq)]
pub struct Fused {
    pub file_id: i64,
    /// The tile a snippet should be built from: the best-positioned arm's
    /// match. `None` for a file only the image arm surfaced.
    pub tile_id: Option<i64>,
    pub score: f32,
    pub arms: ArmSet,
}

/// Merge ranked lists into one order.
///
/// A file's score is the sum of its arms' votes, each `weight / (RRF_K + rank)`.
/// A file that an arm ranked but whose weight for that file is zero gets no
/// vote from it — the arm's list position is not evidence, so it does not
/// enter the score and the arm is not named on the hit.
pub fn fuse(
    arms: &[(Arm, Vec<Ranked>)],
    evidence: &HashMap<i64, TextEvidence>,
    weights: &Weights,
    limit: usize,
) -> Vec<Fused> {
    #[derive(Default)]
    struct Vote {
        score: f32,
        arms: ArmSet,
        tile: Option<i64>,
        tile_priority: u8,
    }

    let mut votes: HashMap<i64, Vote> = HashMap::new();
    for (arm, ranked) in arms {
        for (position, entry) in ranked.iter().enumerate() {
            let file_evidence = evidence
                .get(&entry.file_id)
                .copied()
                .unwrap_or(TextEvidence::None);
            let weight = weights.vote(*arm, file_evidence);
            if weight <= 0.0 {
                continue;
            }
            let vote = votes.entry(entry.file_id).or_default();
            vote.score += weight / (RRF_K + position as f32);
            vote.arms.add(*arm);
            if let Some(tile) = entry.tile_id {
                // The lexical arm's tile has the literal words; the text
                // arm's has the meaning. The snippet reads better around the
                // words, so lexical wins ties.
                let priority = match arm {
                    Arm::Lexical => 3,
                    Arm::Text => 2,
                    Arm::Image => 1,
                };
                if priority > vote.tile_priority {
                    vote.tile = Some(tile);
                    vote.tile_priority = priority;
                }
            }
        }
    }

    let mut fused: Vec<Fused> = votes
        .into_iter()
        .map(|(file_id, vote)| Fused {
            file_id,
            tile_id: vote.tile,
            score: vote.score,
            arms: vote.arms,
        })
        .collect();
    // Ties break by file id: two files the arms cannot separate must still
    // come back in the same order every run.
    fused.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.file_id.cmp(&b.file_id)));
    fused.truncate(limit);
    fused
}

/// One parsed query: the words to match, and the filters that narrow every
/// arm.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Query {
    pub terms: Vec<String>,
    pub filter: Filter,
}

/// Which question the user is asking. A lens selects the arms that run and the
/// kinds that may answer (ADR-0010).
///
/// `All` is the neutral question: every arm over every kind, with the existing
/// evidence-weighted fusion. The other three are the questions a picture
/// library has to separate: what a picture looks like, what words are inside
/// it, and what words are inside a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    #[default]
    All,
    Pictures,
    Words,
    Documents,
}

impl SearchMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "pictures" => Some(Self::Pictures),
            "words" => Some(Self::Words),
            "documents" => Some(Self::Documents),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Pictures => "pictures",
            Self::Words => "words",
            Self::Documents => "documents",
        }
    }

    /// The file kinds this lens may answer with. Empty means every kind.
    pub fn kinds(self) -> &'static [&'static str] {
        match self {
            Self::All => &[],
            Self::Pictures | Self::Words => &["image"],
            Self::Documents => &["pdf", "docx", "text"],
        }
    }

    pub fn runs_lexical(self) -> bool {
        !matches!(self, Self::Pictures)
    }

    pub fn runs_image(self) -> bool {
        matches!(self, Self::All | Self::Pictures)
    }

    pub fn runs_meaning(self) -> bool {
        matches!(self, Self::All | Self::Words | Self::Documents)
    }

    /// Whether only files with body text may answer. The filename line does
    /// not count as text inside the file.
    pub fn needs_visible_text(self) -> bool {
        matches!(self, Self::Words | Self::Documents)
    }

    /// The lens kinds in the user's filter vocabulary, for a conflict note.
    fn kinds_phrase(self) -> &'static str {
        match self {
            Self::All => "every file",
            Self::Pictures | Self::Words => "images",
            Self::Documents => "pdf, docx and text files",
        }
    }
}

impl Query {
    /// The words, joined: what the semantic arms embed. Filters are not
    /// content and deliberately absent.
    pub fn content(&self) -> String {
        self.terms.join(" ")
    }
}

/// Parse the search command's raw arguments into terms and filters.
///
/// `type:`, `in:`, `before:` and `after:` are the filter vocabulary
/// (docs/product-definition.md). A token whose prefix is not one of these is a
/// term and stays literal, so `rfc:9110` searches for what was typed.
///
/// An argument that *starts* with `in:` keeps its spaces, so
/// `tessera search "in:My Documents" invoice` filters by a folder with a space
/// in its name; any other argument is split into terms, which is how one
/// quoted `"invoice airline"` becomes the two terms the user meant.
pub fn parse_query(args: &[String]) -> Result<Query> {
    let mut query = Query::default();
    for arg in args {
        let mut tokens: Vec<&str> = Vec::new();
        if starts_with_filter_prefix(arg) {
            tokens.push(arg.as_str());
        } else {
            tokens.extend(arg.split_whitespace());
        }
        for token in tokens {
            add_token(&mut query, token)?;
        }
    }
    Ok(query)
}

fn starts_with_filter_prefix(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    ["type:", "in:", "before:", "after:"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn add_token(query: &mut Query, token: &str) -> Result<()> {
    let lower = token.to_ascii_lowercase();
    for (prefix, kind) in [
        ("type:", FilterToken::Type),
        ("in:", FilterToken::Folder),
        ("before:", FilterToken::Before),
        ("after:", FilterToken::After),
    ] {
        let Some(value) = lower.strip_prefix(prefix).map(|lower_value| {
            // The keyword's case is insensitive; the value keeps its case —
            // a folder name is not a type name.
            &token[prefix.len()..][..lower_value.len()]
        }) else {
            continue;
        };
        return apply_filter(query, kind, value);
    }
    if !token.is_empty() {
        query.terms.push(token.to_string());
    }
    Ok(())
}

enum FilterToken {
    Type,
    Folder,
    Before,
    After,
}

/// Map one `type:` value to the store's kind names. `doc` is the product
/// spec's word for a document; the store writes DOCX and plain text as two
/// kinds, and both are documents.
fn kinds_for(value: &str) -> Option<Vec<String>> {
    match value.to_ascii_lowercase().as_str() {
        "image" => Some(vec!["image".into()]),
        "pdf" => Some(vec!["pdf".into()]),
        "doc" => Some(vec!["docx".into(), "text".into()]),
        _ => None,
    }
}

fn apply_filter(query: &mut Query, kind: FilterToken, value: &str) -> Result<()> {
    match kind {
        FilterToken::Type => match kinds_for(value) {
            Some(kinds) => {
                for kind in kinds {
                    if !query.filter.kinds.contains(&kind) {
                        query.filter.kinds.push(kind);
                    }
                }
                Ok(())
            }
            None => bail!(
                "type:{value} is not a type this search has — try type:image, type:pdf or type:doc"
            ),
        },
        FilterToken::Folder => {
            if value.is_empty() {
                bail!("in: needs a folder name");
            }
            query.filter.in_folder = Some(value.to_string());
            Ok(())
        }
        FilterToken::Before => {
            query.filter.before_ns = Some(parse_date(value, "before:")?);
            Ok(())
        }
        FilterToken::After => {
            query.filter.after_ns = Some(parse_date(value, "after:")?);
            Ok(())
        }
    }
}

/// `YYYY-MM-DD`, the one date shape the filters accept, as epoch nanoseconds —
/// the unit `files.mtime` is stored in.
fn parse_date(value: &str, keyword: &str) -> Result<i64> {
    let bad =
        || anyhow::anyhow!("{keyword}{value} is not a date — the shape is {keyword}YYYY-MM-DD");
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return Err(bad());
    }
    let parsed = || -> Option<(i64, u32, u32)> {
        Some((
            parts[0].parse::<i64>().ok()?,
            parts[1].parse::<u32>().ok()?,
            parts[2].parse::<u32>().ok()?,
        ))
    };
    let Some((year, month, day)) = parsed() else {
        return Err(bad());
    };
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return Err(bad());
    }
    Ok(days_from_civil(year, month, day) * 86_400_000_000_000)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`): the calendar
/// arithmetic written out, so a date filter needs no dependency.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year as i64;
    era * 146_097 + day_of_era - 719_468
}

/// One hit as the user reads it: the file, where the match sits inside it, and
/// which arms found it.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedHit {
    pub file_id: i64,
    pub path: String,
    pub kind: String,
    /// The page the snippet is from, when the format has pages.
    pub page: Option<u32>,
    pub snippet: String,
    pub arms: ArmSet,
    /// The keyword arm voted, but no term appears in the body text: the file
    /// matched on its name. The window says "name matches" for this hit.
    pub name_only: bool,
    /// The picture's own size, for the wall's layout. `None` for a document
    /// and for a picture whose header has not been read.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The lines whose text contains a query term, and where they sit on the
    /// picture. Empty for a document and for a hit the image arm found alone.
    pub marks: Vec<Mark>,
}

/// A rectangle on a picture, in fractions of its width and height. The same
/// coordinates draw the mark in the wall and in the preview.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// The lines of `text` that contain a term, with their boxes.
///
/// The comparison is case-insensitive and literal, the same rule the keyword
/// arm uses. A tile whose line count and box count disagree is not trusted:
/// halves of two different reads would put a mark where no word is.
pub fn line_marks(
    text: &str,
    boxes: &[Option<crate::geometry::NormBox>],
    terms: &[&str],
) -> Vec<Mark> {
    if text.lines().count() != boxes.len() {
        return Vec::new();
    }
    let needles: Vec<String> = terms
        .iter()
        .filter(|term| !term.is_empty())
        .map(|term| term.to_lowercase())
        .collect();
    text.lines()
        .zip(boxes)
        .filter_map(|(line, bounds)| {
            let bounds = bounds.as_ref()?;
            let lower = line.to_lowercase();
            needles
                .iter()
                .any(|term| lower.contains(term))
                .then_some(Mark {
                    x0: bounds.x0,
                    y0: bounds.y0,
                    x1: bounds.x1,
                    y1: bounds.y1,
                })
        })
        .collect()
}

/// One arm's wall time, in milliseconds. Measured per search, never estimated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmTiming {
    pub arm: Arm,
    pub millis: f64,
}

/// What one search returned.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchOutcome {
    pub hits: Vec<FusedHit>,
    /// One entry per arm that ran, in lexical, image, text order.
    pub timings: Vec<ArmTiming>,
    /// Things the user should know that are not errors — an installed arm
    /// with nothing in the index to rank yet.
    pub notes: Vec<String>,
}

/// Milliseconds since the query started, printed when
/// `TESSERA_SEARCH_TRACE` is set — how a slow stage is found without guessing.
/// Every mark is guarded in one place so the cost when unset is one
/// environment lookup.
fn trace_mark(started: Instant, label: &str) {
    if std::env::var_os("TESSERA_SEARCH_TRACE").is_some() {
        eprintln!(
            "search trace: {label}: {:.1} ms since query start",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// Run the arms the lens asks for against the index in `data_dir` and fuse.
///
/// The arms run on threads, each opening its own [`Store`] inside its thread:
/// `rusqlite::Connection` is `Send` but not `Sync`, and the index is in WAL
/// mode, so the arms read without blocking each other. The caller's
/// connection does the evidence and display passes after the arms land — one
/// writer-free pass over the fused ids.
///
/// `mode` is the question the user is asking. It selects the arms that run,
/// narrows the kinds that may answer, and for `words` and `documents` keeps
/// name-only files out (ADR-0010). `image` and `text` are the embedders whose
/// model sets are installed; an arm whose embedder is absent does not run, and
/// the search is still a search. A call with no terms is a filter-only
/// listing: the files the filters name, newest first, no arms and no scores.
pub fn search(
    data_dir: &Path,
    query: &Query,
    mode: SearchMode,
    image: Option<&dyn QueryEmbedder>,
    text: Option<&dyn TextEmbedder>,
    limit: usize,
) -> Result<SearchOutcome> {
    let trace_started = Instant::now();
    let store = Store::open(data_dir).context("opening the index for search")?;
    trace_mark(trace_started, "store opened");

    let effective_filter = match lens_filter(query, mode) {
        Ok(filter) => filter,
        Err(note) => {
            return Ok(SearchOutcome {
                hits: Vec::new(),
                timings: Vec::new(),
                notes: vec![note],
            });
        }
    };

    if query.terms.is_empty() {
        let hits = if effective_filter.is_empty() {
            Vec::new()
        } else {
            filter_only(&store, &effective_filter, mode, limit)?
        };
        return Ok(SearchOutcome {
            hits,
            timings: Vec::new(),
            notes: Vec::new(),
        });
    }

    let mut notes = Vec::new();
    if mode.runs_image() && image.is_some() && !store.has_image_vectors()? {
        notes.push(if mode == SearchMode::Pictures {
            "no picture has a description yet — run `tessera index` again to embed them".to_string()
        } else {
            "the image arm has no vectors yet — run `tessera index` to embed the files \
             already indexed"
                .to_string()
        });
    }
    if mode.runs_meaning() && text.is_some() && !store.has_text_vectors()? {
        notes.push(
            "the meaning arm has no vectors yet — run `tessera index` to embed the files \
             already indexed"
                .to_string(),
        );
    }
    trace_mark(trace_started, "arm pin counts checked");

    let terms: Vec<&str> = query.terms.iter().map(String::as_str).collect();
    let content = query.content();
    let mut allowed: Option<HashSet<i64>> = if effective_filter.is_empty() {
        None
    } else {
        Some(
            store
                .filtered_file_ids(&effective_filter)?
                .into_iter()
                .collect(),
        )
    };
    if mode.needs_visible_text() {
        let body = store.file_ids_with_body_text()?;
        allowed = Some(match allowed {
            Some(allowed) => allowed.intersection(&body).copied().collect(),
            None => body,
        });
    }

    let mut arms: Vec<(Arm, Vec<Ranked>)> = Vec::new();
    let mut timings: Vec<ArmTiming> = Vec::new();

    std::thread::scope(|scope| -> Result<()> {
        let lexical = if mode.runs_lexical() {
            Some(scope.spawn(|| -> Result<(Vec<Ranked>, f64)> {
                let store = Store::open(data_dir)?;
                let started = Instant::now();
                let ranked = crate::search::lexical_ranked(&store, &terms, &effective_filter)?;
                let ranked = ranked
                    .into_iter()
                    .map(|(file_id, tile_id)| Ranked { file_id, tile_id })
                    .collect();
                Ok((ranked, started.elapsed().as_secs_f64() * 1000.0))
            }))
        } else {
            None
        };
        let image_arm = if mode.runs_image() {
            image.map(|embedder| {
                scope.spawn(|| -> Result<(Vec<Ranked>, f64)> {
                    let store = Store::open(data_dir)?;
                    let started = Instant::now();
                    let vector = embedder.embed_query(&content)?;
                    let ranked = store
                        .ranked_image_file_ids(&vector, embedder.dim())?
                        .into_iter()
                        .map(|file_id| Ranked {
                            file_id,
                            tile_id: None,
                        })
                        .collect();
                    Ok((ranked, started.elapsed().as_secs_f64() * 1000.0))
                })
            })
        } else {
            None
        };
        let text_arm = if mode.runs_meaning() {
            text.map(|embedder| {
                scope.spawn(|| -> Result<(Vec<Ranked>, f64)> {
                    let store = Store::open(data_dir)?;
                    let started = Instant::now();
                    let vector = embedder.embed_text(&content)?;
                    let ranked = store
                        .ranked_text_file_tiles(&vector, crate::store::TEXT_EMBEDDING_DIM)?
                        .into_iter()
                        .map(|(file_id, tile_id)| Ranked {
                            file_id,
                            tile_id: Some(tile_id),
                        })
                        .collect();
                    Ok((ranked, started.elapsed().as_secs_f64() * 1000.0))
                })
            })
        } else {
            None
        };

        if let Some(handle) = lexical {
            let (ranked, millis) = handle
                .join()
                .map_err(|_| anyhow!("the lexical arm panicked"))??;
            timings.push(ArmTiming {
                arm: Arm::Lexical,
                millis,
            });
            arms.push((Arm::Lexical, ranked));
        }

        if let Some(handle) = image_arm {
            let (ranked, millis) = handle
                .join()
                .map_err(|_| anyhow!("the image arm panicked"))??;
            timings.push(ArmTiming {
                arm: Arm::Image,
                millis,
            });
            arms.push((Arm::Image, ranked));
        }

        if let Some(handle) = text_arm {
            let (ranked, millis) = handle
                .join()
                .map_err(|_| anyhow!("the text arm panicked"))??;
            timings.push(ArmTiming {
                arm: Arm::Text,
                millis,
            });
            arms.push((Arm::Text, ranked));
        }

        Ok(())
    })?;
    trace_mark(trace_started, "arms joined");

    // A filter honoured by one arm and not another would let the other arm
    // smuggle back exactly the files the user excluded, so the allowed set
    // narrows every list, whichever arm produced it.
    if let Some(allowed) = &allowed {
        for (_, ranked) in &mut arms {
            ranked.retain(|entry| allowed.contains(&entry.file_id));
        }
    }

    // Evidence for exactly the files in contention, read once. A file absent
    // from the map has no tiles, which is the `None` class.
    let mut candidates: HashSet<i64> = HashSet::new();
    for (_, ranked) in &arms {
        candidates.extend(ranked.iter().map(|entry| entry.file_id));
    }
    let candidates: Vec<i64> = candidates.into_iter().collect();
    let evidence: HashMap<i64, TextEvidence> = store
        .tile_text_lengths(&candidates)?
        .into_iter()
        .map(|(file_id, chars)| (file_id, evidence_of(chars)))
        .collect();
    trace_mark(trace_started, "candidates and evidence collected");

    let fused = fuse(&arms, &evidence, weights_for(mode), limit);
    trace_mark(trace_started, "fused");

    let ids: Vec<i64> = fused.iter().map(|hit| hit.file_id).collect();
    let displays = store.file_displays(&ids)?;
    let tiles: Vec<i64> = fused.iter().filter_map(|hit| hit.tile_id).collect();
    let chosen_tiles = store.tile_rows(&tiles)?;
    trace_mark(trace_started, "display rows fetched");

    let mut hits = Vec::with_capacity(fused.len());
    for hit in fused {
        let Some(display) = displays.get(&hit.file_id) else {
            continue;
        };
        // The chosen tile holds what the arm matched — the lexical arm's words,
        // the text arm's meaning. The file's first tile is the fallback for a
        // hit whose arm ranked no position (the image arm) or whose tile has
        // since gone.
        let chosen = hit.tile_id.and_then(|tile_id| {
            chosen_tiles
                .get(&tile_id)
                .map(|row| (row.page, row.text.as_str()))
        });
        let first = display
            .first_tile
            .as_ref()
            .map(|(_, page, text)| (*page, text.as_str()));
        let (page, text) = chosen.or(first).unwrap_or((None, ""));
        let snippet = if text.is_empty() {
            String::new()
        } else if hit.arms.contains(Arm::Lexical) {
            crate::search::snippet_around(text, &terms, 240)
        } else {
            // No literal words to aim at: a head-of-text preview instead.
            crate::search::snippet_around(text, &[], 160)
        };
        // A file matches by name when the keyword arm voted but the word is
        // only on the filename line. A match on any tile after the first is
        // body text by construction: the name is only prepended to tile one.
        let name_only = hit.arms.contains(Arm::Lexical)
            && hit.tile_id.is_some_and(|tile_id| {
                display
                    .first_tile
                    .as_ref()
                    .is_some_and(|(first_id, _, first_text)| {
                        tile_id == *first_id && !body_contains_any(first_text, &terms)
                    })
            });
        let marks = if display.kind == "image" && hit.arms.contains(Arm::Lexical) {
            hit.tile_id
                .and_then(|tile_id| chosen_tiles.get(&tile_id))
                .map(|tile| line_marks(&tile.text, &tile.boxes, &terms))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        hits.push(FusedHit {
            file_id: hit.file_id,
            path: display.path.clone(),
            kind: display.kind.clone(),
            page,
            snippet,
            arms: hit.arms,
            name_only,
            width: display.width,
            height: display.height,
            marks,
        });
    }
    trace_mark(trace_started, "hits built");

    Ok(SearchOutcome {
        hits,
        timings,
        notes,
    })
}

/// The query's filter narrowed by the lens, or the note that explains why the
/// two cannot both hold. A filter may narrow a lens; it may never widen it.
fn lens_filter(query: &Query, mode: SearchMode) -> std::result::Result<Filter, String> {
    let mut filter = query.filter.clone();
    let kinds = mode.kinds();
    if kinds.is_empty() {
        return Ok(filter);
    }
    if filter.kinds.is_empty() {
        filter.kinds = kinds.iter().map(|kind| (*kind).to_string()).collect();
        return Ok(filter);
    }
    let narrowed: Vec<String> = filter
        .kinds
        .iter()
        .filter(|kind| kinds.contains(&kind.as_str()))
        .cloned()
        .collect();
    if narrowed.is_empty() {
        return Err(format!(
            "the {} lens searches {} only, so {} cannot narrow it",
            mode.name(),
            mode.kinds_phrase(),
            kind_filter_label(&filter.kinds),
        ));
    }
    filter.kinds = narrowed;
    Ok(filter)
}

/// The user's `type:` filter spelled back as they typed it, for the conflict
/// note. `docx` and `text` are one word to the user.
fn kind_filter_label(kinds: &[String]) -> String {
    let mut sorted: Vec<&str> = kinds.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    match sorted.as_slice() {
        ["image"] => "type:image".into(),
        ["pdf"] => "type:pdf".into(),
        ["docx", "text"] => "type:doc".into(),
        _ => sorted
            .iter()
            .map(|kind| format!("type:{kind}"))
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

/// Whether any term appears in the body lines of a tile. The first line is the
/// filename [`crate::index::with_filename`] prepended, so it never counts.
fn body_contains_any(text: &str, terms: &[&str]) -> bool {
    let body = text.split_once('\n').map(|(_, body)| body).unwrap_or("");
    let lower = body.to_lowercase();
    terms
        .iter()
        .any(|term| lower.contains(&term.to_lowercase()))
}

/// A query with no terms: the files the filters name, newest first, each
/// rendered from its first tile. No arm runs, so no hit wears a label.
///
/// The words and documents lenses keep name-only files out here too: a
/// listing is still a search for words inside a file.
fn filter_only(
    store: &Store,
    filter: &Filter,
    mode: SearchMode,
    limit: usize,
) -> Result<Vec<FusedHit>> {
    let mut ids = store.filtered_file_ids(filter)?;
    if mode.needs_visible_text() {
        let body = store.file_ids_with_body_text()?;
        ids.retain(|file_id| body.contains(file_id));
    }
    let ids: Vec<i64> = ids.into_iter().take(limit).collect();
    let displays = store.file_displays(&ids)?;
    let mut hits = Vec::with_capacity(ids.len());
    for file_id in ids {
        let Some(display) = displays.get(&file_id) else {
            continue;
        };
        let (page, text) = display
            .first_tile
            .as_ref()
            .map(|(_, page, text)| (*page, text.as_str()))
            .unwrap_or((None, ""));
        hits.push(FusedHit {
            file_id,
            path: display.path.clone(),
            kind: display.kind.clone(),
            page,
            snippet: if text.is_empty() {
                String::new()
            } else {
                crate::search::snippet_around(text, &[], 160)
            },
            arms: ArmSet::default(),
            name_only: false,
            width: display.width,
            height: display.height,
            marks: Vec::new(),
        });
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked(ids: &[i64]) -> Vec<Ranked> {
        ids.iter()
            .map(|file_id| Ranked {
                file_id: *file_id,
                tile_id: None,
            })
            .collect()
    }

    fn evidence_of_all(ids: &[i64], evidence: TextEvidence) -> HashMap<i64, TextEvidence> {
        ids.iter().map(|id| (*id, evidence)).collect()
    }

    /// The measured correction, pinned: when the image arm disagrees with the
    /// lexical arm on a text-bearing file, a 0.05 vote keeps top-1 where an
    /// equal vote flips it. Equal weights are what ADR-0003 shipped and what
    /// ticket 07 measured to cost 10.7 points of top-1 on real screenshots;
    /// if this test ever fails on the `CHOSEN` half, a regression has been
    /// re-introduced into the weights.
    #[test]
    fn the_image_arms_small_vote_holds_top_1_where_an_equal_vote_flips_it() {
        let files = [1_i64, 2, 3, 4];
        let evidence = evidence_of_all(&files, TextEvidence::Full);
        // The lexical arm's verdict: 1, 2, 3, 4. The image arm's opinion:
        // 3, 2, 4, then 1 — a disagreement big enough to matter.
        let arms = vec![
            (Arm::Lexical, ranked(&[1, 2, 3, 4])),
            (Arm::Image, ranked(&[3, 2, 4, 1])),
        ];

        let fused = fuse(&arms, &evidence, &CHOSEN, 10);
        assert_eq!(fused[0].file_id, 1, "the lexical winner stays the winner");

        let equal = Weights {
            full: ArmWeights {
                lexical: 1.0,
                image: 1.0,
                text: 1.0,
            },
            ..CHOSEN
        };
        let fused = fuse(&arms, &evidence, &equal, 10);
        assert_ne!(
            fused[0].file_id, 1,
            "equal weight hands top-1 to the image arm — the regression this pin exists for"
        );
    }

    /// A file with nothing to read leans on the image arm alone: the lexical
    /// arm's list position for it is not evidence, so it neither scores nor
    /// gets named on the hit.
    #[test]
    fn a_tileless_file_is_scored_by_the_image_arm_only() {
        let arms = vec![(Arm::Lexical, ranked(&[7, 9])), (Arm::Image, ranked(&[7]))];
        let evidence = HashMap::from([(7_i64, TextEvidence::None), (9_i64, TextEvidence::Full)]);
        let fused = fuse(&arms, &evidence, &CHOSEN, 10);

        let photo = fused.iter().find(|f| f.file_id == 7).unwrap();
        assert_eq!(photo.score, 1.0 / RRF_K, "the lexical vote is weighted 0");
        assert_eq!(
            photo.arms.names(),
            vec!["image"],
            "an arm that cast no vote is not named on the hit"
        );
        assert_eq!(photo.tile_id, None, "the image arm ranks files, not tiles");
    }

    /// Every hit names its arms; a file two arms agree on carries both names,
    /// in a fixed order.
    #[test]
    fn a_hit_names_every_arm_that_voted_for_it() {
        let arms = vec![
            (Arm::Lexical, ranked(&[1, 2])),
            (Arm::Text, ranked(&[2, 3])),
            (Arm::Image, ranked(&[3])),
        ];
        let evidence = evidence_of_all(&[1, 2, 3], TextEvidence::Full);
        let fused = fuse(&arms, &evidence, &CHOSEN, 10);

        let of = |id: i64| fused.iter().find(|f| f.file_id == id).unwrap();
        assert_eq!(of(1).arms.names(), vec!["keyword"]);
        assert_eq!(of(2).arms.names(), vec!["keyword", "meaning"]);
        assert_eq!(of(3).arms.names(), vec!["image", "meaning"]);
    }

    /// A snippet tile prefers the lexical arm's match over the text arm's:
    /// the words are what the snippet is built around.
    #[test]
    fn the_snippet_tile_prefers_the_lexical_arm() {
        let arms = vec![
            (
                Arm::Text,
                vec![Ranked {
                    file_id: 1,
                    tile_id: Some(20),
                }],
            ),
            (
                Arm::Lexical,
                vec![Ranked {
                    file_id: 1,
                    tile_id: Some(10),
                }],
            ),
        ];
        let evidence = evidence_of_all(&[1], TextEvidence::Full);
        let fused = fuse(&arms, &evidence, &CHOSEN, 10);
        assert_eq!(fused[0].tile_id, Some(10));
    }

    #[test]
    fn evidence_buckets_are_inclusive_of_the_little_ceiling() {
        assert_eq!(evidence_of(0), TextEvidence::None);
        assert_eq!(evidence_of(1), TextEvidence::Little);
        assert_eq!(evidence_of(LITTLE_CHARS as u64), TextEvidence::Little);
        assert_eq!(evidence_of(LITTLE_CHARS as u64 + 1), TextEvidence::Full);
    }

    #[test]
    fn filters_parse_out_of_the_query_and_stay_case_insensitive() {
        let query = parse_query(&[
            "invoice".to_string(),
            "airline".to_string(),
            "TYPE:PDF".to_string(),
            "in:Invoices".to_string(),
            "before:2024-01-02".to_string(),
            "after:2023-12-31".to_string(),
        ])
        .unwrap();
        assert_eq!(query.terms, vec!["invoice", "airline"]);
        assert_eq!(query.content(), "invoice airline");
        assert_eq!(query.filter.kinds, vec!["pdf"]);
        assert_eq!(query.filter.in_folder.as_deref(), Some("Invoices"));
        assert_eq!(
            query.filter.before_ns,
            Some(1_704_153_600_000_000_000),
            "2024-01-02 00:00 UTC in epoch nanoseconds"
        );
        assert_eq!(
            query.filter.after_ns,
            Some(1_703_980_800_000_000_000),
            "2023-12-31 00:00 UTC in epoch nanoseconds"
        );
    }

    #[test]
    fn a_folder_filter_keeps_the_spaces_it_was_quoted_with() {
        let query = parse_query(&["in:My Documents".to_string(), "invoice".to_string()]).unwrap();
        assert_eq!(query.filter.in_folder.as_deref(), Some("My Documents"));
        assert_eq!(query.terms, vec!["invoice"]);

        // But a quoted phrase with the filter in the middle is split into
        // tokens like any other argument, and each token stands alone.
        let query = parse_query(&["invoice type:pdf".to_string()]).unwrap();
        assert_eq!(query.terms, vec!["invoice"]);
        assert_eq!(query.filter.kinds, vec!["pdf"]);
    }

    #[test]
    fn doc_is_the_specs_word_for_both_document_kinds() {
        let query = parse_query(&["type:doc".to_string()]).unwrap();
        assert_eq!(query.filter.kinds, vec!["docx", "text"]);
        // Several filters union; duplicates collapse.
        let query = parse_query(&[
            "type:doc".to_string(),
            "type:doc".to_string(),
            "type:pdf".to_string(),
        ])
        .unwrap();
        assert_eq!(query.filter.kinds, vec!["docx", "text", "pdf"]);
    }

    #[test]
    fn filter_shaped_tokens_that_are_not_filters_are_terms() {
        let query = parse_query(&["rfc:9110".to_string(), "http://x".to_string()]).unwrap();
        assert_eq!(query.terms, vec!["rfc:9110", "http://x"]);
    }

    #[test]
    fn a_bad_filter_value_is_a_loud_error() {
        assert!(parse_query(&["type:video".to_string()]).is_err());
        assert!(parse_query(&["before:soon".to_string()]).is_err());
        assert!(parse_query(&["before:2024-13-01".to_string()]).is_err());
        assert!(parse_query(&["before:2023-02-29".to_string()]).is_err());
        assert!(parse_query(&["in:".to_string()]).is_err());
    }

    #[test]
    fn dates_convert_to_the_epoch_and_leap_years_count() {
        assert_eq!(parse_date("1970-01-01", "before:").unwrap(), 0);
        assert_eq!(
            parse_date("2024-02-29", "before:").unwrap(),
            days_from_civil(2024, 2, 29) * 86_400_000_000_000
        );
        assert_eq!(
            parse_date("2000-01-01", "after:").unwrap(),
            946_684_800_000_000_000
        );
        // 2023 was not a leap year; the day check is the one that catches it.
        assert!(parse_date("2023-02-29", "after:").is_err());
    }

    // ---- the query path: orchestration over a real store ----

    use crate::index::{Kind, Tile};
    use crate::vectors::Embedding;

    fn facts(mtime: i64) -> crate::index::skip::FileFacts {
        crate::index::skip::FileFacts {
            size: 10,
            mtime,
            placeholder: false,
        }
    }

    fn index_at(
        store: &Store,
        root: &Path,
        name: &str,
        kind: Kind,
        mtime: i64,
        tiles: &[(&str, Option<u32>)],
    ) -> std::path::PathBuf {
        let path = root.join(name);
        let tiles: Vec<Tile> = tiles
            .iter()
            .enumerate()
            .map(|(seq, (text, page))| Tile {
                seq: seq as u32,
                text: (*text).to_string(),
                page: *page,
                boxes: Vec::new(),
            })
            .collect();
        store
            .index_file(root, &path, kind, &facts(mtime), Some("h"), &tiles)
            .unwrap();
        path
    }

    fn index(
        store: &Store,
        root: &Path,
        name: &str,
        kind: Kind,
        tiles: &[(&str, Option<u32>)],
    ) -> std::path::PathBuf {
        index_at(store, root, name, kind, 1, tiles)
    }

    /// A one-bit meaning space: does the text say *apple*? Enough to give the
    /// text arm an opinion no other arm has, without an ONNX session.
    struct FakeText;

    impl TextEmbedder for FakeText {
        fn version(&self) -> &str {
            "fake-text"
        }

        fn embed_text(&self, text: &str) -> Result<Embedding> {
            let mut values = vec![0.0; crate::store::TEXT_EMBEDDING_DIM];
            if text.to_ascii_lowercase().contains("apple") {
                values[0] = 1.0;
            } else {
                values[1] = 1.0;
            }
            Ok(Embedding::new(values, crate::store::TEXT_EMBEDDING_DIM)?.normalized())
        }
    }

    /// The image arm's query side without ONNX: one fixed vector, four wide.
    struct FakeImage;

    impl QueryEmbedder for FakeImage {
        fn dim(&self) -> usize {
            4
        }

        fn embed_query(&self, _text: &str) -> Result<Embedding> {
            Ok(Embedding::new(vec![1.0, 0.0, 0.0, 0.0], 4)?.normalized())
        }
    }

    /// Three arms, one query: the keyword and meaning arms find the PDF, the
    /// image arm finds the photo, and the fusion orders them by the votes they
    /// honestly earned.
    #[test]
    fn a_three_arm_search_merges_the_arms_and_names_them() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();

        // Long enough to be full-evidence text (> LITTLE_CHARS), not just OCR noise.
        let long = format!("doc.pdf\napple pie recipe {}", "and more words ".repeat(12));
        index(
            &store,
            corpus.path(),
            "doc.pdf",
            Kind::Pdf,
            &[(&long, Some(2))],
        );
        let photo = index(&store, corpus.path(), "photo.png", Kind::Image, &[]);
        index(
            &store,
            corpus.path(),
            "note.txt",
            Kind::Text,
            &[("note.txt\nfruit market", None)],
        );

        // The vectors the arms will rank: tile ids come from the pending list.
        let pending = store.tiles_without_text_vectors().unwrap();
        let doc_tile = pending
            .iter()
            .find(|(_, text)| text.contains("apple"))
            .unwrap()
            .0;
        let note_tile = pending
            .iter()
            .find(|(_, text)| text.contains("fruit"))
            .unwrap()
            .0;
        store
            .put_text_embedding(doc_tile, &FakeText.embed_text(&long).unwrap())
            .unwrap();
        store
            .put_text_embedding(note_tile, &FakeText.embed_text("fruit market").unwrap())
            .unwrap();
        let photo_id = store.file_id(&photo).unwrap();
        store
            .put_image_embedding(
                photo_id,
                &Embedding::new(vec![1.0, 0.0, 0.0, 0.0], 4).unwrap(),
            )
            .unwrap();

        let query = parse_query(&["apple".to_string()]).unwrap();
        let outcome = search(
            data.path(),
            &query,
            SearchMode::All,
            Some(&FakeImage),
            Some(&FakeText),
            10,
        )
        .unwrap();

        assert!(outcome.notes.is_empty(), "{:?}", outcome.notes);
        assert_eq!(
            outcome.timings.len(),
            3,
            "every arm that ran reports its time"
        );
        let of = |name: &str| {
            outcome
                .hits
                .iter()
                .find(|hit| hit.path.ends_with(name))
                .unwrap()
        };
        let doc = of("doc.pdf");
        let photo = of("photo.png");
        assert_eq!(doc.arms.names(), vec!["keyword", "meaning"]);
        assert_eq!(
            doc.page,
            Some(2),
            "the snippet comes from the tile that matched"
        );
        assert!(doc.snippet.contains("apple"));
        assert_eq!(photo.arms.names(), vec!["image"]);
        assert_eq!(
            photo.snippet, "",
            "a photo with no text has nothing to show"
        );
        assert_eq!(outcome.hits[0].file_id, doc.file_id, "two votes beat one");
        assert_eq!(outcome.hits[1].file_id, photo.file_id);
        // The note is the meaning arm's rank 1 — a real answer the arm did
        // order — so it comes back last on the small vote that rank earned.
        assert_eq!(outcome.hits[2].path, of("note.txt").path);
    }

    /// An installed arm whose index holds no vectors yet found nothing to
    /// vote with; that is a note, not a silent hole.
    #[test]
    fn an_installed_arm_with_nothing_to_rank_says_so() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        index(
            &store,
            corpus.path(),
            "note.txt",
            Kind::Text,
            &[("note.txt\napple fruit", None)],
        );

        let query = parse_query(&["apple".to_string()]).unwrap();
        let outcome = search(
            data.path(),
            &query,
            SearchMode::All,
            Some(&FakeImage),
            Some(&FakeText),
            10,
        )
        .unwrap();

        let notes = outcome.notes.join("\n");
        assert!(notes.contains("image arm"), "{notes}");
        assert!(notes.contains("meaning arm"), "{notes}");
        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.hits[0].arms.names(), vec!["keyword"]);
    }

    /// A query with no terms and only filters is a listing: the files the
    /// filters name, newest first, wearing no arm label because no arm ran.
    #[test]
    fn a_filter_only_query_lists_the_files_it_names_newest_first() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        index_at(
            &store,
            corpus.path(),
            "old.pdf",
            Kind::Pdf,
            1_000,
            &[("old.pdf\na first page", Some(1))],
        );
        index_at(
            &store,
            corpus.path(),
            "new.pdf",
            Kind::Pdf,
            3_000,
            &[("new.pdf\nthe newer one", Some(1))],
        );
        index(
            &store,
            corpus.path(),
            "note.txt",
            Kind::Text,
            &[("note.txt\nnot a pdf", None)],
        );

        let query = parse_query(&["type:pdf".to_string()]).unwrap();
        let outcome = search(data.path(), &query, SearchMode::All, None, None, 10).unwrap();

        assert!(outcome.timings.is_empty(), "no arm ran");
        assert_eq!(outcome.hits.len(), 2);
        assert!(outcome.hits.iter().all(|hit| hit.arms.is_empty()));
        assert!(outcome.hits[0].path.ends_with("new.pdf"), "newest first");
        assert!(outcome.hits[1].path.ends_with("old.pdf"));
        assert!(
            outcome.hits[0].snippet.contains("newer"),
            "the first tile is the preview"
        );
    }

    /// The filter narrows every arm: this file is ranked by all three, and the
    /// two semantic arms must not smuggle it back past `type:pdf`.
    #[test]
    fn a_filter_narrows_every_arm_not_just_the_lexical_one() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();

        let long = format!("pdf\napple pie {}", "filler ".repeat(30));
        index(
            &store,
            corpus.path(),
            "doc.pdf",
            Kind::Pdf,
            &[(&long, None)],
        );
        let txt = index(
            &store,
            corpus.path(),
            "note.txt",
            Kind::Text,
            &[("note.txt\napple fruit", None)],
        );
        for (tile_id, text) in store.tiles_without_text_vectors().unwrap() {
            store
                .put_text_embedding(tile_id, &FakeText.embed_text(&text).unwrap())
                .unwrap();
        }
        let txt_id = store.file_id(&txt).unwrap();
        store
            .put_image_embedding(
                txt_id,
                &Embedding::new(vec![1.0, 0.0, 0.0, 0.0], 4).unwrap(),
            )
            .unwrap();

        let arms = |query: &Query| {
            search(
                data.path(),
                query,
                SearchMode::All,
                Some(&FakeImage),
                Some(&FakeText),
                10,
            )
            .unwrap()
            .hits
        };
        let unfiltered = arms(&parse_query(&["apple".to_string()]).unwrap());
        assert!(
            unfiltered.iter().any(|hit| hit.path.ends_with("note.txt")),
            "without the filter the txt is a three-arm answer"
        );

        let filtered = arms(&parse_query(&["apple".to_string(), "type:pdf".to_string()]).unwrap());
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].path.ends_with("doc.pdf"));
        assert_eq!(filtered[0].arms.names(), vec!["keyword", "meaning"]);
    }

    // ---- the lenses: which arms run and which kinds answer ----

    fn image_vector(store: &Store, path: &Path, values: [f32; 4]) {
        let file_id = store.file_id(path).unwrap();
        store
            .put_image_embedding(file_id, &Embedding::new(values.to_vec(), 4).unwrap())
            .unwrap();
    }

    #[test]
    fn marks_follow_matching_lines_without_marking_the_filename() {
        let bounds = crate::geometry::NormBox::pixels(10.0, 20.0, 60.0, 30.0, 100, 100);
        let text = "invoice.png\nINVOICE total\nunrelated line";
        let boxes = [None, Some(bounds), Some(bounds)];
        let marks = line_marks(text, &boxes, &["invoice", "total"]);
        assert_eq!(marks.len(), 1);
        assert_eq!(
            (marks[0].x0, marks[0].y0, marks[0].x1, marks[0].y1),
            (0.1, 0.2, 0.6, 0.3)
        );
        assert!(line_marks(text, &boxes, &["png"]).is_empty());
        assert!(line_marks(text, &boxes, &[""]).is_empty());
        assert!(line_marks(text, &boxes[..2], &["invoice"]).is_empty());
    }

    #[test]
    fn picture_hits_carry_size_and_only_literal_votes_carry_marks() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        let path = index(
            &store,
            corpus.path(),
            "shot.png",
            Kind::Image,
            &[("shot.png\nInvoice total\nunrelated", None)],
        );
        let file_id = store.file_id(&path).unwrap();
        store.set_dimensions(file_id, 800, 400).unwrap();
        let displays = store.file_displays(&[file_id]).unwrap();
        let tile_id = displays[&file_id].first_tile.as_ref().unwrap().0;
        let bounds = crate::geometry::NormBox::pixels(80.0, 80.0, 480.0, 120.0, 800, 400);
        store
            .set_tile_boxes(tile_id, &[None, Some(bounds), Some(bounds)])
            .unwrap();
        image_vector(&store, &path, [1.0, 0.0, 0.0, 0.0]);
        let query = parse_query(&["invoice".into()]).unwrap();

        let words = search(data.path(), &query, SearchMode::Words, None, None, 10).unwrap();
        assert_eq!(words.hits.len(), 1);
        let hit = &words.hits[0];
        assert_eq!((hit.width, hit.height), (Some(800), Some(400)));
        assert_eq!(hit.marks.len(), 1);
        assert_eq!(hit.marks[0].y0, 0.2);

        let pictures = search(
            data.path(),
            &query,
            SearchMode::Pictures,
            Some(&FakeImage),
            None,
            10,
        )
        .unwrap();
        assert_eq!(pictures.hits.len(), 1);
        assert!(pictures.hits[0].marks.is_empty());
        assert_eq!(
            (pictures.hits[0].width, pictures.hits[0].height),
            (Some(800), Some(400))
        );

        let filter = parse_query(&["type:image".into()]).unwrap();
        let listing = search(data.path(), &filter, SearchMode::All, None, None, 10).unwrap();
        assert!(listing.hits[0].marks.is_empty());
        assert_eq!(
            (listing.hits[0].width, listing.hits[0].height),
            (Some(800), Some(400))
        );
    }

    #[test]
    fn an_unknown_mode_is_not_a_mode() {
        assert_eq!(SearchMode::parse("all"), Some(SearchMode::All));
        assert_eq!(SearchMode::parse("pictures"), Some(SearchMode::Pictures));
        assert_eq!(SearchMode::parse("words"), Some(SearchMode::Words));
        assert_eq!(SearchMode::parse("documents"), Some(SearchMode::Documents));
        assert_eq!(SearchMode::parse("videos"), None);
        assert_eq!(
            SearchMode::parse("Pictures"),
            None,
            "the command vocabulary is lowercase, like the other filters"
        );
    }

    /// `all` is still the one search box: every arm, every kind, and the
    /// measured evidence weights rather than the pictures-only table.
    #[test]
    fn all_lens_keeps_the_current_fused_search() {
        assert!(SearchMode::All.runs_lexical());
        assert!(SearchMode::All.runs_image());
        assert!(SearchMode::All.runs_meaning());
        assert!(!SearchMode::All.needs_visible_text());
        assert!(
            SearchMode::All.kinds().is_empty(),
            "all is not narrowed to a kind"
        );
        assert_eq!(weights_for(SearchMode::All), &CHOSEN);
    }

    /// The pictures lens asks the image arm and nothing else, even when the
    /// other embedders are installed.
    #[test]
    fn pictures_lens_runs_only_the_image_arm() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();

        let a = index(
            &store,
            corpus.path(),
            "a.png",
            Kind::Image,
            &[("a.png\napple chart", None)],
        );
        let b = index(&store, corpus.path(), "b.png", Kind::Image, &[]);
        image_vector(&store, &a, [0.0, 1.0, 0.0, 0.0]);
        image_vector(&store, &b, [1.0, 0.0, 0.0, 0.0]);

        let query = parse_query(&["apple".to_string()]).unwrap();
        let outcome = search(
            data.path(),
            &query,
            SearchMode::Pictures,
            Some(&FakeImage),
            Some(&FakeText),
            10,
        )
        .unwrap();

        assert_eq!(
            outcome
                .timings
                .iter()
                .map(|timing| timing.arm.name())
                .collect::<Vec<_>>(),
            vec!["image"],
            "the keyword and meaning arms are off in this lens"
        );
        assert_eq!(outcome.hits.len(), 2);
        assert!(outcome.hits.iter().all(|hit| hit.kind == "image"));
        assert!(outcome
            .hits
            .iter()
            .all(|hit| hit.arms.names() == vec!["image"]));
        assert!(
            outcome.hits[0].path.ends_with("b.png"),
            "the image arm's cosine order is the order"
        );
    }

    /// A text-bearing picture does not outrank a better visual match in the
    /// pictures lens. The same pair under `all` is exactly what the evidence
    /// weights are for: the text-bearing file wins.
    #[test]
    fn pictures_lens_ignores_text_evidence_in_ordering() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();

        let long = format!("screenshot.png\napple dashboard {}", "words ".repeat(30));
        let screenshot = index(
            &store,
            corpus.path(),
            "screenshot.png",
            Kind::Image,
            &[(&long, None)],
        );
        let photo = index(&store, corpus.path(), "photo.png", Kind::Image, &[]);
        // The image arm prefers the photo; the keyword arm prefers the text.
        image_vector(&store, &screenshot, [0.0, 1.0, 0.0, 0.0]);
        image_vector(&store, &photo, [1.0, 0.0, 0.0, 0.0]);

        let query = parse_query(&["apple".to_string()]).unwrap();
        let pictures = search(
            data.path(),
            &query,
            SearchMode::Pictures,
            Some(&FakeImage),
            None,
            10,
        )
        .unwrap();
        assert!(
            pictures.hits[0].path.ends_with("photo.png"),
            "appearance alone orders the pictures lens"
        );

        let all = search(
            data.path(),
            &query,
            SearchMode::All,
            Some(&FakeImage),
            None,
            10,
        )
        .unwrap();
        assert!(
            all.hits[0].path.ends_with("screenshot.png"),
            "all keeps the measured text evidence weights"
        );
    }

    #[test]
    fn words_lens_searches_only_pictures_with_visible_text() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();

        index(
            &store,
            corpus.path(),
            "shot.png",
            Kind::Image,
            &[("shot.png\napple pie", None)],
        );
        index(
            &store,
            corpus.path(),
            "doc.pdf",
            Kind::Pdf,
            &[("doc.pdf\napple pie", None)],
        );

        let query = parse_query(&["apple".to_string()]).unwrap();
        let outcome = search(data.path(), &query, SearchMode::Words, None, None, 10).unwrap();

        assert_eq!(outcome.hits.len(), 1);
        assert!(outcome.hits[0].path.ends_with("shot.png"));
        assert_eq!(outcome.hits[0].kind, "image");
    }

    /// `samurai.png` is a picture named samurai; `samurai-notes.png` is a
    /// picture that says samurai. Only the second answers the words lens.
    #[test]
    fn words_lens_skips_a_picture_whose_only_text_is_the_name() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();

        index(
            &store,
            corpus.path(),
            "samurai.png",
            Kind::Image,
            &[("samurai.png", None)],
        );
        index(
            &store,
            corpus.path(),
            "samurai-notes.png",
            Kind::Image,
            &[("samurai-notes.png\nsamurai on a bridge", None)],
        );

        let query = parse_query(&["samurai".to_string()]).unwrap();
        let words = search(data.path(), &query, SearchMode::Words, None, None, 10).unwrap();
        assert_eq!(words.hits.len(), 1);
        assert!(words.hits[0].path.ends_with("samurai-notes.png"));

        let all = search(data.path(), &query, SearchMode::All, None, None, 10).unwrap();
        assert_eq!(all.hits.len(), 2, "all still finds both");
        let name_only = all
            .hits
            .iter()
            .find(|hit| hit.path.ends_with("samurai.png"))
            .expect("the name-only picture is an all-lens hit");
        assert!(name_only.name_only);
        let body = all
            .hits
            .iter()
            .find(|hit| hit.path.ends_with("samurai-notes.png"))
            .unwrap();
        assert!(!body.name_only, "a word inside the picture is not its name");
    }

    #[test]
    fn documents_lens_excludes_pictures() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();

        index(
            &store,
            corpus.path(),
            "doc.pdf",
            Kind::Pdf,
            &[("doc.pdf\napple pie", None)],
        );
        index(
            &store,
            corpus.path(),
            "shot.png",
            Kind::Image,
            &[("shot.png\napple pie", None)],
        );

        let query = parse_query(&["apple".to_string()]).unwrap();
        let outcome = search(data.path(), &query, SearchMode::Documents, None, None, 10).unwrap();

        assert_eq!(outcome.hits.len(), 1);
        assert!(outcome.hits[0].path.ends_with("doc.pdf"));
    }

    /// A filter the lens cannot honour is a note, not a silent miss: the user
    /// typed two things that contradict each other.
    #[test]
    fn a_conflicting_type_filter_returns_a_note() {
        let data = tempfile::tempdir().unwrap();
        Store::open(data.path()).unwrap();

        let query = parse_query(&["apple".to_string(), "type:pdf".to_string()]).unwrap();
        let outcome = search(data.path(), &query, SearchMode::Pictures, None, None, 10).unwrap();

        assert!(outcome.hits.is_empty());
        assert!(outcome.timings.is_empty());
        let note = outcome.notes.join("\n");
        assert!(note.contains("pictures"), "{note}");
        assert!(note.contains("type:pdf"), "{note}");
        assert!(note.contains("images"), "{note}");
    }
}
