use std::path::PathBuf;
use std::time::Instant;

use egui::TextureHandle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum DisplayMode {
    #[default]
    Random,
    Sequential,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct ImagePane {
    pub directory: Option<PathBuf>,
    pub display_duration: f32,
    pub paused: bool,
    pub display_mode: DisplayMode,

    #[serde(skip)]
    pub image_files: Vec<PathBuf>,
    #[serde(skip)]
    pub current_image_path: Option<PathBuf>,
    #[serde(skip)]
    pub texture: Option<TextureHandle>,
    #[serde(skip)]
    pub last_switch: Option<Instant>,
    #[serde(skip)]
    pub needs_rescan: bool,
    #[serde(skip)]
    pub last_scan: Option<Instant>,
    /// Current index for sequential mode.
    #[serde(skip)]
    pub seq_index: usize,
    /// History of displayed images (for back navigation in random mode).
    #[serde(skip)]
    pub history: Vec<PathBuf>,
    /// Current position in history (-1 based from end). 0 = at latest.
    #[serde(skip)]
    pub history_pos: usize,
}

impl Default for ImagePane {
    fn default() -> Self {
        Self {
            directory: None,
            display_duration: 5.0,
            paused: false,
            display_mode: DisplayMode::default(),
            image_files: Vec::new(),
            current_image_path: None,
            texture: None,
            last_switch: None,
            needs_rescan: false,
            last_scan: None,
            seq_index: 0,
            history: Vec::new(),
            history_pos: 0,
        }
    }
}

impl ImagePane {
    pub fn inherit_from(other: &Self) -> Self {
        Self {
            directory: other.directory.clone(),
            display_duration: other.display_duration,
            paused: other.paused,
            display_mode: other.display_mode,
            image_files: other.image_files.clone(),
            current_image_path: None,
            texture: None,
            last_switch: None,
            needs_rescan: false,
            last_scan: None,
            seq_index: 0,
            history: Vec::new(),
            history_pos: 0,
        }
    }

    pub fn clone_config(&self) -> Self {
        Self {
            directory: self.directory.clone(),
            display_duration: self.display_duration,
            paused: self.paused,
            display_mode: self.display_mode,
            ..Default::default()
        }
    }
}
