use egui::{Align2, Color32, FontId, RichText, Sense, Ui, Vec2};

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

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(16.0);

        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.vertical(|ui| {
                egui::Grid::new("settings_grid")
                    .num_columns(2)
                    .spacing([64.0, 32.0])
                    .show(ui, |ui| {
                        // Theme Mode
                        ui.label(
                            RichText::new("Theme Mode")
                                .font(FontId::proportional(16.0))
                                .color(colors.text),
                        );
                        ui.horizontal_wrapped(|ui| {
                            let themes = [
                                (tc_config::Theme::Light, "Light", Color32::from_rgb(0xF4, 0xF5, 0xF7), Color32::from_rgb(0x35, 0xC8, 0xE1)),
                                (tc_config::Theme::Dark, "Dark", Color32::from_rgb(0x0A, 0x11, 0x1E), Color32::from_rgb(0x35, 0xC8, 0xE1)),
                                (tc_config::Theme::Ocean, "Ocean", Color32::from_rgb(0x06, 0x11, 0x1E), Color32::from_rgb(0x00, 0xE5, 0xFF)),
                                (tc_config::Theme::Forest, "Forest", Color32::from_rgb(0x09, 0x15, 0x0E), Color32::from_rgb(0x34, 0xD3, 0x99)),
                                (tc_config::Theme::Sunset, "Sunset", Color32::from_rgb(0x19, 0x0B, 0x08), Color32::from_rgb(0xFB, 0x92, 0x3C)),
                                (tc_config::Theme::Berry, "Berry", Color32::from_rgb(0x15, 0x08, 0x1B), Color32::from_rgb(0xE8, 0x43, 0x93)),
                                (tc_config::Theme::Midnight, "Midnight", Color32::from_rgb(0x00, 0x00, 0x00), Color32::from_rgb(0x3B, 0x82, 0xF6)),
                                (tc_config::Theme::Rose, "Rose", Color32::from_rgb(0x17, 0x09, 0x0A), Color32::from_rgb(0xF4, 0x3F, 0x5E)),
                                (tc_config::Theme::Coffee, "Coffee", Color32::from_rgb(0x14, 0x10, 0x0C), Color32::from_rgb(0xD9, 0x77, 0x06)),
                                (tc_config::Theme::Mint, "Mint", Color32::from_rgb(0x07, 0x15, 0x16), Color32::from_rgb(0x10, 0xB9, 0x81)),
                            ];

                            for (theme_enum, name, bg_color, accent_color) in themes {
                                let is_selected = app.theme == theme_enum;
                                let circle_size = 28.0;
                                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(circle_size), Sense::click());
                                
                                if resp.hovered() {
                                    ui.painter().circle_filled(rect.center(), circle_size / 2.0 + 2.0, colors.hover);
                                }
                                
                                if is_selected {
                                    ui.painter().circle_stroke(rect.center(), circle_size / 2.0 + 3.0, egui::Stroke::new(2.0, colors.text));
                                }

                                ui.painter().circle_filled(rect.center(), circle_size / 2.0, bg_color);
                                ui.painter().circle_stroke(rect.center(), circle_size / 2.0, egui::Stroke::new(1.0, colors.border));
                                ui.painter().circle_filled(rect.center(), circle_size / 4.0, accent_color);
                                
                                resp.on_hover_text(name);

                                if resp.clicked() && app.theme != theme_enum {
                                    app.theme = theme_enum;
                                    app.colors_cache = None;
                                    app.ctx.config.write(|c| {
                                        c.ui.theme = theme_enum;
                                    });
                                }
                                ui.add_space(8.0);
                            }
                        });
                        ui.end_row();

                        // Library
                        ui.label(
                            RichText::new("Library")
                                .font(FontId::proportional(16.0))
                                .color(colors.text),
                        );
                        let add_btn_w = 160.0;
                        let add_btn_h = 40.0;
                        let (add_rect, add_resp) = ui.allocate_exact_size(
                            Vec2::new(add_btn_w, add_btn_h),
                            Sense::click(),
                        );
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
                        ui.end_row();

                        // Audio Output Backend
                        ui.label(
                            RichText::new("Audio Output")
                                .font(FontId::proportional(16.0))
                                .color(colors.text),
                        );
                        let mut current_backend =
                            app.ctx.config.read(|c| c.engine.output_backend).unwrap();
                        let prev_backend = current_backend;

                        egui::ComboBox::from_id_salt("audio_backend_combo")
                            .selected_text(match current_backend {
                                tc_config::types::enums::AudioBackend::Auto => "Auto (Shared)",
                                tc_config::types::enums::AudioBackend::ExclusiveAlsa => {
                                    "Exclusive (ALSA)"
                                },
                                tc_config::types::enums::AudioBackend::ExclusiveAsio => {
                                    "Exclusive (ASIO)"
                                },
                                tc_config::types::enums::AudioBackend::ExclusiveCoreAudioHog => {
                                    "Exclusive (CoreAudio Hog)"
                                },
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut current_backend,
                                    tc_config::types::enums::AudioBackend::Auto,
                                    "Auto (Shared)",
                                );

                                #[cfg(target_os = "linux")]
                                ui.selectable_value(
                                    &mut current_backend,
                                    tc_config::types::enums::AudioBackend::ExclusiveAlsa,
                                    "Exclusive (ALSA)",
                                );

                                #[cfg(target_os = "windows")]
                                ui.selectable_value(
                                    &mut current_backend,
                                    tc_config::types::enums::AudioBackend::ExclusiveAsio,
                                    "Exclusive (ASIO)",
                                );

                                #[cfg(target_os = "macos")]
                                ui.selectable_value(
                                    &mut current_backend,
                                    tc_config::types::enums::AudioBackend::ExclusiveCoreAudioHog,
                                    "Exclusive (CoreAudio Hog)",
                                );
                            });

                        if current_backend != prev_backend {
                            app.ctx.config.write(|c| {
                                c.engine.output_backend = current_backend;
                            });
                            log::info!("Audio backend changed to {:?}", current_backend);
                        }
                        ui.end_row();
                    });
            });
        });
    });
}
