// Justified rows for the picture wall.
//
// Pure layout: no DOM, so the row heights and widths can be tested without a
// browser. The wall is a sequence of rows that each fill the container width
// at a common height; the last row keeps the target height and is left
// aligned. Pictures with no stored size are laid out at 4:3.

const DEFAULT_ASPECT = 4 / 3;

function aspectOf(item) {
  return Number.isFinite(item?.aspect) && item.aspect > 0 ? item.aspect : DEFAULT_ASPECT;
}

export function layoutRows(items, containerWidth, target = 168, gap = 6) {
  const width = Math.max(0, Number(containerWidth) || 0);
  const rows = [];

  let row = [];
  let aspects = 0;

  const finish = (stretch) => {
    const height = stretch && width > 0 ? (width - gap * (row.length - 1)) / aspects : target;
    let x = 0;
    const placed = row.map((item) => {
      const itemWidth = aspectOf(item) * height;
      const entry = { item, x, width: itemWidth, height };
      x += itemWidth + gap;
      return entry;
    });
    return { height, items: placed };
  };

  for (const item of items) {
    row.push(item);
    aspects += aspectOf(item);
    const natural = aspects * target + gap * (row.length - 1);
    if (natural >= width && width > 0) {
      rows.push(finish(true));
      row = [];
      aspects = 0;
    }
  }
  if (row.length) rows.push(finish(false));
  return rows;
}
