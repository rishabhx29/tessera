//! The image pipeline: decode is the engine's job, OCR produces the text, and
//! what lands in the index is that text.
//!
//! Images without text are not a failure. A photo of a dog has nothing to
//! read, and indexing "no text found" as an error would put half a photo
//! library into the failed column. So the pipeline records what it found —
//! which for a textless image is just the filename, which is one of the three
//! things people search for (product-definition 3.1).

use std::path::Path;

use anyhow::Result;

use super::{Extraction, Kind, Pipeline, Tile};
use crate::ocr::OcrEngine;
use crate::store::IMAGE_EMBEDDING_DIM;
use crate::vectors::siglip::SiglipEmbedder;
use crate::vectors::Embedding;

/// Formats the image pipeline claims: the architecture's format list. Whether
/// a given file decodes is the engine's problem to report, not the pipeline's to
/// predict.
const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tiff"];

/// Whether this pipeline claims a file extension. Public so the benchmark
/// measures the same corpus the pipeline would actually index, rather than a
/// second list that drifts from this one.
pub fn claims_extension(extension: &str) -> bool {
    let extension = extension.to_ascii_lowercase();
    EXTENSIONS.contains(&extension.as_str())
}

/// One tile per image.
///
/// An image is one unit of search ("the screenshot with the invoice"), and
/// splitting its text into word-windows would split the words a person
/// remembers seeing together. Documents chunk; screenshots do not.
pub struct ImagePipeline {
    engine: Box<dyn OcrEngine>,
    /// The image arm's embedder, or `None` when the 213 MB `siglip` set is not
    /// installed. Absent means the arm degrades to nothing, not that indexing
    /// fails: a user who has never run `tessera models install --set siglip`
    /// still gets every screenshot indexed and findable by its text.
    embedder: Option<std::sync::Arc<SiglipEmbedder>>,
}

impl ImagePipeline {
    pub fn new(engine: Box<dyn OcrEngine>) -> Self {
        Self {
            engine,
            embedder: None,
        }
    }

    /// Attach the image arm. Separate from [`ImagePipeline::new`] because the
    /// OCR engine is required and the embedder is optional, and conflating them
    /// would mean loading a vision tower to index a folder of text.
    pub fn with_embedder(mut self, embedder: Option<std::sync::Arc<SiglipEmbedder>>) -> Self {
        self.embedder = embedder;
        self
    }

    /// The engine's name, recorded on the file row so a diagnostics page can
    /// say which engine read a given screenshot.
    pub fn engine_name(&self) -> &'static str {
        self.engine.name()
    }
}

impl Pipeline for ImagePipeline {
    fn accepts(&self, path: &Path) -> Option<Kind> {
        let ext = path.extension()?.to_str()?;
        claims_extension(ext).then_some(Kind::Image)
    }

    fn process(&self, path: &Path) -> Result<Extraction> {
        let (lines, boxes): (Vec<_>, Vec<_>) = self
            .engine
            .recognize_lines(path)?
            .into_iter()
            .map(|line| (line.text, line.bounds))
            .unzip();
        let text = crate::ocr::lines_into_text(&lines);
        // An image with no text is not a failure: a photo has nothing to read,
        // and filing it as failed would put half a photo library in the failed
        // column for doing nothing wrong. No tiles means the loop's filename
        // step still gives it a tile, so the image stays findable by name.
        Ok(Extraction::Tiles(if text.trim().is_empty() {
            Vec::new()
        } else {
            vec![Tile {
                seq: 0,
                text,
                page: None,
                boxes,
            }]
        }))
    }

    /// SigLIP's embedding of the image, for the description arm.
    ///
    /// `Ok(None)` when the model set is not installed, which is a normal state
    /// and not an error: the OCR half of this pipeline still works.
    fn embed(&self, path: &Path) -> Result<Option<Embedding>> {
        match &self.embedder {
            Some(embedder) => Ok(Some(embedder.embed_image(path, IMAGE_EMBEDDING_DIM)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An engine that returns fixed text, so the pipeline is tested without a
    /// platform: this is the seam the architecture means.
    struct Fixed(&'static str);

    impl OcrEngine for Fixed {
        fn name(&self) -> &'static str {
            "fixed"
        }
        fn recognize(&self, _path: &Path) -> Result<String> {
            Ok(self.0.to_string())
        }
    }

    /// An engine that fails, standing in for a corrupt image.
    struct Broken;

    impl OcrEngine for Broken {
        fn name(&self) -> &'static str {
            "broken"
        }
        fn recognize(&self, path: &Path) -> Result<String> {
            anyhow::bail!("decoding {}: not a readable image", path.display())
        }
    }

    #[test]
    fn it_claims_the_image_formats_and_names_the_kind() {
        let pipeline = ImagePipeline::new(Box::new(Fixed("x")));
        for name in [
            "a.png", "b.jpg", "c.jpeg", "d.webp", "e.bmp", "f.gif", "g.tiff",
        ] {
            assert_eq!(
                pipeline.accepts(Path::new(name)),
                Some(Kind::Image),
                "{name}"
            );
        }
        assert_eq!(
            pipeline.accepts(Path::new("h.PNG")),
            Some(Kind::Image),
            "case-insensitive"
        );
        assert_eq!(pipeline.accepts(Path::new("i.txt")), None);
        assert_eq!(pipeline.accepts(Path::new("noext")), None);
    }

    #[test]
    fn default_ocr_lines_are_trimmed_without_invented_geometry() {
        let lines = Fixed("  first  \n\n second\r\n  ")
            .recognize_lines(Path::new("a.png"))
            .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "first");
        assert_eq!(lines[1].text, "second");
        assert!(lines.iter().all(|line| line.bounds.is_none()));
        assert!(Broken.recognize_lines(Path::new("broken.png")).is_err());
    }

    struct Positioned;

    impl OcrEngine for Positioned {
        fn name(&self) -> &'static str {
            "positioned"
        }

        fn recognize(&self, _path: &Path) -> Result<String> {
            Ok("invoice\ntotal".into())
        }

        fn recognize_lines(&self, _path: &Path) -> Result<Vec<crate::ocr::OcrLine>> {
            Ok(vec![
                crate::ocr::OcrLine {
                    text: "invoice".into(),
                    bounds: Some(crate::geometry::NormBox {
                        x0: 0.1,
                        y0: 0.2,
                        x1: 0.6,
                        y1: 0.3,
                    }),
                },
                crate::ocr::OcrLine {
                    text: "total".into(),
                    bounds: None,
                },
            ])
        }
    }

    #[test]
    fn an_image_tile_has_one_box_per_line() {
        let pipeline = ImagePipeline::new(Box::new(Positioned));
        let Extraction::Tiles(tiles) = pipeline.process(Path::new("invoice.png")).unwrap() else {
            panic!("an image returns tiles");
        };
        assert_eq!(tiles[0].text, "invoice\ntotal");
        assert_eq!(tiles[0].boxes.len(), 2);
        assert_eq!(tiles[0].boxes[1], None);
        let bounds = tiles[0].boxes[0].unwrap();
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (0.1, 0.2, 0.6, 0.3)
        );
    }

    #[test]
    fn image_dimensions_and_filename_alignment_are_recorded() {
        let data = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let path = corpus.path().join("invoice.png");
        image::RgbImage::new(80, 40).save(&path).unwrap();
        let store = crate::store::Store::open(data.path()).unwrap();
        let pipelines: Vec<Box<dyn Pipeline>> =
            vec![Box::new(ImagePipeline::new(Box::new(Positioned)))];
        let summary = crate::index::index_folder(&store, &pipelines, corpus.path()).unwrap();
        assert_eq!((summary.indexed, summary.failed), (1, 0));
        let id = store.file_id(&path).unwrap();
        let displays = store.file_displays(&[id]).unwrap();
        assert_eq!(
            (displays[&id].width, displays[&id].height),
            (Some(80), Some(40))
        );
        let file = store.files_by_ids(&[id]).unwrap().remove(0);
        assert_eq!(file.tiles[0].text, "invoice.png\ninvoice\ntotal");
        assert_eq!(file.tiles[0].boxes.len(), 3);
        assert_eq!(file.tiles[0].boxes[0], None);
        assert_eq!(file.tiles[0].boxes[1].unwrap().y0, 0.2);
        assert_eq!(file.tiles[0].boxes[2], None);

        image::RgbImage::new(40, 120).save(&path).unwrap();
        crate::index::index_folder(&store, &pipelines, corpus.path()).unwrap();
        let displays = store.file_displays(&[id]).unwrap();
        assert_eq!(
            (displays[&id].width, displays[&id].height),
            (Some(40), Some(120))
        );
    }

    #[test]
    fn recognised_text_becomes_one_tile() {
        let pipeline = ImagePipeline::new(Box::new(Fixed("the invoice from the airline")));
        let Extraction::Tiles(tiles) = pipeline.process(Path::new("a.png")).unwrap() else {
            panic!("an image with text is not awaiting OCR");
        };
        assert_eq!(tiles.len(), 1, "an image is one unit of search");
        assert_eq!(tiles[0].text, "the invoice from the airline");
        assert_eq!(tiles[0].seq, 0);
        assert_eq!(tiles[0].page, None, "an image has no pages");
    }

    #[test]
    fn an_image_with_no_text_is_not_a_failure() {
        // A photo has nothing to read. Indexing it as failed would put half a
        // photo library in the failed column for doing nothing wrong.
        let pipeline = ImagePipeline::new(Box::new(Fixed("   \n  ")));
        assert_eq!(
            pipeline.process(Path::new("dog.png")).unwrap(),
            Extraction::Tiles(Vec::new())
        );
    }

    #[test]
    fn an_unreadable_image_is_an_error_naming_the_file() {
        let pipeline = ImagePipeline::new(Box::new(Broken));
        let err = pipeline.process(Path::new("broken.png")).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("broken.png"), "{message}");
        assert!(message.contains("not a readable image"), "{message}");
    }

    #[test]
    fn the_engine_name_is_what_gets_recorded() {
        assert_eq!(
            ImagePipeline::new(Box::new(Fixed("x"))).engine_name(),
            "fixed"
        );
        assert_eq!(ImagePipeline::new(Box::new(Broken)).engine_name(), "broken");
    }
}
