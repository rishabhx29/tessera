//! Is the image input actually reaching the model?
//!
//! Earlier measurements put the cosine between two *deliberately different*
//! images at 0.762, and unrelated photographs at 0.59. For a contrastive encoder
//! both are far too high: a red circle on white and a yellow triangle on black
//! should sit near zero, and a model that puts them at 0.76 has a representation
//! with almost no dynamic range.
//!
//! That pattern has a mundane explanation worth ruling out before spending an
//! hour downloading a bigger model: the preprocessed tensor is not varying the
//! way the pixels do. A whole-image bias, a constant channel, or a resize that
//! flattens everything would each produce a plausible-looking embedding that is
//! mostly the same vector.
//!
//! So this prints the input tensor's per-channel statistics and the spread of the
//! embeddings, for images that are as far apart as images can be.
//!
//! ```
//! cargo test --release -p tessera-core --test image_input_sanity -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

const SIDE: u32 = 224;
const DIM: usize = 768;

fn environment() -> &'static ort::environment::Environment {
    static ENV: OnceLock<std::result::Result<ort::environment::Environment, String>> =
        OnceLock::new();
    match ENV.get_or_init(|| ort::init().build().map_err(|e| format!("{e}"))) {
        Ok(e) => e,
        Err(m) => panic!("onnx runtime: {m}"),
    }
}

fn models() -> tessera_core::models::Models {
    let dir = std::env::var("TESSERA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            tessera_core::cli::models_dir(
                &tessera_core::cli::default_data_dir().expect("a data dir"),
                None,
            )
        });
    tessera_core::models::Models::open(&dir).expect("models dir")
}

/// The same preprocessing the engine uses, but with its statistics reported.
fn pixels(path: &std::path::Path) -> Vec<f32> {
    let image = image::open(path).expect("open").into_rgb8();
    let resized =
        image::imageops::resize(&image, SIDE, SIDE, image::imageops::FilterType::CatmullRom);
    let plane = (SIDE as usize) * (SIDE as usize);
    let mut out = vec![0f32; 3 * plane];
    for (x, y, p) in resized.enumerate_pixels() {
        let at = (y as usize) * (SIDE as usize) + (x as usize);
        for c in 0..3 {
            let v = p[c] as f32 / 255.0;
            out[c * plane + at] = (v - 0.5) / 0.5;
        }
    }
    out
}

fn stats(v: &[f32]) -> (f32, f32, f32) {
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / v.len() as f32;
    let mut sorted = v.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    (mean, var.sqrt(), sorted[sorted.len() / 2])
}

#[test]
#[ignore = "needs the siglip model set"]
fn the_image_input_reaches_the_model_with_some_dynamic_range() {
    let models = models();
    let folder = std::env::var("TESSERA_PHOTOS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(home)
                .join("OneDrive")
                .join("Pictures")
                .join("Screenshots")
        });
    assert!(folder.is_dir(), "no folder at {}", folder.display());

    // Three images as different from each other as a folder allows: a solid
    // colour, a photograph, and the committed fixture. If these all embed to
    // the same thing, the input is the problem.
    let dir = std::env::temp_dir().join("tessera-input-sanity");
    std::fs::create_dir_all(&dir).unwrap();
    let white = dir.join("white.png");
    let black = dir.join("black.png");
    image::RgbImage::from_pixel(64, 64, image::Rgb([255, 255, 255]))
        .save(&white)
        .unwrap();
    image::RgbImage::from_pixel(64, 64, image::Rgb([0, 0, 0]))
        .save(&black)
        .unwrap();

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("png" | "jpg" | "jpeg")
            )
        })
        .collect();
    paths.sort();
    let samples = vec![white, black, paths[0].clone()];

    eprintln!("\n== preprocessed input statistics ==");
    let mut tensors = Vec::new();
    for path in &samples {
        let px = pixels(path);
        let (mean, sd, median) = stats(&px);
        eprintln!(
            "{:?}\n   mean {mean:+.3}  sd {sd:.3}  median {median:+.3}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        tensors.push(px);
    }
    // The inputs must differ. Identical inputs would mean the preprocessing is
    // not reading the file at all.
    let a = &tensors[0];
    let b = &tensors[1];
    let diff = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .sum::<f32>()
        / a.len() as f32;
    eprintln!("mean |white - black| = {diff:.3}");
    assert!(
        diff > 0.5,
        "white and black preprocessing to the same tensor: {diff}"
    );

    // Now the embeddings.
    let env = environment();
    let mut session = {
        let path = models
            .verified_path("siglip", "siglip-vision.int8.onnx")
            .expect("int8 vision");
        Session::builder(env)
            .expect("builder")
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .expect("optimise")
            .commit_from_file(&path)
            .expect("load")
    };
    let input = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .unwrap_or_default();

    let mut pooled_vec: Vec<Vec<f32>> = Vec::new();
    let mut hidden_vec: Vec<(Vec<f32>, Vec<usize>)> = Vec::new();
    for tensor in &tensors {
        let t = Tensor::from_array((
            [1usize, 3, SIDE as usize, SIDE as usize],
            tensor.clone().into_boxed_slice(),
        ))
        .expect("tensor");
        let out = session.run(vec![(input.clone(), t)]).expect("run");
        // Both outputs, because the question is which of them responds to the
        // image at all. `pooler_output` is what the engine reads.
        let (_, pooled) = out[1].try_extract_tensor::<f32>().expect("pooler");
        let mut pv = pooled.to_vec();
        let n: f32 = pv.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n > f32::EPSILON {
            for x in pv.iter_mut() {
                *x /= n;
            }
        }
        pooled_vec.push(pv);

        let (hshape, hidden) = out[0].try_extract_tensor::<f32>().expect("hidden");
        let dims: Vec<usize> = hshape.iter().map(|d| *d as usize).collect();
        let mut hv = hidden.to_vec();
        let n: f32 = hv.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n > f32::EPSILON {
            for x in hv.iter_mut() {
                *x /= n;
            }
        }
        hidden_vec.push((hv, dims.clone()));
    }

    let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let names = ["white", "black", "a real image"];

    eprintln!("\n== pooler_output (what the engine reads) ==");
    for i in 0..pooled_vec.len() {
        for j in (i + 1)..pooled_vec.len() {
            eprintln!(
                "  {} vs {}: {:.3}",
                names[i],
                names[j],
                cos(&pooled_vec[i], &pooled_vec[j])
            );
        }
    }

    eprintln!("\n== first token of last_hidden_state ==");
    for i in 0..hidden_vec.len() {
        let a = &hidden_vec[i].0[..DIM];
        for j in (i + 1)..hidden_vec.len() {
            let b = &hidden_vec[j].0[..DIM];
            eprintln!("  {} vs {}: {:.3}", names[i], names[j], cos(a, b));
        }
    }
    eprintln!(
        "  (hidden shapes: {:?})",
        hidden_vec
            .iter()
            .map(|(_, d)| d.clone())
            .collect::<Vec<_>>()
    );

    eprintln!(
        "\nWhite against black is the diagnostic. If pooler_output says ~0.9\n\
         and the first token says something clearly different, the engine has\n\
         been reading the wrong output all along and the number is recoverable."
    );
}
