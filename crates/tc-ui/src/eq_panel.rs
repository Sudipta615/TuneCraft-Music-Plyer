//! EQ panel bridge — syncs EQ state to the Slint EqPanel component.

use slint::{ModelRc, SharedString};

use crate::{app::TuneCraftApp, converters::eq_band_to_item, App};

const EQ_FREQUENCIES: [&str; 10] = [
    "32Hz", "64Hz", "125Hz", "250Hz", "500Hz", "1kHz", "2kHz", "4kHz", "8kHz", "16kHz",
];

const EQ_PRESETS: [&str; 6] = [
    "Custom",
    "Flat",
    "Bass Boost",
    "Treble Boost",
    "V-Shape",
    "Vocal",
];

/// Build the EQ bands model.
pub fn build_eq_bands(app: &TuneCraftApp) -> ModelRc<crate::EqBandItem> {
    let items: Vec<crate::EqBandItem> = app
        .eq_bands
        .iter()
        .enumerate()
        .map(|(i, &gain)| eq_band_to_item(i as i32, EQ_FREQUENCIES[i], gain))
        .collect();
    ModelRc::new(slint::VecModel::from(items))
}

/// Build the EQ presets model (as plain strings for ComboBox).
pub fn build_eq_presets() -> ModelRc<SharedString> {
    let items: Vec<SharedString> = EQ_PRESETS.iter().map(|s| SharedString::from(*s)).collect();
    ModelRc::new(slint::VecModel::from(items))
}

/// Sync EQ panel state to Slint.
pub fn sync_eq_panel(app: &TuneCraftApp, slint_app: &App) {
    slint_app.set_eq_enabled(app.eq_enabled);
    slint_app.set_eq_bands(build_eq_bands(app));
    slint_app.set_eq_presets(build_eq_presets());
    slint_app.set_eq_active_preset(SharedString::from(app.eq_preset.clone()));
    slint_app.set_eq_preamp_db(app.eq_preamp);
    slint_app.set_eq_bass_shelf(app.eq_bass_shelf);
    slint_app.set_eq_treble_shelf(app.eq_treble_shelf);
    slint_app.set_eq_stereo_width(app.eq_stereo_width);
    slint_app.set_eq_balance(app.eq_balance);
    slint_app.set_eq_dither_enabled(app.eq_dither);
    slint_app.set_eq_midside_enabled(app.eq_midside);
}
