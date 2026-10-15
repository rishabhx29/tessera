//! Which of the vision tower's outputs is the embedding?
//!
//! Every accuracy number in this repo is built on `pooler_output`, chosen by
//! name because the graphs also emit `last_hidden_state` and it seemed the more
//! semantically apt of the two. The numbers then said something might be off:
//! unrelated photographs sit at a median cosine of 0.59-0.63, and 48% of queries
//! missed despite the right image being one of eleven. A well-behaved contrastive
//! space puts unrelated images near zero.
//!
//! That reading was wrong, and this test is where it gets settled. The high
//! image-to-image similarity is anisotropy — normal for this family of models,
//! and confirmed separately by `cross_modal_alignment` — and the misses were the
//! model's attribute binding on an eleven-photo set built to stress exactly that.
//!
//! What this test demonstrates is why the *other* two candidates cannot be
//! swapped in. It reports each one's noise floor and, the decisive number, the
//! cosine between `pooler_output` and each alternative on the same image. If the
//! pooler sliced one position out of `last_hidden_state`, or averaged its patches,
//! that cosine would be 1.0. It is a few hundredths instead, signed either way, so
//! `pooler_output` is a learned transform and `last_hidden_state` sits in a
//! *different vector space* from the one the text tower was trained against.
//!
//! What that costs was measured on the 300-photo corpus by the harness recorded in
//! `docs/performance.md`. Of the nine tower combinations it tried, eight land
//! between 0.0013 and 0.0040 R@1 — chance is 0.0033, which is 0.0015 away by
//! sampling error alone on 1501 queries — and the ninth is the shipped pair at
//! 0.6629. No pooling of an unprojected space beats chance without a projection
//! trained to read it, which is what the pooler already is. Those corpus figures
//! come from that harness; what this test carries is the geometry behind them,
//! re-runnable from the repository.
//!
//! ```
//! cargo test --release -p tessera-core --test image_pooling -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

const DIM: usize = 768;
const SIDE: u32 = 224;

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

fn load(env: &ort::environment::Environment, path: &std::path::Path) -> Session {
    Session::builder(env)
        .expect("builder")
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .expect("optimise")
        .commit_from_file(path)
        .expect("load")
}

/// CHW, rescaled and normalised exactly as the engine does it.
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

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normalise(v: &mut [f32]) {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

#[test]
#[ignore = "needs the siglip model set"]
fn which_output_is_the_real_embedding() {
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

    let env = environment();
    let mut vision = load(
        env,
        &models
            .verified_path("siglip", "siglip-vision.int8.onnx")
            .expect("int8 vision"),
    );
    let names: Vec<String> = vision
        .outputs()
        .iter()
        .map(|o| o.name().to_string())
        .collect();
    eprintln!("\nvision outputs: {names:?}");

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("jpg" | "jpeg" | "png")
            )
        })
        .collect();
    paths.sort();
    if paths.len() > 24 {
        let stride = paths.len() / 24;
        paths = (0..24).map(|i| paths[i * stride].clone()).collect();
    }

    let input_name = vision
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .unwrap_or_default();

    // Three candidate representations from the same forward pass.
    let mut pooled: Vec<Vec<f32>> = Vec::new();
    let mut cls: Vec<Vec<f32>> = Vec::new();
    let mut mean: Vec<Vec<f32>> = Vec::new();

    for path in &paths {
        let px = pixels(path);
        let t = Tensor::from_array((
            [1usize, 3, SIDE as usize, SIDE as usize],
            px.into_boxed_slice(),
        ))
        .expect("tensor");
        let out = vision.run(vec![(input_name.clone(), t)]).expect("run");

        // pooler_output: [1, 768]
        let (shape, d) = out[1].try_extract_tensor::<f32>().expect("pooler");
        let dims: Vec<usize> = shape.iter().map(|s| *s as usize).collect();
        assert_eq!(dims, vec![1, DIM], "pooler shape");
        let mut p = d.to_vec();
        normalise(&mut p);
        pooled.push(p);

        // last_hidden_state: [1, patches, 768] - take the first token, and the
        // mean over tokens.
        let (lshape, ldata) = out[0].try_extract_tensor::<f32>().expect("hidden");
        let ldims: Vec<usize> = lshape.iter().map(|s| *s as usize).collect();
        eprintln!(
            "  {}: last_hidden_state {ldims:?}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        assert!(
            ldims.len() == 3 && ldims[2] == DIM,
            "hidden shape {ldims:?}"
        );
        let patches = ldims[1];

        let mut first = ldata[..DIM].to_vec();
        normalise(&mut first);
        cls.push(first);

        let mut avg = vec![0f32; DIM];
        for p in 0..patches {
            let base = p * DIM;
            for (i, a) in avg.iter_mut().enumerate() {
                *a += ldata[base + i];
            }
        }
        for a in avg.iter_mut() {
            *a /= patches as f32;
        }
        normalise(&mut avg);
        mean.push(avg);
        if mean.len() == 1 {
            eprintln!("first-token dim {DIM}, patches {patches}");
        }
    }

    for (label, vectors) in [
        ("pooler_output", &pooled),
        ("first token", &cls),
        ("mean patches", &mean),
    ] {
        let mut floors = Vec::new();
        for i in 0..vectors.len() {
            for j in (i + 1)..vectors.len() {
                floors.push(cosine(&vectors[i], &vectors[j]));
            }
        }
        floors.sort_by(|a, b| a.total_cmp(b));
        let median = floors[floors.len() / 2];
        let p5 = floors[floors.len() / 20];

        // Self-similarity baseline: if a vector does not score 1.0 against
        // itself, normalisation or indexing is wrong somewhere.
        let self_sim = cosine(&vectors[0], &vectors[0]);
        eprintln!(
            "\n== {label} ==\nnoise floor p5 {p5:.3}, median {median:.3}, \
             max {:.3}\nself-cosine {self_sim:.3}",
            floors[floors.len() - 1]
        );
    }

    // The decisive number. A pooler that sliced one position out of the hidden
    // state, or averaged its patches, would sit at cosine 1.0 with that
    // alternative on every image. Cosines of a few hundredths, signed either way,
    // say the pooler is a transform learned in training and that it reads a
    // different vector space from the one the alternatives live in — which is why
    // they cannot be swapped in, whatever their noise floors look like.
    let spread = |a: &[Vec<f32>], b: &[Vec<f32>]| -> (f32, f32, f32) {
        let mut v: Vec<f32> = (0..a.len()).map(|i| cosine(&a[i], &b[i])).collect();
        v.sort_by(|x, y| x.total_cmp(y));
        (v[0], v.iter().sum::<f32>() / v.len() as f32, v[v.len() - 1])
    };
    let (cls_min, cls_mean, cls_max) = spread(&pooled, &cls);
    let (pool_min, pool_mean, pool_max) = spread(&pooled, &mean);
    eprintln!(
        "\ncosine(pooler_output, alternative) over {} images:",
        pooled.len()
    );
    eprintln!("  first token  min {cls_min:+.4}  mean {cls_mean:+.4}  max {cls_max:+.4}");
    eprintln!("  mean patches min {pool_min:+.4}  mean {pool_mean:+.4}  max {pool_max:+.4}");
}
