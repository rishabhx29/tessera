//! Vectors: what a search arm stores and how two of them are compared.
//!
//! This module owns the *representation*, deliberately separate from both the
//! model that produces an embedding and the store that persists one. Three
//! things live here because all three are easy to get subtly wrong and none of
//! them is interesting:
//!
//! - **A dimension is checked, not assumed.** A 768-d query against 512-d
//!   vectors is a silent wrong answer if it is not caught, and "search got
//!   worse for no reason" is the exact failure the store's version pinning
//!   exists to prevent.
//! - **The byte encoding is explicit.** Little-endian f32, written and read
//!   here, so nothing else in the engine has to know it.
//! - **Cosine is computed in f32, not f64.** The vectors are f32 to begin
//!   with; accumulating in f64 would be a different computation, and the
//!   whole point is that the stored value is the value that was compared.
//!
//! ## Why this is a brute-force scan and not sqlite-vec
//!
//! `docs/architecture.md` originally specified a `vec0` virtual table. Ticket 07
//! departed from that deliberately, and the reasoning is worth keeping:
//!
//! - The stored layout is the *same* either way — sqlite-vec's `vec0` table
//!   holds float32 BLOBs internally — so adopting it later is a change to where
//!   the cosine runs, not a migration of the data.
//! - The crossover is around 100–200k images. At the 50k the perf target names,
//!   the spike measured sqlite-vec's KNN adding ~140 ms, which is already over
//!   budget, so it does not buy the target it was adopted for.
//! - A loadable extension means a per-platform binary to pin, verify and ship.
//!   Tessera's pitch is one app anyone can run, and this is the kind of
//!   infrastructure that makes that untrue.
//!
//! The cost is linear in the corpus, and `docs/performance.md` carries the
//! measurement of what that actually is rather than a projection.

use anyhow::{bail, Result};

pub mod siglip;

/// Bytes per float32 on disk. Named rather than written as `4` so the encoder
/// and the decoder cannot drift apart.
const F32_BYTES: usize = 4;

/// One embedding, in the search space of whichever model produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding {
    values: Vec<f32>,
}

impl Embedding {
    /// An embedding of the given width.
    ///
    /// Rejects the wrong width here rather than at comparison time, because a
    /// width mismatch found during a query is a failure the user sees as "search
    /// stopped finding things" with nothing in the logs.
    pub fn new(values: Vec<f32>, expected_dim: usize) -> Result<Self> {
        if values.len() != expected_dim {
            bail!(
                "embedding has {} dimensions but this model produces {expected_dim}: \
                 these cannot be compared",
                values.len()
            );
        }
        if values.iter().any(|v| !v.is_finite()) {
            bail!("embedding contains a non-finite value");
        }
        Ok(Self { values })
    }

    pub fn dim(&self) -> usize {
        self.values.len()
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// The bytes as they go into SQLite: little-endian f32, which is what every
    /// other reader of this format will assume and what SQLite itself uses for
    /// reals.
    pub fn to_blob(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.values.len() * 4);
        for value in &self.values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Read a blob back, refusing a length that is not a whole number of
    /// floats. A truncated BLOB is corruption; silently padding it would
    /// produce vectors that merely look plausible.
    pub fn from_blob(bytes: &[u8], expected_dim: usize) -> Result<Self> {
        if bytes.len() != expected_dim * F32_BYTES {
            bail!(
                "stored vector is {} bytes, which is not {} float32 values",
                bytes.len(),
                expected_dim
            );
        }
        // `chunks` rather than `chunks_exact`: the length was already checked to be a
        // whole number of floats, so there is nothing to discard, and the exact variant
        // only earns an allow-attribute here.
        let mut values = Vec::with_capacity(expected_dim);
        for chunk in bytes.chunks(F32_BYTES) {
            values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Self::new(values, expected_dim)
    }

    /// Scale to unit length, in place.
    ///
    /// Returns the original norm. A zero vector has no direction, so it is left
    /// alone rather than divided by zero — and the caller gets `0.0` back from
    /// [`cosine`] against anything, which is the honest answer for "no
    /// direction" rather than a NaN that would poison a ranking.
    pub fn normalize(&mut self) -> f32 {
        let norm = self.norm();
        if norm > f32::EPSILON {
            for value in &mut self.values {
                *value /= norm;
            }
        }
        norm
    }

    fn norm(&self) -> f32 {
        self.values.iter().map(|v| v * v).sum::<f32>().sqrt()
    }

    /// A unit-length copy, for a query vector.
    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }
}

/// Cosine similarity, in `[-1, 1]`, with `0.0` for anything degenerate.
///
/// Both sides are assumed to be normalized already — [`Embedding::normalized`]
/// does that once at write time and once per query — so this is a dot product
/// rather than a division per call. That is not an optimisation for its own
/// sake: it is what keeps a scan over tens of thousands of vectors doing one
/// multiply-add per dimension instead of a square root.
pub fn cosine(a: &Embedding, b: &Embedding) -> f32 {
    if a.dim() != b.dim() {
        // Cannot happen through the constructors, which pin the width. Cheap
        // enough to keep as a guard against a future caller that bypasses them.
        return 0.0;
    }
    a.values().iter().zip(b.values()).map(|(x, y)| x * y).sum()
}

/// One row of a scan result: which file, and how well it matched.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredFile {
    pub file_id: i64,
    pub score: f32,
}

/// Rank `candidates` against `query`, best first.
///
/// This sorts the whole candidate list and then truncates.
///
/// An earlier version of this comment claimed a partial selection, which was not
/// true. The correction is recorded rather than deleted because the brute-force
/// decision in `docs/performance.md` rests on this function's cost, and an
/// inflated description of that cost would undermine the one number in this
/// module a reader cannot re-derive for themselves. A bounded selection is a real
/// optimisation with a real measurement behind it, and it does not exist yet.
pub fn rank(
    query: &Embedding,
    mut candidates: Vec<(i64, &Embedding)>,
    limit: usize,
) -> Vec<ScoredFile> {
    let mut scored: Vec<ScoredFile> = candidates
        .drain(..)
        .map(|(file_id, embedding)| ScoredFile {
            file_id,
            score: cosine(query, embedding),
        })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIM: usize = 4;

    fn embed(values: [f32; DIM]) -> Embedding {
        Embedding::new(values.to_vec(), DIM).expect("the test vector is the right width")
    }

    #[test]
    fn an_embedding_of_the_wrong_width_is_refused() {
        // The whole point of checking: a 3-d vector where 4 is expected would
        // otherwise produce a plausible-looking wrong answer.
        let err = Embedding::new(vec![1.0, 2.0, 3.0], DIM).expect_err("3 is not 4");
        assert!(format!("{err:#}").contains("3 dimensions"), "{err:#}");
    }

    #[test]
    fn a_non_finite_value_is_refused_rather_than_stored() {
        // NaN would propagate into every ranking it touched and come back as
        // "no results", which is the hardest failure to debug.
        let err = Embedding::new(vec![1.0, f32::NAN, 3.0, 4.0], DIM).expect_err("NaN");
        assert!(format!("{err:#}").contains("non-finite"), "{err:#}");
    }

    #[test]
    fn a_vector_survives_the_round_trip_through_bytes() {
        let original = embed([0.5, -0.25, 0.125, -1.0]);
        let restored = Embedding::from_blob(&original.to_blob(), DIM).expect("the blob is whole");
        assert_eq!(original, restored, "the bytes lost precision");
    }

    #[test]
    fn a_truncated_blob_is_corruption_not_a_short_vector() {
        // Padding a short blob would give a vector that merely looks plausible.
        let err = Embedding::from_blob(&[0u8; 7], DIM).expect_err("7 bytes is not 4 floats");
        assert!(format!("{err:#}").contains("not 4 float32"), "{err:#}");
    }

    #[test]
    fn normalizing_gives_a_unit_vector() {
        let mut vector = embed([3.0, 4.0, 0.0, 0.0]);
        assert_eq!(vector.normalize(), 5.0, "the norm of a 3-4 vector is 5");
        let norm = vector.values().iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6, "norm was {norm}");
    }

    #[test]
    fn a_zero_vector_normalises_to_itself_and_scores_zero() {
        // Dividing by the norm would be a NaN, and a NaN in a ranking sorts
        // arbitrarily and silently.
        let zero = embed([0.0; DIM]);
        assert_eq!(
            zero.clone().normalized(),
            zero,
            "the zero vector has no direction"
        );
        assert_eq!(
            cosine(&zero, &embed([1.0, 0.0, 0.0, 0.0])),
            0.0,
            "an undefined direction is zero similarity, not NaN"
        );
    }

    #[test]
    fn cosine_measures_direction_not_length() {
        // Both sides normalized, which is the contract. `cosine` is a dot
        // product, not a division, so this test is also the one that pins that
        // precondition.
        let a = embed([1.0, 0.0, 0.0, 0.0]).normalized();
        let same_direction_ten_times_longer = embed([10.0, 0.0, 0.0, 0.0]).normalized();
        assert!(
            (cosine(&a, &a) - 1.0).abs() < 1e-6,
            "identical direction is 1"
        );
        assert!(
            (cosine(&a, &same_direction_ten_times_longer) - 1.0).abs() < 1e-6,
            "length must not change the score"
        );
    }

    #[test]
    fn unnormalized_vectors_are_outside_the_documented_range() {
        // Pinned deliberately rather than guarded: `cosine` is a dot product
        // for speed, so a caller that skips `normalized` gets a score outside
        // [-1, 1]. This test exists so the precondition is a written contract
        // someone can find, not a trap. The two paths that could get it wrong
        // are the store (which normalizes on write) and the query (which
        // normalizes the text embedding before ranking).
        assert!(
            cosine(&embed([1.0, 0.0, 0.0, 0.0]), &embed([10.0, 0.0, 0.0, 0.0])) > 1.0,
            "if this ever stops being true, cosine has grown a division and the \
             performance note in docs/performance.md needs revisiting"
        );
    }

    #[test]
    fn opposite_directions_score_negative_one() {
        assert!(
            (cosine(&embed([1.0, 0.0, 0.0, 0.0]), &embed([-1.0, 0.0, 0.0, 0.0])) + 1.0).abs()
                < 1e-6
        );
    }

    #[test]
    fn ranking_is_best_first_and_honours_the_limit() {
        let query = embed([1.0, 0.0, 0.0, 0.0]);
        let best = embed([0.99, 0.01, 0.0, 0.0]);
        let middle = embed([0.6, 0.8, 0.0, 0.0]);
        let worst = embed([-1.0, 0.0, 0.0, 0.0]);

        let ranked = rank(&query, vec![(3, &worst), (1, &best), (2, &middle)], 10);
        assert_eq!(
            ranked.iter().map(|r| r.file_id).collect::<Vec<_>>(),
            vec![1, 2, 3],
            "not ordered by score"
        );

        let top_two = rank(&query, vec![(3, &worst), (1, &best), (2, &middle)], 2);
        assert_eq!(
            top_two.iter().map(|r| r.file_id).collect::<Vec<_>>(),
            vec![1, 2],
            "the limit must drop the worst, not an arbitrary one"
        );
    }

    #[test]
    fn ranking_an_empty_corpus_is_empty_not_an_error() {
        // A corpus that has never been embedded yet is a state, not a failure.
        let query = embed([1.0, 0.0, 0.0, 0.0]);
        assert!(rank(&query, Vec::new(), 10).is_empty());
    }
}
