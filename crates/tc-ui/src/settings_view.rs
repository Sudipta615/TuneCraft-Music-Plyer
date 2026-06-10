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

                ui.add_space(32.0);

                // Audio Output Backend
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Audio Output")
                            .font(FontId::proportional(16.0))
                            .color(colors.text),
                    );
                    ui.add_space(16.0);

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
                        // Output change requires engine restart (handled automatically by the engine watcher if implemented, or requires app restart)
                        log::info!("Audio backend changed to {:?}", current_backend);
                    }
                });
            });
        });
    });
}
