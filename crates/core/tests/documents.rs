//! Ticket 08 acceptance: PDF and DOCX become searchable tiles through the
//! same `Pipeline` seam as text, with page context, an honest awaiting-OCR
//! record, and failures that never stop the run.
//!
//! The documents here are built byte by byte rather than committed as binary
//! fixtures. The interesting cases are relational — "the marker word is on
//! page four", "this page draws a rectangle and no glyphs" — and a builder
//! states that relationship in code where a blob would hide it, while a
//! committed file would pin whatever its bytes happened to exercise forever.

use std::io::Write;
use std::path::{Path, PathBuf};

use tessera_core::index::{
    docx::DocxPipeline, index_folder, pdf::PdfPipeline, text::TextPipeline, Extraction, Pipeline,
};
use tessera_core::search::search;
use tessera_core::store::Store;

fn write(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).unwrap();
}

fn store_at(dir: &Path) -> Store {
    Store::open(dir).unwrap()
}

/// The engine's document pipelines, in the order the CLI offers them.
fn pipelines() -> Vec<Box<dyn Pipeline>> {
    vec![
        Box::new(TextPipeline),
        Box::new(PdfPipeline),
        Box::new(DocxPipeline),
    ]
}

// -- Fixtures -------------------------------------------------------------

/// Escape a string for a PDF literal string: `\`, `(` and `)` are the only
/// characters that need it.
fn escape_pdf_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '(' => out.push_str(r"\("),
            ')' => out.push_str(r"\)"),
            other => out.push(other),
        }
    }
    out
}

/// A minimal, valid PDF whose pages are the given content streams: one
/// standard font, no compression, a real cross-reference table.
fn pdf_bytes(page_streams: &[String]) -> Vec<u8> {
    let page_count = page_streams.len();
    let mut objects: Vec<String> = Vec::new();
    // Object 1: catalog. Object 2: page tree. Object 3: the font.
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
    let kids: Vec<String> = (0..page_count)
        .map(|i| format!("{} 0 R", 5 + 2 * i))
        .collect();
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {page_count} >>",
        kids.join(" ")
    ));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    for (i, stream) in page_streams.iter().enumerate() {
        // Page i: content stream at object 4 + 2i, page object at 5 + 2i.
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            4 + 2 * i
        ));
    }

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// A PDF whose pages carry `pages` as one line of Helvetica text each.
fn simple_pdf(pages: &[&str]) -> Vec<u8> {
    let streams: Vec<String> = pages
        .iter()
        .map(|text| format!("BT /F1 18 Tf 72 720 Td ({}) Tj ET", escape_pdf_text(text)))
        .collect();
    pdf_bytes(&streams)
}

/// A PDF whose one page draws a rectangle and no glyphs — what a scan's
/// content stream looks like: bytes, no characters.
fn scanned_pdf() -> Vec<u8> {
    pdf_bytes(&["0 0 612 792 re f".to_string()])
}

/// A .docx whose only content is the given paragraphs of text.
fn simple_docx(paragraphs: &[&str]) -> Vec<u8> {
    const WORD_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let mut body = String::new();
    for paragraph in paragraphs {
        let text = paragraph
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        body.push_str(&format!(
            r#"<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#
        ));
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{WORD_NS}"><w:body>{body}</w:body></w:document>"#
    );
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(document.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buffer.into_inner()
}

// -- Acceptance -----------------------------------------------------------

#[test]
fn a_pdf_and_a_docx_are_searchable_in_the_same_index_as_text() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(
        dir,
        "quarterly-report.pdf",
        &simple_pdf(&[
            "alpha cover page",
            "beta page two",
            "gamma page three",
            "aardvark quarterly invoice from the airline",
            "delta closing page",
        ]),
    );
    write(
        dir,
        "lease-notes.docx",
        &simple_docx(&["the non-compete clause", "in the lease agreement for R&D"]),
    );
    write(dir, "plain.txt", b"a text file about zebras");

    let store = store_at(data.path());
    let summary = index_folder(&store, &pipelines(), dir).unwrap();
    assert_eq!(
        (summary.indexed, summary.failed, summary.awaiting_ocr),
        (3, 0, 0)
    );

    // The PDF is found from its words, and the hit names the page the words
    // are on — not the file's first page, not a chunk number.
    let hits = search(&store, &["aardvark"]).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "pdf");
    assert_eq!(hits[0].page, Some(4));
    assert!(hits[0].snippet.contains("aardvark"));
    assert_eq!(
        hits[0].path,
        dir.join("quarterly-report.pdf").to_string_lossy()
    );

    // Multi-term AND across the file, and the page is still the page.
    let hits = search(&store, &["invoice", "airline"]).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page, Some(4));

    // The DOCX has no pages, and the result says so.
    let hits = search(&store, &["non-compete"]).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "docx");
    assert_eq!(hits[0].page, None);
    assert!(hits[0].snippet.contains("non-compete"));

    // The ampersand survived the XML: "R&D" is stored as text, not as "RD".
    // Checked in the tile itself rather than through FTS, because how the
    // trigram tokenizer treats `&` is not the question this test is asking.
    let docx = dir.join("lease-notes.docx");
    let docx_id = store.file_id(&docx).unwrap();
    let stored = store.files_by_ids(&[docx_id]).unwrap().remove(0);
    assert!(
        stored.tiles.iter().any(|tile| tile.text.contains("R&D")),
        "the entity reference was lost: {:?}",
        stored
            .tiles
            .iter()
            .map(|tile| tile.text.as_str())
            .collect::<Vec<_>>()
    );

    // And text files still work in the same index: one query surface, several
    // formats, which is the product's whole claim.
    let hits = search(&store, &["zebras"]).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "text");
    assert_eq!(hits[0].page, None);
}

#[test]
fn a_scanned_pdf_is_recorded_as_awaiting_ocr_and_visible_in_stats() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "receipt.pdf", &scanned_pdf());
    write(dir, "note.txt", b"a perfectly readable note");

    let store = store_at(data.path());
    let summary = index_folder(&store, &pipelines(), dir).unwrap();
    assert_eq!(
        (summary.indexed, summary.awaiting_ocr, summary.failed),
        (1, 1, 0),
        "a scan is neither indexed content nor a failure"
    );

    let scan = dir.join("receipt.pdf");
    let row = store
        .file_state(&scan)
        .unwrap()
        .expect("the file is recorded");
    assert!(!row.extracted, "the row is pending work, not indexed");

    // Visible in `tessera stats`, and distinguishable from a failure.
    let report = tessera_core::cli::stats_report(&store).unwrap();
    assert!(
        report.iter().any(|line| line.starts_with("awaiting_ocr")),
        "stats does not report the scan: {report:?}"
    );
    assert!(
        !report.iter().any(|line| line.contains("FAILED")),
        "a scan is not a failure: {report:?}"
    );

    // Nothing was invented for it: the file is not searchable by name or by
    // content, because its content is exactly what is missing.
    assert!(
        search(&store, &["receipt"]).unwrap().is_empty(),
        "a pending row must not pretend to be indexed"
    );
    assert_eq!(
        search(&store, &["readable"]).unwrap().len(),
        1,
        "the readable file is unaffected"
    );
}

#[test]
fn a_broken_document_fails_with_its_reason_and_the_run_continues() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    write(dir, "broken.pdf", b"this is not a pdf at all");
    write(dir, "broken.docx", b"this is not a zip either");
    write(dir, "fine.txt", b"the invoice from the airline");

    let store = store_at(data.path());
    let summary = index_folder(&store, &pipelines(), dir).unwrap();
    assert_eq!(
        (summary.indexed, summary.failed),
        (1, 2),
        "one unreadable file must never stop the run"
    );

    let failures = store.failures().unwrap();
    assert_eq!(failures.len(), 2);
    for (path, reason) in &failures {
        assert!(
            path.ends_with(".pdf") || path.ends_with(".docx"),
            "unexpected failure: {path}"
        );
        assert!(
            reason.contains("reading "),
            "the reason for {path} does not name the file: {reason}"
        );
    }
    assert_eq!(
        search(&store, &["airline"]).unwrap().len(),
        1,
        "the readable file was still indexed"
    );
}

#[test]
fn a_long_pdf_page_chunks_without_losing_its_page_number() {
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let dir = corpus.path();
    let long: String = (0..450).map(|word| format!("word{word} ")).collect();
    write(
        dir,
        "long.pdf",
        &simple_pdf(&["short first page", long.as_str()]),
    );

    let store = store_at(data.path());
    index_folder(&store, &pipelines(), dir).unwrap();

    // "word449" is in the last chunk of page 2, and that chunk is still page 2.
    let hits = search(&store, &["word449"]).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].page,
        Some(2),
        "the last chunk of page 2 is still page 2"
    );
}

// -- Measurement ----------------------------------------------------------
//
// Ticket 08's latency acceptance, measured rather than assumed. The week-1
// spike measured Python's pypdf at ~400 ms p50 per document
// (docs/performance.md); these pipelines are pure Rust, and the number below
// is what this machine actually does through the same `Pipeline::process` the
// index loop calls.
//
//     set TESSERA_DOCS_CORPUS to a folder of real .pdf/.docx files
//     set TESSERA_DOCS_LIMIT (optional) to sample the first N
//     cargo test --release -p tessera-core --test documents -- --ignored --nocapture
//
// Extraction only: file load, page text, churn — not embedding and not the
// store. A fast extraction that reads nothing is not a result, so files whose
// text came out empty are counted and printed beside the timings.

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let index = (((sorted.len() - 1) as f64) * q).round() as usize;
    sorted[index]
}

#[test]
#[ignore = "needs TESSERA_DOCS_CORPUS pointing at real .pdf/.docx files"]
fn document_extraction_latency_is_measured() {
    let dir = PathBuf::from(
        std::env::var("TESSERA_DOCS_CORPUS")
            .expect("set TESSERA_DOCS_CORPUS to a folder of real .pdf/.docx files"),
    );
    let limit: Option<usize> = std::env::var("TESSERA_DOCS_LIMIT")
        .ok()
        .map(|value| value.parse().expect("TESSERA_DOCS_LIMIT is a number"));

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("reading TESSERA_DOCS_CORPUS")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    if let Some(limit) = limit {
        files.truncate(limit);
    }
    assert!(
        files.len() >= 20,
        "only {} documents in {}; too few to measure anything",
        files.len(),
        dir.display()
    );

    let pipelines: [(&str, Box<dyn Pipeline>); 2] = [
        ("pdf", Box::new(PdfPipeline)),
        ("docx", Box::new(DocxPipeline)),
    ];
    for (label, pipeline) in &pipelines {
        let sample: Vec<&PathBuf> = files
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case(label))
            })
            .collect();
        if sample.is_empty() {
            println!("{label}: no files in the corpus");
            continue;
        }

        let mut millis: Vec<f64> = Vec::new();
        let mut characters: Vec<usize> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        let mut awaiting = 0usize;
        let mut bytes = 0u64;
        for path in &sample {
            bytes += std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
            let started = std::time::Instant::now();
            let outcome = pipeline.process(path);
            millis.push(started.elapsed().as_secs_f64() * 1000.0);
            match outcome {
                Ok(Extraction::Tiles(tiles)) => {
                    characters.push(tiles.iter().map(|tile| tile.text.chars().count()).sum());
                }
                Ok(Extraction::AwaitingOcr) => awaiting += 1,
                Err(err) => failures.push(format!("{}: {err:#}", path.display())),
            }
        }

        let mut sorted = millis.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = millis.iter().sum::<f64>() / millis.len() as f64;
        let empty = characters.iter().filter(|&&count| count == 0).count();
        println!(
            "{label}: {} files, {:.1} MB, p50 {:.1} ms, p95 {:.1} ms, mean {:.1} ms, \
             {empty} with no text, {awaiting} awaiting OCR, {} failed",
            sample.len(),
            bytes as f64 / 1_000_000.0,
            percentile(&sorted, 0.50),
            percentile(&sorted, 0.95),
            mean,
            failures.len(),
        );
        for failure in failures.iter().take(5) {
            println!("  FAILED {failure}");
        }
        assert!(
            failures.len() * 4 <= sample.len(),
            "over a quarter of the {label} corpus failed; that measures the corpus, not the speed"
        );
    }
}
