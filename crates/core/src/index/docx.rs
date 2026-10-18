//! The DOCX pipeline: the visible text of `word/document.xml`.
//!
//! A .docx is a zip whose main text lives in one XML part. Paragraphs become
//! lines, tabs become tabs, and only visible runs count: field instructions
//! (`w:instrText`), deleted revisions (`w:delText`) and other edits marks stay
//! out, because they are not what the document reads as.
//!
//! A DOCX with no words is *not* awaiting OCR — there is nothing in it to OCR.
//! It becomes a name-only file, like an empty .txt.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use quick_xml::events::{BytesRef, Event};
use quick_xml::Reader;

use super::text::chunk_text;
use super::{Extraction, Kind, Pipeline, Tile};

const EXTENSIONS: &[&str] = &["docx"];

/// The main text part of a Word document, at this path in every .docx.
const DOCUMENT_PART: &str = "word/document.xml";

/// A cap on the decompressed XML. The 50 MB index cap bounds the *zip*, and
/// XML compresses well enough that a hostile or damaged file could decompress
/// to gigabytes; a document.xml beyond this is recorded as failed rather than
/// read to death.
const MAX_DOCUMENT_XML_BYTES: usize = 64 * 1024 * 1024;

/// Word documents: unzip, read the XML, chunk.
pub struct DocxPipeline;

impl Pipeline for DocxPipeline {
    fn accepts(&self, path: &Path) -> Option<Kind> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())?
            .to_ascii_lowercase();
        EXTENSIONS.contains(&ext.as_str()).then_some(Kind::Docx)
    }

    fn process(&self, path: &Path) -> Result<Extraction> {
        let xml = document_xml(path)?;
        let text = document_text(&xml).with_context(|| format!("reading {}", path.display()))?;
        Ok(Extraction::Tiles(
            chunk_text(&text)
                .into_iter()
                .enumerate()
                .map(|(seq, text)| Tile {
                    seq: seq as u32,
                    text,
                    page: None,
                })
                .collect(),
        ))
    }
}

/// The `word/document.xml` part of `path`, as text.
fn document_xml(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).with_context(|| {
        format!(
            "reading {}: not a zip archive (is it really a .docx?)",
            path.display()
        )
    })?;
    let entry = archive.by_name(DOCUMENT_PART).with_context(|| {
        format!(
            "reading {}: no {DOCUMENT_PART} inside (is it really a .docx?)",
            path.display()
        )
    })?;
    let mut bytes = Vec::new();
    entry
        .take(MAX_DOCUMENT_XML_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading {}: decompressing {DOCUMENT_PART}", path.display()))?;
    if bytes.len() > MAX_DOCUMENT_XML_BYTES {
        bail!(
            "reading {}: {DOCUMENT_PART} is over {} MB decompressed; refusing to read it",
            path.display(),
            MAX_DOCUMENT_XML_BYTES / (1024 * 1024)
        );
    }
    let xml = String::from_utf8(bytes)
        .with_context(|| format!("reading {}: {DOCUMENT_PART} is not UTF-8", path.display()))?;
    // Word writes UTF-8, sometimes with a byte-order mark; the XML parser does
    // not want the mark as content.
    Ok(xml.trim_start_matches('\u{feff}').to_string())
}

/// The visible text of a `word/document.xml`, paragraphs on their own lines.
///
/// Word escapes `&`, `<` and `>` as entities, and quick-xml (since 0.38)
/// reports every `&…;` as its own [`Event::GeneralRef`] instead of folding it
/// into the surrounding text — so a title like "R&D" arrives as three events,
/// and dropping the middle one would quietly index "RD".
fn document_text(xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    let mut in_paragraph = false;
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                "p" => in_paragraph = true,
                "t" => in_text = true,
                _ => {}
            },
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                "p" => {
                    in_paragraph = false;
                    out.push('\n');
                }
                "t" => in_text = false,
                _ => {}
            },
            // Tabs and line breaks are run-level siblings of `w:t`, not
            // children of it — the run is what puts them between words.
            Ok(Event::Empty(e)) if in_paragraph => match e.local_name().as_ref() {
                "tab" => out.push('\t'),
                "br" | "cr" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => out.push_str(&t),
            Ok(Event::GeneralRef(r)) if in_text => push_reference(&mut out, &r),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err).with_context(|| "parsing word/document.xml"),
        }
    }
    Ok(out)
}

/// Resolve one `&…;` reference into text: numeric character references and
/// the five entities every XML document may use without declaring them.
///
/// Anything else is kept verbatim, ampersand and semicolon included: an
/// undeclared entity is not something this reader can resolve, and text the
/// index never saw is text the user cannot find.
fn push_reference(out: &mut String, reference: &BytesRef<'_>) {
    if let Ok(Some(character)) = reference.resolve_char_ref() {
        out.push(character);
        return;
    }
    match reference.as_ref() {
        "amp" => out.push('&'),
        "lt" => out.push('<'),
        "gt" => out.push('>'),
        "quot" => out.push('"'),
        "apos" => out.push('\''),
        other => {
            out.push('&');
            out.push_str(other);
            out.push(';');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts(name: &str) -> Option<Kind> {
        DocxPipeline.accepts(Path::new(name))
    }

    #[test]
    fn claims_docx_and_names_the_kind() {
        assert_eq!(accepts("a.docx"), Some(Kind::Docx));
        assert_eq!(accepts("b.DOCX"), Some(Kind::Docx)); // case-insensitive
        assert_eq!(
            accepts("c.doc"),
            None,
            "the pre-2007 binary format is not docx"
        );
        assert_eq!(accepts("d.pdf"), None);
        assert_eq!(accepts("noext"), None);
    }

    const WORD_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

    fn document(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
               <w:document xmlns:w="{WORD_NS}"><w:body>{body}</w:body></w:document>"#
        )
    }

    #[test]
    fn paragraphs_become_lines_and_only_visible_runs_count() {
        let xml = document(
            r#"<w:p><w:r><w:t>the non-compete clause</w:t></w:r></w:p>
               <w:p>
                 <w:r><w:t>in the lease</w:t><w:tab/><w:t>agreement</w:t></w:r>
                 <w:r><w:delText>struck out yesterday</w:delText></w:r>
                 <w:r><w:instrText>PAGE \* MERGEFORMAT</w:instrText></w:r>
               </w:p>"#,
        );
        let text = document_text(&xml).unwrap();
        assert!(text.contains("the non-compete clause"), "{text:?}");
        assert!(
            text.contains("in the lease\tagreement"),
            "a tab between runs is part of the text: {text:?}"
        );
        assert!(
            !text.contains("struck out"),
            "deleted revisions are not what the document reads as: {text:?}"
        );
        assert!(
            !text.contains("MERGEFORMAT"),
            "field instructions are not text: {text:?}"
        );
        assert!(
            text.contains("the non-compete clause\n"),
            "a paragraph ends with a line break: {text:?}"
        );
    }

    #[test]
    fn entity_references_survive_as_text() {
        let xml = document(r#"<w:p><w:r><w:t>R&amp;D &#8212; the plan</w:t></w:r></w:p>"#);
        let text = document_text(&xml).unwrap();
        assert!(
            text.contains("R&D \u{2014} the plan"),
            "an ampersand must not vanish between two text events: {text:?}"
        );
    }

    #[test]
    fn an_undeclared_entity_is_kept_verbatim_rather_than_dropped() {
        let xml = document(r#"<w:p><w:r><w:t>a &bogus; b</w:t></w:r></w:p>"#);
        let text = document_text(&xml).unwrap();
        assert!(text.contains("a &bogus; b"), "{text:?}");
    }

    #[test]
    fn an_empty_document_is_empty_text_not_an_error() {
        let xml = document("<w:p><w:r><w:t></w:t></w:r></w:p>");
        assert_eq!(document_text(&xml).unwrap().trim(), "");
    }
}
