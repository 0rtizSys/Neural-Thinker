//! Tag colors: grouping nodes by their first tag, picking each tag's color
//! from the theme, and the legend that highlights one tag's notes.

use std::collections::HashMap;

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};

use super::{GraphView, LEGEND_ROWS, NO_TAG};
use crate::tags;
use crate::theme::{GraphColors, TAG_SLOTS};

impl GraphView {
    /// Groups the nodes by first tag and gives each tag an automatic color slot.
    pub(super) fn index_tags(&mut self) {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for node in &self.graph.nodes {
            if let Some(tag) = node.tags.first() {
                *counts.entry(tag).or_default() += 1;
            }
        }
        let mut sorted: Vec<(&str, usize)> = counts.into_iter().collect();
        sorted.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        // Each tag prefers the slot its name hashes to, so it keeps its color
        // as the vault changes; the most used tags take distinct slots first.
        let mut used = [false; TAG_SLOTS];
        self.tag_slot = sorted
            .iter()
            .enumerate()
            .map(|(rank, (name, _))| {
                let home = (fnv1a(name) % TAG_SLOTS as u64) as usize;
                if rank >= TAG_SLOTS {
                    return home as u8;
                }
                let slot = (0..TAG_SLOTS)
                    .map(|k| (home + k) % TAG_SLOTS)
                    .find(|&s| !used[s])
                    .unwrap_or(home);
                used[slot] = true;
                slot as u8
            })
            .collect();
        let index: HashMap<&str, u32> = sorted
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (*name, i as u32))
            .collect();
        self.node_tag = self
            .graph
            .nodes
            .iter()
            .map(|n| n.tags.first().map_or(NO_TAG, |t| index[t.as_str()]))
            .collect();
        self.tag_counts = sorted.iter().map(|(_, c)| *c).collect();
        self.tag_names = sorted.into_iter().map(|(n, _)| n.to_owned()).collect();
        let filter = self.tag_filter.take();
        self.set_tag_filter(filter);
    }

    /// Highlights the notes tagged `tag` (or a tag below it), or clears it.
    pub(super) fn set_tag_filter(&mut self, tag: Option<String>) {
        self.tag_mask = match &tag {
            Some(tag) => self
                .graph
                .nodes
                .iter()
                .map(|n| {
                    n.tags
                        .iter()
                        .any(|t| t == tag || tags::parents(t).any(|p| p == tag))
                })
                .collect(),
            None => Vec::new(),
        };
        self.tag_filter = tag.filter(|_| self.tag_mask.iter().any(|&m| m));
        if self.tag_filter.is_none() {
            self.tag_mask.clear();
        }
    }

    /// Color of every distinct tag under the current theme.
    pub(super) fn tag_colors(&self, colors: &GraphColors) -> Vec<egui::Color32> {
        self.tag_names
            .iter()
            .zip(&self.tag_slot)
            .map(|(name, &slot)| {
                std::iter::once(name.as_str())
                    .chain(tags::parents(name))
                    .find_map(|t| colors.tags.get(t).copied())
                    .unwrap_or(colors.tag_slots[slot as usize])
            })
            .collect()
    }

    /// The tag legend in the bottom-left corner. Clicking a tag highlights its
    /// notes; clicking it again (or Esc) clears that.
    pub(super) fn legend(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        tag_colors: &[egui::Color32],
        colors: &GraphColors,
    ) {
        if self.tag_names.is_empty() {
            return;
        }
        let rows = self.tag_names.len().min(LEGEND_ROWS);
        let more = self.tag_names.len() - rows;
        let font = egui::FontId::proportional(12.0);
        let painter = ui.painter_at(rect);
        let row_h = 19.0;
        let pad = 8.0;
        let dot = 4.5;
        let names: Vec<_> = self.tag_names[..rows]
            .iter()
            .map(|name| {
                let mut label: String = name.chars().take(28).collect();
                if label.len() < name.len() {
                    label.push('…');
                }
                painter.layout_no_wrap(
                    format!("#{label}"),
                    font.clone(),
                    egui::Color32::PLACEHOLDER,
                )
            })
            .collect();
        let counts: Vec<_> = self.tag_counts[..rows]
            .iter()
            .map(|c| {
                painter.layout_no_wrap(c.to_string(), font.clone(), egui::Color32::PLACEHOLDER)
            })
            .collect();
        let more_text = (more > 0).then(|| {
            painter.layout_no_wrap(
                format!("+{more} more"),
                font.clone(),
                egui::Color32::PLACEHOLDER,
            )
        });
        let name_w = names.iter().map(|g| g.size().x).fold(0.0, f32::max);
        let count_w = counts.iter().map(|g| g.size().x).fold(0.0, f32::max);
        let width = (pad * 2.0 + dot * 2.0 + 8.0 + name_w + 14.0 + count_w)
            .max(more_text.as_ref().map_or(0.0, |g| g.size().x + pad * 2.0));
        let height = pad * 2.0 + row_h * (rows + more_text.is_some() as usize) as f32;
        let frame = Rect::from_min_size(
            rect.left_bottom() + Vec2::new(10.0, -10.0 - height),
            Vec2::new(width, height),
        );
        if !rect.contains_rect(frame) {
            return;
        }
        painter.rect(
            frame,
            6.0,
            colors.background.gamma_multiply(0.9),
            Stroke::new(1.0, colors.edge),
            egui::StrokeKind::Inside,
        );

        let mut toggled = None;
        for (i, (name, count)) in names.into_iter().zip(counts).enumerate() {
            let row = Rect::from_min_size(
                frame.min + Vec2::new(pad / 2.0, pad + row_h * i as f32),
                Vec2::new(width - pad, row_h),
            );
            let response = ui.interact(row, ui.id().with(("nt_graph_tag", i)), Sense::click());
            let picked = self.tag_filter.as_deref() == Some(self.tag_names[i].as_str());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                painter.rect_filled(row, 4.0, colors.edge.gamma_multiply(0.6));
            }
            if response.clicked() {
                toggled = Some(i);
            }
            let strength = if self.tag_filter.is_none() || picked {
                1.0
            } else {
                0.5
            };
            let center = Pos2::new(row.min.x + pad / 2.0 + dot, row.center().y);
            painter.circle_filled(center, dot, tag_colors[i].gamma_multiply(strength));
            if picked {
                painter.circle_stroke(center, dot + 2.5, Stroke::new(1.2, colors.highlight));
            }
            let text_pos = Pos2::new(center.x + dot + 8.0, row.center().y - name.size().y / 2.0);
            painter.galley(text_pos, name, colors.label.gamma_multiply(0.9 * strength));
            let count_pos = Pos2::new(
                row.max.x - pad / 2.0 - count.size().x,
                row.center().y - count.size().y / 2.0,
            );
            painter.galley(
                count_pos,
                count,
                colors.label.gamma_multiply(0.5 * strength),
            );
        }
        if let Some(text) = more_text {
            let pos = Pos2::new(
                frame.min.x + pad,
                frame.max.y - pad - row_h / 2.0 - text.size().y / 2.0,
            );
            painter.galley(pos, text, colors.label.gamma_multiply(0.45));
        }
        if let Some(i) = toggled {
            let tag = self.tag_names[i].clone();
            let next = (self.tag_filter.as_deref() != Some(tag.as_str())).then_some(tag);
            self.set_tag_filter(next);
        }
    }
}

/// 64-bit FNV-1a: a small, stable string hash (unlike `DefaultHasher`, it
/// never changes between builds, so tag colors do not either).
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}
