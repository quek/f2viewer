use std::collections::HashSet;
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
    files.sort();
    files
}

/// Pick a random image, preferring ones not displayed yet in the current cycle.
/// Once every file has been shown, `shown` is cleared and a new cycle starts.
/// The current image is avoided if possible.
pub fn pick_random_image(
    files: &[PathBuf],
    current: Option<&Path>,
    shown: &mut HashSet<PathBuf>,
) -> Option<PathBuf> {
    if files.is_empty() {
        return None;
    }

    let mut candidates: Vec<&PathBuf> = files
        .iter()
        .filter(|p| !shown.contains(p.as_path()) && current.is_none_or(|c| c != p.as_path()))
        .collect();

    if candidates.is_empty() {
        // Everything has been shown: start a new cycle
        shown.clear();
        candidates = files
            .iter()
            .filter(|p| current.is_none_or(|c| c != p.as_path()))
            .collect();
        // Only the current image is available
        if candidates.is_empty() {
            candidates = files.iter().collect();
        }
    }

    let mut rng = rand::thread_rng();
    candidates.choose(&mut rng).map(|p| (*p).clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn covers_every_image_before_repeating() {
        let files = files(&["a", "b", "c", "d"]);
        let mut shown = HashSet::new();
        let mut current: Option<PathBuf> = None;

        // One full cycle must hit each file exactly once
        let mut picked = Vec::new();
        for _ in 0..files.len() {
            let p = pick_random_image(&files, current.as_deref(), &mut shown).unwrap();
            shown.insert(p.clone());
            picked.push(p.clone());
            current = Some(p);
        }
        picked.sort();
        assert_eq!(picked, files);

        // Cycle exhausted: the next pick resets `shown` and avoids the current image
        let next = pick_random_image(&files, current.as_deref(), &mut shown).unwrap();
        assert_ne!(Some(&next), current.as_ref());
        assert!(shown.is_empty(), "cycle should have been reset");
    }

    #[test]
    fn new_files_are_preferred_over_shown_ones() {
        let mut files = files(&["a", "b"]);
        let mut shown: HashSet<PathBuf> = files.iter().cloned().collect();

        // A file added by a rescan is unshown, so it wins over the two seen ones
        files.push(PathBuf::from("c"));
        let picked = pick_random_image(&files, Some(Path::new("a")), &mut shown).unwrap();
        assert_eq!(picked, PathBuf::from("c"));
    }

    #[test]
    fn single_file_is_returned_even_when_current() {
        let files = files(&["only"]);
        let mut shown: HashSet<PathBuf> = files.iter().cloned().collect();
        let picked = pick_random_image(&files, Some(Path::new("only")), &mut shown);
        assert_eq!(picked, Some(PathBuf::from("only")));
    }

    #[test]
    fn empty_list_yields_nothing() {
        let mut shown = HashSet::new();
        assert_eq!(pick_random_image(&[], None, &mut shown), None);
    }
}
