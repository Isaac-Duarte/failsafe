use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ScreenQualityPreset {
    #[default]
    Auto,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "360p")]
    P360,
}

impl ScreenQualityPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
            Self::P480 => "480p",
            Self::P360 => "360p",
        }
    }

    pub fn settings(self) -> ScreenSettings {
        match self {
            Self::Auto | Self::P720 => ScreenSettings {
                max_width: 1280,
                jpeg_quality: 60,
                target_fps: 15,
            },
            Self::P1080 => ScreenSettings {
                max_width: 1920,
                jpeg_quality: 75,
                target_fps: 20,
            },
            Self::P480 => ScreenSettings {
                max_width: 854,
                jpeg_quality: 50,
                target_fps: 12,
            },
            Self::P360 => ScreenSettings {
                max_width: 640,
                jpeg_quality: 40,
                target_fps: 10,
            },
        }
    }
}

impl FromStr for ScreenQualityPreset {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "1080p" => Ok(Self::P1080),
            "720p" => Ok(Self::P720),
            "480p" => Ok(Self::P480),
            "360p" => Ok(Self::P360),
            _ => Err(format!("unknown screen quality preset: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSettings {
    pub max_width: u32,
    pub jpeg_quality: u8,
    pub target_fps: u64,
}

impl Default for ScreenSettings {
    fn default() -> Self {
        ScreenQualityPreset::default().settings()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScreenControlMessage {
    SetQuality { preset: ScreenQualityPreset },
}

impl ScreenControlMessage {
    pub fn apply(self, settings: &mut ScreenSettings) {
        let Self::SetQuality { preset } = self;
        *settings = preset.settings();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_settings_match_plan() {
        assert_eq!(
            ScreenQualityPreset::P1080.settings(),
            ScreenSettings {
                max_width: 1920,
                jpeg_quality: 75,
                target_fps: 20,
            }
        );
        assert_eq!(
            ScreenQualityPreset::Auto.settings(),
            ScreenQualityPreset::P720.settings()
        );
    }

    #[test]
    fn preset_from_str() {
        assert_eq!("720p".parse(), Ok(ScreenQualityPreset::P720));
        assert!("4k".parse::<ScreenQualityPreset>().is_err());
    }
}
