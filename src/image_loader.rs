use std::path::{Path, PathBuf};

use egui::TextureHandle;
use rand::seq::SliceRandom;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp"];

/// Recursively scan a directory for image files.
/// Validates that resolved paths stay within the base directory (path traversal prevention).
pub fn scan_directory(dir: &Path) -> Vec<PathBuf> {
    let Ok(base) = dir.canonicalize() else {
        log::warn!("Cannot canonicalize directory: {:?}", dir);
        return Vec::new();
    };

    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(&base)
        .follow_links(false) // Don't follow symlinks for security
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Path traversal check
        if let Ok(canonical) = path.canonicalize() {
            if !canonical.starts_with(&base) {
                continue;
            }
        } else {
            continue;
        }

        if !path.is_file() {
            continue;
        }

        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                files.push(path.to_path_buf());
            }
        }
    }
    files
}

/// Pick a random image from the list, avoiding the current one if possible.
pub fn pick_random_image(files: &[PathBuf], current: Option<&Path>) -> Option<PathBuf> {
    if files.is_empty() {
        return None;
    }
    if files.len() == 1 {
        return Some(files[0].clone());
    }

    let mut rng = rand::thread_rng();
    for _ in 0..10 {
        if let Some(path) = files.choose(&mut rng) {
            if current.is_none_or(|c| c != path) {
                return Some(path.clone());
            }
        }
    }
    // Fallback: just pick any
    files.choose(&mut rng).cloned()
}

/// Load an image file and create an egui texture.
pub fn load_texture(ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
    let img = match image::open(path) {
        Ok(img) => img,
        Err(e) => {
            log::warn!("Failed to load image {:?}: {}", path, e);
            return None;
        }
    };

    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);

    let name = path.file_name().unwrap_or_default().to_string_lossy();
    Some(ctx.load_texture(
        name,
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}
