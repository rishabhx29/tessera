// The words lens shows the part of the picture that matched, not the whole
// picture. These are the fractions of the image that the crop card displays.

const DEFAULT_PADDING = 2.4;
const DEFAULT_MIN_WIDTH = 0.3;
const DEFAULT_CARD_ASPECT = 1.5;

function finite(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function clamp(value, low, high) {
  return Math.min(high, Math.max(low, value));
}

// A region around `mark`, in image fractions, whose on-screen shape is
// `cardAspect`. The mark keeps a wide margin, and a very wide or tall picture
// zooms out rather than cutting the crop.
export function cropAround(
  mark,
  {
    width,
    height,
    cardAspect = DEFAULT_CARD_ASPECT,
    padding = DEFAULT_PADDING,
    minWidth = DEFAULT_MIN_WIDTH,
  } = {},
) {
  const imageWidth = finite(width);
  const imageHeight = finite(height);
  const aspect = finite(cardAspect);
  if (!mark || imageWidth <= 0 || imageHeight <= 0 || aspect <= 0) return null;

  const xs = [finite(mark.x0), finite(mark.x1)];
  const ys = [finite(mark.y0), finite(mark.y1)];
  if ([...xs, ...ys].some((value) => value === null)) return null;

  const left = clamp(Math.min(...xs), 0, 1);
  const right = clamp(Math.max(...xs), 0, 1);
  const top = clamp(Math.min(...ys), 0, 1);
  const bottom = clamp(Math.max(...ys), 0, 1);
  if (right <= left || bottom <= top) return null;

  const imageAspect = imageWidth / imageHeight;
  const markWidth = right - left;
  const markHeight = bottom - top;

  let widthFraction = Math.max(markWidth * padding, minWidth);
  let heightFraction = (widthFraction * imageAspect) / aspect;
  const minHeight = markHeight * padding;
  if (heightFraction < minHeight) {
    heightFraction = minHeight;
    widthFraction = (heightFraction * aspect) / imageAspect;
  }
  if (widthFraction > 1) {
    widthFraction = 1;
    heightFraction = (widthFraction * imageAspect) / aspect;
  }
  if (heightFraction > 1) {
    heightFraction = 1;
    widthFraction = (heightFraction * aspect) / imageAspect;
  }
  if (widthFraction > 1) widthFraction = 1;
  if (heightFraction > 1) heightFraction = 1;

  const centreX = (left + right) / 2;
  const centreY = (top + bottom) / 2;
  const x0 = clamp(centreX - widthFraction / 2, 0, 1 - widthFraction);
  const y0 = clamp(centreY - heightFraction / 2, 0, 1 - heightFraction);
  return {
    x0,
    y0,
    x1: x0 + widthFraction,
    y1: y0 + heightFraction,
  };
}

// The crop is one scaled image inside a clipped card. These percentages place
// the image so that `region` fills the card exactly.
export function cropStyle(region) {
  const width = region.x1 - region.x0;
  const height = region.y1 - region.y0;
  return {
    width: `${100 / width}%`,
    height: `${100 / height}%`,
    left: `${(-region.x0 / width) * 100}%`,
    top: `${(-region.y0 / height) * 100}%`,
  };
}
