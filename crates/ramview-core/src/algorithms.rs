use crate::models::ProcessMemoryInfo;

/// Calculated 2D boundary rectangle for a visual process node within the treemap.
#[derive(Clone, Debug, PartialEq)]
pub struct TreemapRectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub process: ProcessMemoryInfo,
    pub is_highlighted: bool,
}

impl TreemapRectangle {
    pub fn new(x: f64, y: f64, width: f64, height: f64, process: ProcessMemoryInfo) -> Self {
        Self {
            x,
            y,
            width,
            height,
            process,
            is_highlighted: true,
        }
    }

    pub fn aspect_ratio(&self) -> f64 {
        if self.width <= 0.0 || self.height <= 0.0 {
            f64::INFINITY
        } else {
            (self.width / self.height).max(self.height / self.width)
        }
    }
}

#[derive(Clone, Debug)]
struct LayoutItem {
    process: ProcessMemoryInfo,
    area: f64,
}

/// Squarified Treemap algorithm (Bruls, Huizing, van Wijk).
/// Organizes weighted process nodes into roughly-square rectangular blocks,
/// avoiding long, narrow slivers.
pub struct SquarifiedTreemap;

impl SquarifiedTreemap {
    pub fn calculate_layout(
        weighted_items: &[(ProcessMemoryInfo, f64)],
        width: f64,
        height: f64,
        padding: f64,
    ) -> Vec<TreemapRectangle> {
        let mut results: Vec<TreemapRectangle> = Vec::new();

        if weighted_items.is_empty() || width <= 1.0 || height <= 1.0 {
            return results;
        }

        let total_weight: f64 = weighted_items.iter().map(|(_, weight)| *weight).sum();
        if total_weight <= 0.0 {
            return results;
        }

        let total_area = width * height;

        // Convert weights to geometric areas
        let mut items: Vec<LayoutItem> = Vec::with_capacity(weighted_items.len());
        for (process, weight) in weighted_items {
            let area = (weight / total_weight) * total_area;
            if area > 0.0 {
                items.push(LayoutItem {
                    process: process.clone(),
                    area,
                });
            }
        }

        if items.is_empty() {
            return results;
        }

        // Current available bounding rectangle
        let mut cur_x = 0.0;
        let mut cur_y = 0.0;
        let mut cur_w = width;
        let mut cur_h = height;

        let mut current_row: Vec<LayoutItem> = Vec::new();

        for item in items {
            let short_edge = cur_w.min(cur_h);
            if short_edge <= 0.001 {
                break;
            }

            if current_row.is_empty() {
                current_row.push(item);
                continue;
            }

            let current_worst = worst_aspect_ratio(&current_row, short_edge);
            let mut test_row = current_row.clone();
            test_row.push(item.clone());
            let test_worst = worst_aspect_ratio(&test_row, short_edge);

            if test_worst <= current_worst {
                // Adding item improves or maintains aspect ratio
                current_row.push(item);
            } else {
                // Adding item worsens aspect ratio: layout current row and start fresh row
                layout_row(
                    &current_row,
                    &mut cur_x,
                    &mut cur_y,
                    &mut cur_w,
                    &mut cur_h,
                    &mut results,
                    padding,
                );
                current_row.clear();
                current_row.push(item);
            }
        }

        // Layout any remaining items in the final row
        if !current_row.is_empty() {
            layout_row(
                &current_row,
                &mut cur_x,
                &mut cur_y,
                &mut cur_w,
                &mut cur_h,
                &mut results,
                padding,
            );
        }

        results
    }
}

fn worst_aspect_ratio(row: &[LayoutItem], side_length: f64) -> f64 {
    if row.is_empty() || side_length <= 0.0 {
        return f64::INFINITY;
    }

    let sum_area: f64 = row.iter().map(|x| x.area).sum();
    if sum_area <= 0.0 {
        return f64::INFINITY;
    }

    let max_area = row
        .iter()
        .map(|x| x.area)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_area = row.iter().map(|x| x.area).fold(f64::INFINITY, f64::min);

    let side_sq = side_length * side_length;
    let sum_area_sq = sum_area * sum_area;

    let term1 = (side_sq * max_area) / sum_area_sq;
    let term2 = sum_area_sq / (side_sq * min_area);

    term1.max(term2)
}

fn layout_row(
    row: &[LayoutItem],
    cur_x: &mut f64,
    cur_y: &mut f64,
    cur_w: &mut f64,
    cur_h: &mut f64,
    results: &mut Vec<TreemapRectangle>,
    padding: f64,
) {
    if row.is_empty() {
        return;
    }

    let sum_area: f64 = row.iter().map(|x| x.area).sum();
    if sum_area <= 0.0 {
        return;
    }

    let is_horizontal = *cur_w >= *cur_h;
    let short_edge = if is_horizontal { *cur_h } else { *cur_w };

    // Thickness of this row strip
    let strip_thickness = sum_area / short_edge;

    if is_horizontal {
        // Vertical strip dividing width
        let row_x = *cur_x;
        let mut item_y = *cur_y;

        for item in row {
            let item_h = item.area / strip_thickness;
            let render_w = (strip_thickness - padding).max(0.0);
            let render_h = (item_h - padding).max(0.0);

            results.push(TreemapRectangle::new(
                row_x + (padding / 2.0),
                item_y + (padding / 2.0),
                render_w,
                render_h,
                item.process.clone(),
            ));

            item_y += item_h;
        }

        *cur_x += strip_thickness;
        *cur_w = (*cur_w - strip_thickness).max(0.0);
    } else {
        // Horizontal strip dividing height
        let row_y = *cur_y;
        let mut item_x = *cur_x;

        for item in row {
            let item_w = item.area / strip_thickness;
            let render_w = (item_w - padding).max(0.0);
            let render_h = (strip_thickness - padding).max(0.0);

            results.push(TreemapRectangle::new(
                item_x + (padding / 2.0),
                row_y + (padding / 2.0),
                render_w,
                render_h,
                item.process.clone(),
            ));

            item_x += item_w;
        }

        *cur_y += strip_thickness;
        *cur_h = (*cur_h - strip_thickness).max(0.0);
    }
}
