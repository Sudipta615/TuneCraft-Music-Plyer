//! Toast notification system for TuneCraft.

/// Severity level for toast notifications.
#[derive(Clone, PartialEq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

use super::TuneCraftApp;

impl TuneCraftApp {
    pub fn push_toast(&mut self, message: impl Into<String>, level: ToastLevel) {
        self.push_toast_with_id(message, level, 0)
    }

    /// Push a toast with a unique identifier (v0.9.3: L-04 fix — unique layer IDs).
    pub fn push_toast_with_id(&mut self, message: impl Into<String>, level: ToastLevel, id: u64) {
        let expiry = std::time::Instant::now() + std::time::Duration::from_secs(4);
        self.toasts.push((message.into(), expiry, level, id));
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub fn draw_toasts(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        self.toasts.retain(|(_, expiry, _, _)| *expiry > now);

        let screen = ctx.content_rect();
        let mut y_offset = screen.bottom() - 80.0;

        for (msg, expiry, level, id) in self.toasts.iter().rev() {
            let remaining = expiry.duration_since(now).as_secs_f32();
            let alpha = (remaining.min(0.5) * 2.0).clamp(0.0, 1.0);

            let bg_color = match level {
                // passing to from_rgba_premultiplied, since that function expects
                // pre-multiplied color values (R*a, G*a, B*a, a).
                ToastLevel::Info => egui::Color32::from_rgba_premultiplied(
                    (50.0 * alpha) as u8,
                    (50.0 * alpha) as u8,
                    (60.0 * alpha) as u8,
                    (220.0 * alpha) as u8,
                ),
                ToastLevel::Success => egui::Color32::from_rgba_premultiplied(
                    (30.0 * alpha) as u8,
                    (80.0 * alpha) as u8,
                    (50.0 * alpha) as u8,
                    (220.0 * alpha) as u8,
                ),
                ToastLevel::Warning => egui::Color32::from_rgba_premultiplied(
                    (90.0 * alpha) as u8,
                    (70.0 * alpha) as u8,
                    (20.0 * alpha) as u8,
                    (220.0 * alpha) as u8,
                ),
                ToastLevel::Error => egui::Color32::from_rgba_premultiplied(
                    (90.0 * alpha) as u8,
                    (30.0 * alpha) as u8,
                    (30.0 * alpha) as u8,
                    (220.0 * alpha) as u8,
                ),
            };
            let text_alpha = (alpha * 220.0) as u8; // Bug #6 fix: consistent with bg alpha (220·α, not 255·α)

            let text_color = egui::Color32::from_rgba_premultiplied(
                (220.0 * alpha) as u8,
                (220.0 * alpha) as u8,
                (220.0 * alpha) as u8,
                text_alpha,
            );

            let toast_width = 280.0;
            let toast_height = 36.0;
            let toast_rect = egui::Rect::from_min_size(
                egui::Pos2::new(screen.right() - toast_width - 16.0, y_offset - toast_height),
                egui::Vec2::new(toast_width, toast_height),
            );

            let layer_id_bg = egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new(format!("toast_bg_{}", id)),
            );
            let layer_id_text = egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new(format!("toast_txt_{}", id)),
            );

            ctx.layer_painter(layer_id_bg)
                .rect_filled(toast_rect, 6.0, bg_color);
            ctx.layer_painter(layer_id_text).text(
                toast_rect.center(),
                egui::Align2::CENTER_CENTER,
                msg,
                egui::FontId::proportional(12.0),
                text_color,
            );

            y_offset -= toast_height + 6.0;
        }

        if !self.toasts.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
}
