//! Groups only geometrically compatible OCR lines; the source attachment stays immutable.
use crate::translation_input::{TextBlock, MAX_TEXT_CHARS};
use crate::translation_typography::source_font_size;

#[derive(Clone, Debug)]
pub struct Paragraph {
    pub source: TextBlock,
    pub source_block_indices: Vec<usize>,
    pub source_font_size: f64,
}

// Called only after translation_input::validate has bounded geometry, colors and text.
pub fn group(blocks: &[TextBlock]) -> Vec<Paragraph> {
    let mut indices: Vec<_> = (0..blocks.len()).collect();
    indices.sort_by(|&a, &b| {
        blocks[a]
            .top
            .total_cmp(&blocks[b].top)
            .then(blocks[a].left.total_cmp(&blocks[b].left))
            .then(a.cmp(&b))
    });
    let mut rows: Vec<Paragraph> = Vec::new();
    for index in indices {
        let next = paragraph(vec![index], blocks);
        let nearest = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| same_row(row, &next))
            .filter(|(_, row)| can_union(row, &next, blocks))
            .min_by(|(_, a), (_, b)| {
                horizontal_gap(&a.source, &next.source)
                    .total_cmp(&horizontal_gap(&b.source, &next.source))
            })
            .map(|(index, _)| index);
        if let Some(row) = nearest {
            let mut members = rows[row].source_block_indices.clone();
            members.push(index);
            members.sort_by(|&a, &b| blocks[a].left.total_cmp(&blocks[b].left).then(a.cmp(&b)));
            rows[row] = paragraph(members, blocks);
        } else {
            rows.push(next);
        }
    }
    // Top-coordinate jitter can insert a bridging fragment after both neighbors.
    loop {
        let pair = (0..rows.len()).find_map(|a| {
            (a + 1..rows.len())
                .find(|&b| same_row(&rows[a], &rows[b]) && can_union(&rows[a], &rows[b], blocks))
                .map(|b| (a, b))
        });
        let Some((a, b)) = pair else { break };
        let mut members = rows[a].source_block_indices.clone();
        members.extend(rows.remove(b).source_block_indices);
        members.sort_by(|&a, &b| blocks[a].left.total_cmp(&blocks[b].left).then(a.cmp(&b)));
        rows[a] = paragraph(members, blocks);
    }
    rows.sort_by(|a, b| {
        a.source
            .top
            .total_cmp(&b.source.top)
            .then(a.source.left.total_cmp(&b.source.left))
    });
    let mut paragraphs: Vec<(Paragraph, Paragraph)> = Vec::new();
    for row in rows {
        let nearest = paragraphs
            .iter()
            .enumerate()
            .filter(|(_, (whole, last))| continues_paragraph(whole, last, &row))
            .filter(|(_, (whole, _))| can_union(whole, &row, blocks))
            .min_by(|(_, (_, a)), (_, (_, b))| {
                (row.source.top - bottom(&a.source))
                    .total_cmp(&(row.source.top - bottom(&b.source)))
            })
            .map(|(index, _)| index);
        if let Some(index) = nearest {
            let mut members = paragraphs[index].0.source_block_indices.clone();
            members.extend_from_slice(&row.source_block_indices);
            paragraphs[index] = (paragraph(members, blocks), row);
        } else {
            paragraphs.push((row.clone(), row));
        }
    }
    paragraphs.sort_by_key(|(whole, _)| whole.source_block_indices.iter().min().copied());
    paragraphs.into_iter().map(|(whole, _)| whole).collect()
}

fn paragraph(indices: Vec<usize>, blocks: &[TextBlock]) -> Paragraph {
    let mut source = blocks[indices[0]].clone();
    let mut fonts = Vec::with_capacity(indices.len());
    source.text.clear();
    for &index in &indices {
        let block = &blocks[index];
        let right = right(&source).max(right(block));
        let bottom = bottom(&source).max(bottom(block));
        source.left = source.left.min(block.left);
        source.top = source.top.min(block.top);
        source.width = right - source.left;
        source.height = bottom - source.top;
        if !source.text.is_empty() {
            source.text.push(' ');
        }
        source.text.push_str(block.text.trim());
        fonts.push(source_font_size(block));
    }
    fonts.sort_by(f64::total_cmp);
    Paragraph {
        source,
        source_block_indices: indices,
        source_font_size: fonts[fonts.len() / 2],
    }
}

fn same_row(a: &Paragraph, b: &Paragraph) -> bool {
    let (a_box, b_box) = (&a.source, &b.source);
    let overlap = bottom(a_box).min(bottom(b_box)) - a_box.top.max(b_box.top);
    let gap = horizontal_gap(a_box, b_box);
    compatible(a, b, 1.5)
        && overlap >= a_box.height.min(b_box.height) * 0.65
        && gap >= 0.0
        && gap <= a_box.height.min(b_box.height) * 0.6
}

fn continues_paragraph(whole: &Paragraph, last: &Paragraph, next: &Paragraph) -> bool {
    let (a, b) = (&last.source, &next.source);
    let gap = b.top - bottom(a);
    let height = a.height.min(b.height);
    let short_ending = a.width < whole.source.width.max(b.width) * 0.8
        && a.text
            .trim_end()
            .ends_with(['.', '!', '?', '。', '！', '？']);
    compatible(whole, next, 1.25)
        && gap >= -height * 0.1 && gap <= height * 0.65
        && (a.left - b.left).abs() <= height * 0.75
        && right(a).min(right(b)) - a.left.max(b.left) >= a.width.min(b.width) * 0.65
        && !short_ending && !list_entry(&b.text)
        // Short stacked controls lack enough evidence to be prose continuation.
        && (prose_line(&a.text) || prose_line(&b.text))
        && (a.width >= last.source_font_size * 8.0 || b.width >= next.source_font_size * 8.0)
}

fn prose_line(text: &str) -> bool {
    text.chars().filter(|ch| !ch.is_whitespace()).count() >= 12
        || text
            .chars()
            .filter(|ch| !ch.is_ascii() && ch.is_alphabetic())
            .count()
            >= 8
}

fn can_union(a: &Paragraph, b: &Paragraph, blocks: &[TextBlock]) -> bool {
    if a.source.text.chars().count() + b.source.text.chars().count() + 1 > MAX_TEXT_CHARS {
        return false;
    }
    let left = a.source.left.min(b.source.left);
    let top = a.source.top.min(b.source.top);
    let right = right(&a.source).max(right(&b.source));
    let bottom = bottom(&a.source).max(bottom(&b.source));
    // A filled paragraph rectangle must never paint over another OCR region.
    !blocks.iter().enumerate().any(|(index, block)| {
        !a.source_block_indices.contains(&index)
            && !b.source_block_indices.contains(&index)
            && block.left < right
            && block.left + block.width > left
            && block.top < bottom
            && block.top + block.height > top
    })
}

fn compatible(a: &Paragraph, b: &Paragraph, ratio: f64) -> bool {
    a.source_font_size.max(b.source_font_size) <= a.source_font_size.min(b.source_font_size) * ratio
        && close_color(&a.source.text_color, &b.source.text_color)
        && close_color(&a.source.background_color, &b.source.background_color)
}

fn close_color(a: &str, b: &str) -> bool {
    // OCR color sampling jitters by a few levels even within one printed line.
    [1, 3, 5].into_iter().all(|index| {
        let a = u8::from_str_radix(&a[index..index + 2], 16).unwrap();
        let b = u8::from_str_radix(&b[index..index + 2], 16).unwrap();
        a.abs_diff(b) <= 16
    })
}

fn list_entry(text: &str) -> bool {
    let text = text.trim_start();
    let after_number = text.trim_start_matches(|ch: char| ch.is_ascii_digit());
    text.starts_with(['-', '*', '•', '●'])
        || (after_number.len() < text.len()
            && (after_number.starts_with(". ") || after_number.starts_with(") ")))
}

fn right(block: &TextBlock) -> f64 {
    block.left + block.width
}
fn bottom(block: &TextBlock) -> f64 {
    block.top + block.height
}
fn horizontal_gap(a: &TextBlock, b: &TextBlock) -> f64 {
    a.left.max(b.left) - right(a).min(right(b))
}
