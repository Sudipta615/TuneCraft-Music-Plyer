use serde::{Deserialize, Serialize};

use super::enums::Theme;

/// UI configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "UiConfig::default_show_spectrum")]
    pub show_spectrum: bool,
    #[serde(default = "UiConfig::default_show_waveform")]
    pub show_waveform: bool,
    #[serde(default)]
    pub minimize_to_tray: bool,
}

impl UiConfig {
    fn default_show_spectrum() -> bool { true }
    fn default_show_waveform() -> bool { true }

    /// Validate UI config (currently no constraints to validate).
    pub fn validate(&mut self) -> Vec<String> {
        Vec::new()
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            show_spectrum: true,
            show_waveform: true,
            minimize_to_tray: false,
        }
    }
}

