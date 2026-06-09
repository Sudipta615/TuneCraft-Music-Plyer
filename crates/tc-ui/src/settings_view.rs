use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};

use crate::app::TuneCraftApp;

pub fn draw(app: &mut TuneCraftApp, ui: &mut Ui) {
    let colors = app.colors();

    ui.vertical(|ui| {
        ui.add_space(32.0);

        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(
                RichText::new("Settings")
                    .font(FontId::proportional(28.0))
                    .color(colors.text)
                    .strong(),
            );
        });

        ui.add_space(32.0);

        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.vertical(|ui| {
                // Theme Toggle
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Theme Mode")
                            .font(FontId::proportional(16.0))
                            .color(colors.text),
                    );
                    ui.add_space(16.0);

                    let theme_icon = if app.dark_mode {
                        egui_phosphor::regular::SUN
                    } else {
                        egui_phosphor::regular::MOON
                    };

                    if ui
                        .add(egui::Button::new(
                            RichText::new(theme_icon)
                                .font(FontId::proportional(20.0))
                                .color(colors.text_dim),
                        ))
                        .clicked()
                    {
                        app.dark_mode = !app.dark_mode;
                        app.colors_cache = None;
                        let new_theme = if app.dark_mode {
                            tc_config::Theme::Dark
                        } else {
                            tc_config::Theme::Light
                        };
                        app.ctx.config.write(|c| {
                            c.ui.theme = new_theme;
                        });
                    }
                });

                ui.add_space(32.0);

                // Add Music Button
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Library")
                            .font(FontId::proportional(16.0))
                            .color(colors.text),
                    );
                    ui.add_space(16.0);

                    let add_btn_w = 160.0;
                    let add_btn_h = 40.0;
                    let (add_rect, add_resp) =
                        ui.allocate_exact_size(Vec2::new(add_btn_w, add_btn_h), Sense::click());
                    let add_bg = if add_resp.hovered() {
                        colors.accent_dark
                    } else {
                        colors.accent
                    };
                    ui.painter().rect_filled(add_rect, 8.0, add_bg);
                    ui.painter().text(
                        add_rect.center(),
                        Align2::CENTER_CENTER,
                        "+ Add Music Folder",
                        FontId::proportional(14.0),
                        Color32::WHITE,
                    );
                    if add_resp.clicked() {
                        app.show_add_music_dialog = true;
                    }
                });
            });
        });
    });
}
