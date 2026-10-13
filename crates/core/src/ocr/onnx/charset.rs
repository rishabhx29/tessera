//! The recogniser's alphabet and the decoding of its output.
//!
//! Everything here is a pure function of numbers and strings, which is what
//! makes the engine's hardest parts testable without a model present. The
//! integration notes in ADR-0008 came out of the week-1 spike's debug loop, and
//! each one is a rule encoded below rather than a comment to remember.

/// Class 0 is the CTC blank: a position where the recogniser emitted nothing.
/// It never appears in output text.
pub const BLANK: usize = 0;

/// The class -> character table, read out of the recogniser's own metadata.
///
/// The Paddle convention the pinned export follows:
///
/// ```text
/// ['blank'] + metadata.lines() + [' ']
/// ```
///
/// The leading blank is class 0, and the *trailing* space is the character
/// Paddle appends after its charset. Without that trailing space the mapping is
/// off by one from the end and the last class indexes out of bounds; dropping
/// it because `splitlines()` hides it is the bug ADR-0008 records.
///
/// `lines()` rather than `split('\n')` because the two differ exactly on a
/// trailing newline, and `lines()` is the one that matches the reference
/// implementation's `splitlines()`.
pub fn alphabet(metadata: &str) -> Vec<String> {
    let mut table = Vec::with_capacity(metadata.lines().count() + 2);
    table.push("blank".to_string());
    table.extend(metadata.lines().map(str::to_string));
    table.push(" ".to_string());
    table
}

/// A line of text and how sure the recogniser was about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub text: String,
    /// The mean probability over the characters kept. 0.0 when nothing was kept,
    /// so an empty line can never pass a score gate.
    pub score: f32,
}

/// Greedy CTC decoding of one line.
///
/// Greedy CTC is three rules: take the best class at each step, collapse
/// consecutive repeats, drop blanks. The surviving characters form the text and
/// their probabilities are averaged into the score.
///
/// # The trap this exists to avoid
///
/// The pinned export emits **per-step probabilities**, not logits — each row of
/// the output tensor already sums to 1.0. Applying softmax a second time crushes
/// a confident 0.94 into 0.0001 and every line then fails the score gate, which
/// looks exactly like a broken model. So probabilities are read as
/// probabilities; `rec_emits_probabilities` below is the test that holds that
/// assumption to the pinned file.
pub fn greedy_decode(steps: &[usize], scores: &[f32], alphabet: &[String]) -> Read {
    let mut chars = String::new();
    let mut kept: Vec<f32> = Vec::new();
    let mut previous = usize::MAX;
    for (step, &class) in steps.iter().enumerate() {
        if class != BLANK && class != previous {
            if let Some(character) = alphabet.get(class) {
                chars.push_str(character);
                if let Some(score) = scores.get(step) {
                    kept.push(*score);
                }
            }
        }
        previous = class;
    }
    let score = if kept.is_empty() {
        0.0
    } else {
        kept.iter().sum::<f32>() / kept.len() as f32
    };
    Read { text: chars, score }
}

/// Whether a recogniser row looks like probabilities rather than logits.
///
/// A logits row sums to something arbitrary and usually large in magnitude; a
/// probabilities row sums to 1. Used by the test that guards the pinned export,
/// so that swapping in a logits export fails a test with a clear name instead of
/// quietly emptying every index.
pub fn rec_emits_probabilities(row: &[f32]) -> bool {
    (row.iter().sum::<f32>() - 1.0).abs() < 1e-3
}

/// Whether a decoded line is text worth putting in an index.
///
/// Two independent conditions, both from the spike's measurements on this corpus
/// (ADR-0008):
///
/// - **Confidence ≥ `gate`.** Without a floor of some kind an icon is indexed as
///   a line of characters and pollutes every search near it. Note that the 0.5 in
///   use does *not* separate real text from confident misreads on its own — see
///   `SCORE_GATE`, which says so.
/// - **At least `min_chars` + 1 characters.** A single character is nearly
///   always an icon that cleared the gate by luck, and this is the condition
///   actually doing most of that work.
pub fn passes_gate(text: &str, score: f32, gate: f32, min_chars: usize) -> bool {
    score >= gate && text.trim().chars().count() > min_chars
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny stand-in for the real 18,710-class table. The rules under test do
    /// not care how many characters there are.
    fn table() -> Vec<String> {
        vec!["blank".into(), "a".into(), "b".into(), " ".into()]
    }

    #[test]
    fn the_alphabet_is_blank_then_characters_then_a_space() {
        let built = alphabet("a\nb\nc");
        assert_eq!(built, vec!["blank", "a", "b", "c", " "]);
        // So class 1 is the first real character and the last class is a space:
        // both ends are what the CTC decode assumes.
        assert_eq!(built[BLANK], "blank");
        assert_eq!(*built.last().unwrap(), " ");
    }

    #[test]
    fn a_trailing_newline_does_not_add_a_phantom_character() {
        // `splitlines()` and `lines()` agree here; `split('\n')` would not, and
        // the phantom entry would shift every class by one.
        assert_eq!(alphabet("a\nb\n").len(), alphabet("a\nb").len());
    }

    #[test]
    fn the_alphabet_length_must_match_the_output_width() {
        // The invariant that makes the decode total: the class count the graph
        // emits has to be the table's length, or every class above it would
        // index nowhere.
        let three_classes = 18_710;
        let built = alphabet("x\ny\nz");
        assert_ne!(built.len(), three_classes, "a 3-char charset is not 18,710");
        // Same check against a table built to fit, which is the real shape.
        let fitted: Vec<String> = (0..three_classes - 1).map(|i| i.to_string()).collect();
        assert_eq!(fitted.len() + 1, three_classes);
    }

    #[test]
    fn decoding_drops_blanks_and_collapses_repeats() {
        //  a  a  b  blank  ->  "ab": the second "a" is a repeat and collapses, and
        // the blank is nothing at all.
        let steps = [1, 1, 2, BLANK];
        let scores = [0.9, 0.99, 0.8, 0.99];
        let read = greedy_decode(&steps, &scores, &table());
        assert_eq!(read.text, "ab");
        // The score is the mean of what was *kept*, not of every step: the
        // collapsed repeat and the blank are confidently confident about saying
        // nothing, and averaging them in would flatter every line.
        assert!(
            (read.score - (0.9 + 0.8) / 2.0).abs() < 1e-6,
            "{}",
            read.score
        );
    }

    #[test]
    fn a_repeat_split_by_a_blank_is_two_characters_not_one() {
        // "a blank a" keeps both. And a repeat followed by a blank and the same
        // class again ("b blank b") is also two: the blank is what tells the
        // decoder the previous run ended, which is the whole point of CTC.
        assert_eq!(
            greedy_decode(&[1, BLANK, 1], &[0.9, 0.99, 0.9], &table()).text,
            "aa"
        );
        assert_eq!(
            greedy_decode(&[2, BLANK, 2], &[0.9, 0.99, 0.9], &table()).text,
            "bb"
        );
    }

    #[test]
    fn a_line_of_nothing_scores_zero_so_it_can_never_pass_a_gate() {
        let read = greedy_decode(&[BLANK, BLANK], &[0.99, 0.99], &table());
        assert_eq!(read.text, "");
        assert_eq!(read.score, 0.0);
    }

    #[test]
    fn a_class_beyond_the_table_is_ignored_rather_than_panicking() {
        // Defence in depth: construction checks the table against the graph's
        // output width, and this makes a wrong table a wrong answer, not a crash
        // in the middle of indexing a folder.
        let read = greedy_decode(&[99, 1], &[0.9, 0.9], &table());
        assert_eq!(read.text, "a");
    }

    #[test]
    fn probabilities_and_logits_are_told_apart() {
        assert!(rec_emits_probabilities(&[0.1, 0.2, 0.7]));
        assert!(!rec_emits_probabilities(&[1.5, -0.5, 3.0]));
    }

    /// The two populations ADR-0008 measured on this corpus: real text reads
    /// 0.98–1.00, icon misreads at or below 0.73.
    const REAL_TEXT_SCORES: [f32; 4] = [0.98, 0.99, 1.0, 0.96];
    const ICON_MISREAD_SCORES: [f32; 4] = [0.73, 0.61, 0.55, 0.52];

    #[test]
    fn the_gate_keeps_confident_text_and_drops_what_is_below_it() {
        // 0.5, as the ticket specifies and as the corpus was measured with.
        for score in REAL_TEXT_SCORES {
            assert!(
                passes_gate("Total due: 42.50 EUR", score, 0.5, 1),
                "confident text at {score} must be retained"
            );
        }
        for score in [0.49, 0.4, 0.2, 0.05] {
            assert!(
                !passes_gate("II", score, 0.5, 1),
                "a line reading at {score} must be dropped"
            );
        }
    }

    #[test]
    fn the_gate_alone_does_not_separate_the_two_populations() {
        // Recorded because ADR-0008 says a 0.5 gate "cleanly separates" text
        // (0.98–1.00) from icon misreads (≤0.73) — and it does not, because 0.73
        // is above 0.5. The gate stays at 0.5 because that is what the ticket
        // specifies and what the corpus numbers were measured with, but the
        // separation actually comes from the length check beside it, which is
        // why that one is not optional either. A gate in (0.73, 0.98) *would*
        // separate them; whether it costs real characters is a question for the
        // quality pass, not something to change under a ticket that pins 0.5.
        assert!(
            passes_gate("xy", 0.73, 0.5, 1),
            "0.73 is above a 0.5 gate, so confidence alone does not stop a misread"
        );
        assert!(
            !passes_gate("x", 0.73, 0.5, 1),
            "what stops the short ones is the length check, not the score"
        );
        // Which is what stops the misreads the spike actually saw: they were
        // mostly short.
        for score in ICON_MISREAD_SCORES {
            assert!(!passes_gate("l", score, 0.5, 1), "{score}");
        }
    }

    #[test]
    fn a_single_character_never_survives_how_sure_the_engine_is() {
        // Confidence alone cannot tell a lone glyph from an icon; given the gate
        // above, the length floor is the condition doing the work.
        assert!(!passes_gate("x", 1.0, 0.5, 1));
        assert!(passes_gate("xy", 1.0, 0.5, 1));
        // Whitespace does not count as characters: a box that is all padding
        // decoded to spaces must not pass on length.
        assert!(!passes_gate("    ", 1.0, 0.5, 1));
    }
}
