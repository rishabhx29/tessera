//! Is the image arm broken, or is it solving the wrong problem?
//!
//! The fusion test showed OCR text alone retrieving screenshots at 0.964 while
//! the image arm managed 0.268. That leaves two very different explanations:
//!
//! - the image arm is broken (wrong pooling, wrong preprocessing, broken model), or
//! - the image arm works and screenshots are simply a *text* problem, where any
//!   visual signal is noise.
//!
//! These are distinguishable. Images whose content is visual rather than textual
//! are the image arm's home ground; if it scores near chance there too, it is
//! broken. If it scores well, it is fine and the screenshot number was measuring
//! the corpus.
//!
//! The test below renders simple, unmistakable visual scenes and queries them
//! with their description. It is a weaker test than real photographs — flat
//! colours are not a dog — but it answers the question that is actually open.
//!
//! ```
//! cargo test --release -p tessera-core --test image_control -- --ignored --nocapture
//! ```

use tessera_core::models::Models;
use tessera_core::vectors::siglip::SiglipEmbedder;
use tessera_core::vectors::{cosine, Embedding};

const DIM: usize = 768;
const SIDE: u32 = 224;

fn models() -> Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        });
    Models::open(&dir).expect("models dir")
}

/// A flat scene: a background and a shape. Crude, but unambiguous.
fn scene(bg: [u8; 3], fg: [u8; 3], shape: Shape) -> image::RgbImage {
    let mut img = image::RgbImage::from_pixel(SIDE, SIDE, image::Rgb(bg));
    let r = SIDE / 4;
    let (cx, cy) = (SIDE / 2, SIDE / 2);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let dx = x as i32 - cx as i32;
            let dy = y as i32 - cy as i32;
            let inside = match shape {
                Shape::Circle => dx * dx + dy * dy <= (r * r) as i32,
                Shape::Square => dx.abs() <= r as i32 && dy.abs() <= r as i32,
                Shape::Triangle => dy <= 0 && dy.abs() + dx.abs() <= r as i32,
            };
            if inside {
                img.put_pixel(x, y, image::Rgb(fg));
            }
        }
    }
    image::imageops::resize(&img, SIDE, SIDE, image::imageops::FilterType::CatmullRom)
}

enum Shape {
    Circle,
    Square,
    Triangle,
}

/// Encode the same preprocessing the engine uses, so this cannot drift from it.
fn embed_rendered(embedder: &SiglipEmbedder, img: &image::RgbImage) -> Result<Embedding, String> {
    let pixels: Vec<f32> = img
        .pixels()
        .flat_map(|p| {
            [0usize, 1, 2].map(|c| {
                let value = p[c] as f32 / 255.0;
                (value - 0.5) / 0.5
            })
        })
        .collect();
    // The engine's own path is file-based; write to a temp file and use it, so
    // this test exercises the shipping code rather than a parallel copy of it.
    let dir = std::env::temp_dir().join(format!("tessera-control-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("scene.png");
    img.save(&path).map_err(|e| e.to_string())?;
    let out = embedder
        .embed_image(&path, DIM)
        .map_err(|e| format!("{e:#}"));
    let _ = pixels;
    let _ = std::fs::remove_file(&path);
    out
}

#[test]
#[ignore = "needs the siglip model set"]
fn the_image_arm_works_on_visual_content() {
    let models = models();
    assert!(
        SiglipEmbedder::is_installed(&models),
        "install the siglip set"
    );
    let embedder = SiglipEmbedder::new(&models, 4).expect("embedder");

    // (background, foreground, shape, the description that should find it)
    let scenes: Vec<([u8; 3], [u8; 3], Shape, &str)> = vec![
        (
            [255, 255, 255],
            [220, 30, 30],
            Shape::Circle,
            "a red circle on a white background",
        ),
        (
            [255, 255, 255],
            [30, 30, 220],
            Shape::Square,
            "a blue square on a white background",
        ),
        (
            [255, 255, 255],
            [30, 160, 30],
            Shape::Triangle,
            "a green triangle on a white background",
        ),
        (
            [20, 20, 20],
            [240, 240, 60],
            Shape::Circle,
            "a yellow circle on a black background",
        ),
        (
            [20, 20, 20],
            [240, 240, 60],
            Shape::Square,
            "a yellow square on a black background",
        ),
        (
            [20, 20, 20],
            [240, 240, 60],
            Shape::Triangle,
            "a yellow triangle on a black background",
        ),
    ];

    let mut images = Vec::new();
    for (bg, fg, shape, _) in &scenes {
        let img = scene(
            *bg,
            *fg,
            match shape {
                Shape::Circle => Shape::Circle,
                Shape::Square => Shape::Square,
                Shape::Triangle => Shape::Triangle,
            },
        );
        images.push(
            embed_rendered(&embedder, &img).unwrap_or_else(|e| panic!("embedding a scene: {e}")),
        );
    }

    // How far apart are these scenes? If even six deliberately different images
    // are all at cosine 0.5, the space is flat and everything else follows.
    let mut floors = Vec::new();
    for i in 0..images.len() {
        for j in (i + 1)..images.len() {
            floors.push(cosine(&images[i], &images[j]));
        }
    }
    floors.sort_by(|a, b| a.total_cmp(b));
    let median = floors[floors.len() / 2];
    eprintln!(
        "\n== distinct visual scenes ==\nmedian cosine between deliberately different images: \
         {median:.3}"
    );

    let mut hits = 0;
    for (i, (_, _, _, description)) in scenes.iter().enumerate() {
        let query = embedder
            .embed_text(description, DIM)
            .unwrap_or_else(|e| panic!("embedding {description:?}: {e:#}"));
        let mut best = (f32::MIN, usize::MAX);
        for (j, image) in images.iter().enumerate() {
            let score = cosine(&query, image);
            if score > best.0 {
                best = (score, j);
            }
        }
        let ok = best.1 == i;
        hits += usize::from(ok);
        eprintln!(
            "  {} {description:<42} -> scene {}{}",
            if ok { "HIT " } else { "miss" },
            best.1,
            if ok {
                String::new()
            } else {
                format!(" ({:.3} vs {:.3})", best.0, cosine(&query, &images[i]))
            }
        );
    }

    eprintln!("\nvisual-control top-1: {hits}/{}", scenes.len());
    assert_eq!(
        hits,
        scenes.len(),
        "the image arm cannot match a description to an unambiguous visual scene; \
         something is wrong with the embedding rather than with the corpus"
    );
}
