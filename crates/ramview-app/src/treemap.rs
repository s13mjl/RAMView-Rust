use std::collections::HashMap;

use ramview_core::algorithms::TreemapRectangle;
use ramview_core::models::ProcessMemoryInfo;

const ANIMATION_DURATION: f64 = 0.280;

#[derive(Clone, Copy, Debug)]
pub struct RectF {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl RectF {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

fn lerp(from: RectF, to: RectF, t: f32) -> RectF {
    RectF {
        left: from.left + (to.left - from.left) * t,
        top: from.top + (to.top - from.top) * t,
        right: from.right + (to.right - from.right) * t,
        bottom: from.bottom + (to.bottom - from.bottom) * t,
    }
}

pub struct AnimatedBox {
    pub key: String,
    pub item: TreemapRectangle,
    pub current: RectF,
    start_rect: RectF,
    target_rect: RectF,
    pub opacity: f32,
    start_opacity: f32,
    target_opacity: f32,
    anim_start: f64,
    removing: bool,
}

impl AnimatedBox {
    fn progress(&self, now: f64) -> f32 {
        (((now - self.anim_start) / ANIMATION_DURATION).clamp(0.0, 1.0)) as f32
    }
}

/// Animated treemap boxes in DIP coordinates (§9): 280ms quadratic ease-out,
/// geometry interpolation, fade-in for new processes and fade-out on exit.
#[derive(Default)]
pub struct AnimatedTreemap {
    pub boxes: Vec<AnimatedBox>,
    index: HashMap<String, usize>,
}

impl AnimatedTreemap {
    pub fn new() -> Self {
        Self {
            boxes: Vec::new(),
            index: HashMap::new(),
        }
    }

    pub fn sync(&mut self, items: &[TreemapRectangle], now: f64) {
        let mut incoming: Vec<String> = Vec::with_capacity(items.len());

        for item in items {
            let key = key_for(&item.process);
            incoming.push(key.clone());

            let target = RectF::new(
                item.x as f32,
                item.y as f32,
                (item.x + item.width) as f32,
                (item.y + item.height) as f32,
            );
            let target_opacity = if item.is_highlighted { 1.0 } else { 0.22 };

            if let Some(&index) = self.index.get(&key) {
                let entry = &mut self.boxes[index];
                entry.item = item.clone();
                entry.start_rect = entry.current;
                entry.target_rect = target;
                entry.start_opacity = entry.opacity;
                entry.target_opacity = target_opacity;
                entry.anim_start = now;
                entry.removing = false;
            } else {
                self.index.insert(key.clone(), self.boxes.len());
                self.boxes.push(AnimatedBox {
                    key,
                    item: item.clone(),
                    current: target,
                    start_rect: target,
                    target_rect: target,
                    opacity: 0.0,
                    start_opacity: 0.0,
                    target_opacity,
                    anim_start: now,
                    removing: false,
                });
            }
        }

        for entry in &mut self.boxes {
            if entry.removing || incoming.contains(&entry.key) {
                continue;
            }
            entry.removing = true;
            entry.start_rect = entry.current;
            entry.target_rect = entry.current;
            entry.start_opacity = entry.opacity;
            entry.target_opacity = 0.0;
            entry.anim_start = now;
        }
    }

    /// Advances all animations; returns true while any box is still animating.
    pub fn update(&mut self, now: f64) -> bool {
        let mut animating = false;

        for entry in &mut self.boxes {
            let t = entry.progress(now);
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            entry.current = lerp(entry.start_rect, entry.target_rect, eased);
            entry.opacity =
                entry.start_opacity + (entry.target_opacity - entry.start_opacity) * eased;
            if t < 1.0 {
                animating = true;
            }
        }

        let before = self.boxes.len();
        self.boxes
            .retain(|entry| !(entry.removing && entry.progress(now) >= 1.0));
        if self.boxes.len() != before {
            self.reindex();
        }

        animating
    }

    fn reindex(&mut self) {
        self.index.clear();
        for (index, entry) in self.boxes.iter().enumerate() {
            self.index.insert(entry.key.clone(), index);
        }
    }
}

/// Matches AnimatedTreemapCanvas: name + group flag / PID.
pub fn key_for(process: &ProcessMemoryInfo) -> String {
    format!(
        "{}_{}",
        process.process_name,
        if process.instance_count > 1 {
            "group".to_string()
        } else {
            process.process_id.to_string()
        }
    )
}
