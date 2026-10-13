//! Finding text in an image and deciding how to feed it to the recogniser.
//!
//! Pure geometry and arithmetic: no model, no runtime, no I/O. The engine in
//! `super` is the part that is hard to test, so the parts that are easy to get
//! subtly wrong live here where a test can hold them.

/// An axis-aligned box in pixel coordinates, `x0,y0` inclusive and `x1,y1`
/// exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Box {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Box {
    pub fn width(&self) -> u32 {
        self.x1.saturating_sub(self.x0)
    }

    pub fn height(&self) -> u32 {
        self.y1.saturating_sub(self.y0)
    }

    /// Move a box from the detector's downscaled image back onto the original.
    ///
    /// `scale_x`/`scale_y` are the factors the image was shrunk by, so a
    /// detector pixel is `1/scale` original pixels. Clamped to the image, because
    /// the last box on the right or bottom edge always rounds past it and an
    /// out-of-range crop is a panic further down.
    pub fn scaled_back(&self, scale_x: f32, scale_y: f32, width: u32, height: u32) -> Box {
        let clamp_x = |value: f32| value.round().clamp(0.0, width as f32) as u32;
        let clamp_y = |value: f32| value.round().clamp(0.0, height as f32) as u32;
        Box {
            x0: clamp_x(self.x0 as f32 / scale_x),
            y0: clamp_y(self.y0 as f32 / scale_y),
            x1: clamp_x(self.x1 as f32 / scale_x),
            y1: clamp_y(self.y1 as f32 / scale_y),
        }
    }
}

/// The detector's probability map, thresholded into a binary mask of `w * h`.
pub fn threshold(probabilities: &[f32], width: usize, height: usize, at: f32) -> Vec<bool> {
    let mut mask = vec![false; width * height];
    for (pixel, probability) in mask.iter_mut().zip(probabilities) {
        *pixel = *probability > at;
    }
    debug_assert_eq!(mask.len(), width * height);
    mask
}

/// Every 8-connected run of set pixels, as its bounding box and pixel count.
///
/// Connected components rather than the detector's rotated-box postprocessing:
/// screen text is axis-aligned, and unrotating a screenshot's text regions costs
/// time and finds nothing (ADR-0008). The area is kept so a component can be
/// judged on how much ink it has, not only its size.
pub fn components(mask: &[bool], width: usize, height: usize) -> Vec<(Box, usize)> {
    let mut seen = vec![false; mask.len()];
    let mut found = Vec::new();
    // A stack, not recursion: a solid black region is 40,000 pixels deep and
    // recursing that far is a stack overflow in a program that is otherwise fine.
    let mut stack: Vec<usize> = Vec::new();

    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut min_x = start % width;
        let mut max_x = min_x;
        let mut min_y = start / width;
        let mut max_y = min_y;
        let mut area = 0usize;

        while let Some(index) = stack.pop() {
            area += 1;
            let x = index % width;
            let y = index / width;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let neighbour = ny as usize * width + nx as usize;
                    if mask[neighbour] && !seen[neighbour] {
                        seen[neighbour] = true;
                        stack.push(neighbour);
                    }
                }
            }
        }
        found.push((
            Box {
                x0: min_x as u32,
                y0: min_y as u32,
                x1: max_x as u32 + 1,
                y1: max_y as u32 + 1,
            },
            area,
        ));
    }
    found
}

/// Drop the components that are not text.
///
/// The floors are the spike's, and they do one job each: a handful of pixels is
/// noise, a box 3px on a side is a single misread glyph or an icon, and the
/// recogniser should not be asked about either. Everything that survives still
/// has to clear the score gate later — this only avoids wasting a model run on it.
pub fn plausible(components: &[(Box, usize)]) -> Vec<Box> {
    components
        .iter()
        .filter(|(bbox, area)| *area >= 10 && bbox.width() >= 3 && bbox.height() >= 3)
        .map(|(bbox, _)| *bbox)
        .collect()
}

/// Group line indices into batches whose **total width** stays under a limit.
///
/// The recogniser's output tensor is `[batch, steps, 18,710]`, so cost is driven
/// by batch size times width, not by the number of lines. Batching by count
/// alone puts twenty 2,000px lines in one batch and allocates the 18,710-wide
/// tensor for all of them (ADR-0008). Sorting by width first means each batch is
/// lines of similar width, which also pads far less.
pub fn batches_by_width(
    widths: &[usize],
    max_total_width: usize,
    max_batch: usize,
) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..widths.len()).collect();
    order.sort_by_key(|&index| widths[index]);

    let mut batches: Vec<Vec<usize>> = Vec::new();
    let mut batch: Vec<usize> = Vec::new();
    let mut total = 0usize;
    for index in order {
        let width = widths[index];
        // A line wider than the budget still gets its own batch: dropping it would
        // silently lose text, which is worse than a big allocation.
        if !batch.is_empty() && (total + width > max_total_width || batch.len() >= max_batch) {
            batches.push(std::mem::take(&mut batch));
            total = 0;
        }
        batch.push(index);
        total += width;
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// The size a line crop is fed to the recogniser at: fixed height, width by
/// aspect ratio.
///
/// `height` comes from the graph's own input shape rather than a constant here,
/// so a different export changes this without changing the code.
pub fn rec_size(
    crop_width: u32,
    crop_height: u32,
    height: u32,
    min_width: u32,
    max_width: u32,
) -> Option<(u32, u32)> {
    if crop_height < 2 || crop_width < 2 {
        return None;
    }
    let width = ((crop_width as f64 * height as f64 / crop_height as f64).round() as u32)
        .clamp(min_width, max_width);
    Some((width, height))
}

/// The detector's input size: the image scaled down by at most this long side,
/// and both sides rounded to a multiple of 32.
///
/// The 32 is not ours: the detector graph contains a resize to 1/32 of its input
/// and rejects anything that is not divisible by it (`{1,64,27,57} != {1,64,28,58}`
/// from the spike's first attempt). Rounding to the multiple is what keeps the
/// run from failing on any image whose height is not 32k.
pub fn det_size(width: u32, height: u32, max_side: u32, multiple: u32) -> (u32, u32) {
    let longest = width.max(height) as f32;
    let scale = if longest > max_side as f32 {
        max_side as f32 / longest
    } else {
        1.0
    };
    let fit = |side: u32| -> u32 {
        let scaled = (side as f32 * scale / multiple as f32).round() as u32;
        (scaled.max(1) * multiple).max(multiple)
    };
    (fit(width), fit(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `x`-shaped helper so tests read as pictures.
    fn mask_of(width: usize, height: usize, on: &[(usize, usize)]) -> Vec<bool> {
        let mut mask = vec![false; width * height];
        for &(x, y) in on {
            mask[y * width + x] = true;
        }
        mask
    }

    #[test]
    fn thresholding_keeps_what_is_above_the_gate() {
        let mask = threshold(&[0.29, 0.31, 0.9, 0.0], 2, 2, 0.3);
        assert_eq!(mask, vec![false, true, true, false]);
    }

    #[test]
    fn one_component_is_one_box() {
        // A 3x2 block inside a 6x4 mask.
        let mask = mask_of(6, 4, &[(1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (3, 2)]);
        let found = components(&mask, 6, 4);
        assert_eq!(found.len(), 1);
        let (bbox, area) = found[0];
        assert_eq!(
            bbox,
            Box {
                x0: 1,
                y0: 1,
                x1: 4,
                y1: 3
            }
        );
        assert_eq!(area, 6);
        assert_eq!(bbox.width(), 3);
        assert_eq!(bbox.height(), 2);
    }

    #[test]
    fn diagonal_neighbours_are_one_component_because_screens_are_diagonal() {
        // 8-connectivity: a slash, a backslash and a dotted "i" all stay one
        // glyph. Under 4-connectivity each pixel became its own component and a
        // line of text shattered into unreadable fragments.
        let mask = mask_of(5, 3, &[(0, 0), (1, 1), (2, 0), (2, 1), (2, 2)]);
        let found = components(&mask, 5, 3);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].0,
            Box {
                x0: 0,
                y0: 0,
                x1: 3,
                y1: 3
            }
        );
    }

    #[test]
    fn separated_blobs_stay_separate() {
        let mask = mask_of(10, 1, &[(1, 0), (2, 0), (8, 0)]);
        let found = components(&mask, 10, 1);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0.x1, 3);
        assert_eq!(
            found[1].0,
            Box {
                x0: 8,
                y0: 0,
                x1: 9,
                y1: 1
            }
        );
    }

    #[test]
    fn a_whole_image_of_ink_is_componentised_without_blowing_the_stack() {
        // 200x200 all set is 40,000 deep. Recursion here is a stack overflow in a
        // program that is otherwise working; the test is the evidence.
        let (w, h) = (200, 200);
        let mask = vec![true; w * h];
        let found = components(&mask, w, h);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].0,
            Box {
                x0: 0,
                y0: 0,
                x1: 200,
                y1: 200
            }
        );
        assert_eq!(found[0].1, w * h);
    }

    #[test]
    fn noise_and_single_glyphs_are_not_offered_to_the_recogniser() {
        let tiny_noise = (
            Box {
                x0: 0,
                y0: 0,
                x1: 2,
                y1: 2,
            },
            4,
        );
        let small_glyph = (
            Box {
                x0: 0,
                y0: 0,
                x1: 3,
                y1: 3,
            },
            9,
        );
        let a_line = (
            Box {
                x0: 0,
                y0: 0,
                x1: 300,
                y1: 20,
            },
            60,
        );
        let kept = plausible(&[tiny_noise, small_glyph, a_line]);
        assert_eq!(kept, vec![a_line.0]);
    }

    #[test]
    fn boxes_map_back_onto_the_original_and_are_clamped_to_it() {
        let bbox = Box {
            x0: 64,
            y0: 32,
            x1: 640,
            y1: 320,
        };
        // Halved on the way in, so doubled on the way out.
        assert_eq!(
            bbox.scaled_back(0.5, 0.5, 1920, 1080),
            Box {
                x0: 128,
                y0: 64,
                x1: 1280,
                y1: 640
            }
        );
        // The right and bottom edges always round past the image; an out-of-range
        // crop is a panic in the slicing code, not a slightly wrong box.
        let edge = Box {
            x0: 1264,
            y0: 1016,
            x1: 1300,
            y1: 1100,
        };
        let clamped = edge.scaled_back(0.5, 0.5, 1280, 1080);
        assert_eq!(clamped.x1, 1280);
        assert_eq!(clamped.y1, 1080);
    }

    #[test]
    fn batches_are_capped_by_total_width_and_by_count() {
        let widths = [100, 100, 100, 100, 100, 100];
        let by_width = batches_by_width(&widths, 250, 24);
        assert_eq!(by_width, vec![vec![0, 1], vec![2, 3], vec![4, 5]]);

        let by_count = batches_by_width(&widths, 100_000, 2);
        assert_eq!(by_count, vec![vec![0, 1], vec![2, 3], vec![4, 5]]);
    }

    #[test]
    fn a_line_wider_than_the_budget_still_gets_read() {
        // Dropping it would silently lose a line of text, which is a worse bug
        // than a large allocation.
        let widths = [50, 5_000];
        let batches = batches_by_width(&widths, 250, 24);
        assert_eq!(batches, vec![vec![0], vec![1]]);
    }

    #[test]
    fn batches_group_similar_widths_so_less_of_the_batch_is_padding() {
        // Sorted by width: the two narrow lines batch together and the two wide
        // ones do not get padded against them. Batch-by-count alone would have put
        // all four in one batch and padded 1,000px of output for 30px of lines.
        let widths = [1_000, 10, 900, 20];
        assert_eq!(
            batches_by_width(&widths, 100, 24),
            vec![vec![1, 3], vec![2], vec![0]]
        );
        // With room for everything, order is still by width so padding is minimal.
        assert_eq!(
            batches_by_width(&widths, 10_000, 24),
            vec![vec![1, 3, 2, 0]]
        );
    }

    #[test]
    fn a_line_is_resized_to_the_graphs_height_and_keeps_its_aspect() {
        assert_eq!(rec_size(200, 40, 48, 16, 1920), Some((240, 48)));
        // A sliver of a line still has to be wide enough for the graph.
        assert_eq!(rec_size(4, 40, 48, 16, 1920), Some((16, 48)));
        // A very long line is capped rather than allowed to allocate without end.
        assert_eq!(rec_size(9_000, 40, 48, 16, 1920), Some((1920, 48)));
        // Anything degenerate is refused rather than turned into a 1-pixel crop.
        assert_eq!(rec_size(1, 40, 48, 16, 1920), None);
        assert_eq!(rec_size(200, 1, 48, 16, 1920), None);
    }

    #[test]
    fn detection_size_is_a_multiple_of_32_or_the_graph_refuses_to_run() {
        // 420 is not divisible by 32: this is the exact input that produced
        // `{1,64,27,57} != {1,64,28,58}` in the spike.
        let (w, h) = det_size(900, 420, 1280, 32);
        assert_eq!(h % 32, 0, "height {h} must be a multiple of 32");
        assert_eq!(w % 32, 0);
        assert!(w >= 32 && h >= 32);
    }

    #[test]
    fn a_small_image_is_not_upscaled_and_a_huge_one_is_capped() {
        assert_eq!(det_size(800, 600, 1280, 32), (800, 608));
        let (w, h) = det_size(3840, 2160, 1280, 32);
        assert_eq!(w, 1280);
        assert_eq!(
            h, 736,
            "2160 * 1280/3840 = 720 rounds up to 23 blocks of 32"
        );
        assert!(det_size(10, 10, 1280, 32).0 >= 32, "never below one block");
    }
}
