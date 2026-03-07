use std::path::PathBuf;
use std::time::Instant;

use egui::TextureHandle;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ImagePane {
    pub directory: Option<PathBuf>,
    pub display_duration: f32,
    pub paused: bool,

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
}

impl Default for ImagePane {
    fn default() -> Self {
        Self {
            directory: None,
            display_duration: 5.0,
            paused: false,
            image_files: Vec::new(),
            current_image_path: None,
            texture: None,
            last_switch: None,
            needs_rescan: false,
        }
    }
}

impl ImagePane {
    /// Create a new pane that inherits directory and duration from another pane.
    pub fn inherit_from(other: &Self) -> Self {
        Self {
            directory: other.directory.clone(),
            display_duration: other.display_duration,
            paused: other.paused,
            image_files: other.image_files.clone(),
            current_image_path: None,
            texture: None,
            last_switch: None,
            needs_rescan: false,
        }
    }
}
