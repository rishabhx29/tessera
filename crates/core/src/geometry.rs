//! Where a line of text sits on a picture.
//!
//! OCR engines report pixels; the window draws over an image scaled to whatever
//! the layout gives it. One type in between keeps the two from having to agree:
//! fractions of the picture, 0..1, rounded where they are stored.
//!
//! The same coordinates draw the amber marks in the wall and in the preview, so
//! a mark cannot land in one place on the tile and another in the sheet.

use serde_json::Value;

/// A rectangle on a picture, in fractions of its width and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl NormBox {
    /// A pixel rectangle on a picture of `width` by `height`, as fractions.
    ///
    /// Edges are clamped to the picture: a detector box can round one pixel
    /// past the last row, and a mark drawn outside the image would be a lie
    /// about where the words are. A picture with no pixels has no geometry.
    pub fn pixels(x0: f32, y0: f32, x1: f32, y1: f32, width: u32, height: u32) -> Self {
        if width == 0 || height == 0 {
            return Self {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            };
        }
        let width = width as f32;
        let height = height as f32;
        Self {
            x0: round4((x0 / width).clamp(0.0, 1.0)),
            y0: round4((y0 / height).clamp(0.0, 1.0)),
            x1: round4((x1 / width).clamp(0.0, 1.0)),
            y1: round4((y1 / height).clamp(0.0, 1.0)),
        }
    }

    fn from_json(value: &Value) -> Option<Self> {
        let items = value.as_array()?;
        if items.len() != 4 {
            return None;
        }
        let number = |index: usize| items[index].as_f64().map(|n| n as f32);
        Some(Self {
            x0: number(0)?,
            y0: number(1)?,
            x1: number(2)?,
            y1: number(3)?,
        })
    }

    fn to_json(self) -> Value {
        Value::Array(
            [self.x0, self.y0, self.x1, self.y1]
                .into_iter()
                .map(|value| Value::from(value as f64))
                .collect(),
        )
    }
}

fn round4(value: f32) -> f32 {
    (value * 10_000.0).round() / 10_000.0
}

/// The `tiles.boxes` value: one entry per line of the tile's text, `null` for
/// a line with no place on the picture (the filename line).
pub fn boxes_to_json(boxes: &[Option<NormBox>]) -> String {
    Value::Array(
        boxes
            .iter()
            .map(|bounds| match bounds {
                Some(bounds) => bounds.to_json(),
                None => Value::Null,
            })
            .collect(),
    )
    .to_string()
}

/// Parse a `tiles.boxes` value. Anything unexpected reads as no geometry: a
/// wrong box is worse than no box.
pub fn boxes_from_json(json: &str) -> Vec<Option<NormBox>> {
    let Ok(Value::Array(items)) = serde_json::from_str(json) else {
        return Vec::new();
    };
    let mut boxes = Vec::with_capacity(items.len());
    for item in &items {
        match item {
            Value::Null => boxes.push(None),
            value => match NormBox::from_json(value) {
                Some(bounds) => boxes.push(Some(bounds)),
                None => return Vec::new(),
            },
        }
    }
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_normalizes() {
        let bounds = NormBox::pixels(100.0, 50.0, 600.0, 250.0, 1000, 500);
        assert_eq!(
            bounds,
            NormBox {
                x0: 0.1,
                y0: 0.1,
                x1: 0.6,
                y1: 0.5,
            }
        );
    }

    #[test]
    fn edges_beyond_the_picture_are_clamped() {
        let bounds = NormBox::pixels(-10.0, -10.0, 1010.0, 510.0, 1000, 500);
        assert_eq!(
            bounds,
            NormBox {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            }
        );
    }

    #[test]
    fn coordinates_are_rounded_to_four_decimals() {
        let bounds = NormBox::pixels(1.0, 0.0, 2.0, 1.0, 3, 7);
        assert_eq!(bounds.x0, 0.3333);
        assert_eq!(bounds.x1, 0.6667);
        assert_eq!(bounds.y1, 0.1429);
    }

    #[test]
    fn boxes_round_trip() {
        let boxes = vec![
            None,
            Some(NormBox {
                x0: 0.0821,
                y0: 0.145,
                x1: 0.553,
                y1: 0.1812,
            }),
        ];
        assert_eq!(boxes_from_json(&boxes_to_json(&boxes)), boxes);
    }

    #[test]
    fn an_empty_list_is_no_geometry() {
        assert_eq!(boxes_to_json(&[]), "[]");
        assert!(boxes_from_json("[]").is_empty());
    }

    #[test]
    fn malformed_json_reads_as_no_geometry() {
        assert!(boxes_from_json("not json").is_empty());
        assert!(boxes_from_json("[[0.1, 0.2]]").is_empty(), "two numbers");
        assert!(
            boxes_from_json("[[0.1, 0.2, 0.3, \"x\"]]").is_empty(),
            "a string is not a coordinate"
        );
        assert!(boxes_from_json("{}").is_empty());
    }
}
