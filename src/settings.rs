use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::color::{MIN_KELVIN, NEUTRAL_KELVIN};

/// How the warm tint reaches the screen. Each one has different reach, so the user picks.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    /// macOS Night Shift, held on around the clock. Apple's color handling, but stops at 2700 K. The macOS default.
    NightShift,
    /// Display gamma tables, the way f.lux does it. Reaches 1200 K, can dim below minimum brightness.
    /// The only engine on Windows.
    Gamma,
    /// macOS Accessibility color filter (Color Tint). Applied at the very end of the pipeline.
    ColorFilter,
}

impl EngineKind {
    pub const fn available() -> &'static [EngineKind] {
        if cfg!(target_os = "macos") {
            &[EngineKind::NightShift, EngineKind::Gamma, EngineKind::ColorFilter]
        } else {
            &[EngineKind::Gamma]
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            EngineKind::NightShift => "Night Shift (most natural, 2700 K min)",
            EngineKind::Gamma => "Gamma (warmest, can dim)",
            EngineKind::ColorFilter => "Color Filter (Accessibility tint)",
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            EngineKind::NightShift => "night_shift",
            EngineKind::Gamma => "gamma",
            EngineKind::ColorFilter => "color_filter",
        }
    }

    pub fn from_id(id: &str) -> Option<EngineKind> {
        Self::available().iter().copied().find(|k| k.id() == id)
    }
}

pub const MIN_BRIGHTNESS: u8 = 20;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub kelvin: u16,
    /// Percent. Below 100 dims the screen further than the hardware minimum (Gamma engine only).
    pub brightness: u8,
    pub engine: EngineKind,
    pub launch_at_login: bool,
    pub keep_awake: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            kelvin: 2700,
            brightness: 100,
            engine: EngineKind::available()[0],
            launch_at_login: false,
            keep_awake: false,
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        let parsed = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok());
        parsed.unwrap_or_default().sanitized()
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// The file is user-editable, so anything out of range or unsupported on this OS falls back to a valid value.
    fn sanitized(mut self) -> Settings {
        self.kelvin = self.kelvin.clamp(MIN_KELVIN, NEUTRAL_KELVIN);
        self.brightness = self.brightness.clamp(MIN_BRIGHTNESS, 100);
        if !EngineKind::available().contains(&self.engine) {
            self.engine = EngineKind::available()[0];
        }
        self
    }
}

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Dozy").join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_edited_file_is_clamped() {
        let s: Settings = serde_json::from_str(r#"{"kelvin": 50, "brightness": 0}"#).unwrap();
        let s = s.sanitized();
        assert_eq!((s.kelvin, s.brightness, s.enabled), (MIN_KELVIN, MIN_BRIGHTNESS, true));
    }

    #[test]
    fn unknown_engine_is_rejected_by_parse_and_falls_back() {
        assert!(serde_json::from_str::<Settings>(r#"{"engine": "laser"}"#).is_err());
        assert_eq!(EngineKind::from_id("laser"), None);
    }

    #[test]
    fn roundtrip() {
        let s = Settings { kelvin: 1900, brightness: 55, engine: EngineKind::Gamma, keep_awake: true, ..Settings::default() };
        let back: Settings = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
