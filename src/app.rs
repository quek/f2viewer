use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::Color32;
use serde::{Deserialize, Serialize};

use crate::image_loader;

/// Copy a file to the clipboard as CF_HDROP so it can be pasted in Explorer etc.
fn copy_file_to_clipboard(path: &Path) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    const CF_HDROP: u32 = 15;
    const GMEM_MOVEABLE: u32 = 0x0002;
    const GMEM_ZEROINIT: u32 = 0x0040;

    #[repr(C)]
    struct DropFiles {
        p_files: u32,
        pt_x: i32,
        pt_y: i32,
        f_nc: i32,
        f_wide: i32,
    }

    extern "system" {
        fn OpenClipboard(hwnd: *mut std::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, data: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalAlloc(flags: u32, bytes: usize) -> *mut std::ffi::c_void;
        fn GlobalLock(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalUnlock(hmem: *mut std::ffi::c_void) -> i32;
        fn GlobalFree(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    // canonicalize to get absolute path, then strip \\?\ prefix for Explorer compatibility
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    let path_str = canonical.to_string_lossy();
    let clean = path_str.strip_prefix("\\\\?\\").unwrap_or(&path_str);
    let wide: Vec<u16> = OsStr::new(clean)
        .encode_wide()
        .chain(std::iter::once(0)) // null terminator
        .chain(std::iter::once(0)) // double null terminator
        .collect();

    unsafe {
        let header_size = std::mem::size_of::<DropFiles>();
        let total = header_size + wide.len() * std::mem::size_of::<u16>();

        let hmem = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total);
        if hmem.is_null() {
            return Err("GlobalAlloc failed".into());
        }

        let ptr = GlobalLock(hmem);
        if ptr.is_null() {
            GlobalFree(hmem);
            return Err("GlobalLock failed".into());
        }

        let df = ptr as *mut DropFiles;
        (*df).p_files = header_size as u32;
        (*df).f_wide = 1;

        let dest = (ptr as *mut u8).add(header_size) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), dest, wide.len());

        GlobalUnlock(hmem);

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            GlobalFree(hmem);
            return Err("OpenClipboard failed".into());
        }

        EmptyClipboard();
        if SetClipboardData(CF_HDROP, hmem).is_null() {
            GlobalFree(hmem);
            CloseClipboard();
            return Err("SetClipboardData failed".into());
        }

        CloseClipboard();
        Ok(())
    }
}
use crate::pane::{DisplayMode, ImagePane};
use crate::split_tree::{PaneId, SplitDirection, SplitTree};

/// State for the in-app delete confirmation dialog.
struct PendingDelete {
    pane_id: PaneId,
    path: PathBuf,
    pos: egui::Pos2,
    saved_paused: HashMap<PaneId, bool>,
}

/// State for fullscreen mode.
struct FullscreenState {
    pane_id: PaneId,
    saved_paused: HashMap<PaneId, bool>,
}
use crate::ui::controls::PaneAction;
use crate::ui::tree_ui;

/// Serializable state for persistence across restarts.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct SaveState {
    tree: SplitTree,
    panes: HashMap<PaneId, ImagePane>,
    next_pane_id: PaneId,
}

impl Default for SaveState {
    fn default() -> Self {
        let mut panes = HashMap::new();
        panes.insert(0, ImagePane::default());
        Self {
            tree: SplitTree::new_leaf(0),
            panes,
            next_pane_id: 1,
        }
    }
}

pub struct F2ViewerApp {
    tree: SplitTree,
    panes: HashMap<PaneId, ImagePane>,
    next_pane_id: PaneId,
    dirty: bool,
    pending_delete: Option<PendingDelete>,
    fullscreen: Option<FullscreenState>,
    copy_toast: Option<Instant>,
}

impl F2ViewerApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        Self::setup_fonts(&cc.egui_ctx);

        if let Some(state) = Self::load_state() {
            let mut app = Self {
                tree: state.tree,
                panes: state.panes,
                next_pane_id: state.next_pane_id,
                dirty: false,
                pending_delete: None,
                fullscreen: None,
                copy_toast: None,
            };
            // Rescan directories and load initial image for all restored panes
            for pane in app.panes.values_mut() {
                if let Some(ref dir) = pane.directory {
                    pane.image_files = image_loader::scan_directory(dir);
                    if !pane.image_files.is_empty() {
                        let next = image_loader::pick_random_image(&pane.image_files, None);
                        if let Some(path) = next {
                            pane.texture = image_loader::load_texture(&cc.egui_ctx, &path);
                            pane.current_image_path = Some(path.clone());
                            pane.last_switch = Some(Instant::now());
                            pane.history.push(path);
                        }
                    }
                }
            }
            app
        } else {
            let id = 0;
            let mut panes = HashMap::new();
            panes.insert(id, ImagePane::default());
            Self {
                tree: SplitTree::new_leaf(id),
                panes,
                next_pane_id: 1,
                dirty: false,
                pending_delete: None,
                fullscreen: None,
                copy_toast: None,
            }
        }
    }

    fn setup_fonts(ctx: &egui::Context) {
        let font_path = dirs::font_dir()
            .unwrap_or_default()
            .join("HackGen35ConsoleNF-Regular.ttf");
        let font_data = std::fs::read(&font_path).unwrap_or_else(|_| {
            let local = std::path::PathBuf::from(
                r"C:\Users\ancient\AppData\Local\Microsoft\Windows\Fonts\HackGen35ConsoleNF-Regular.ttf",
            );
            std::fs::read(&local).unwrap_or_default()
        });

        if font_data.is_empty() {
            log::warn!("HackGen font not found");
            return;
        }

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "HackGen".to_owned(),
            egui::FontData::from_owned(font_data).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "HackGen".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "HackGen".to_owned());
        ctx.set_fonts(fonts);
    }

    fn state_file_path() -> PathBuf {
        let dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("f2viewer");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("state.json")
    }

    fn load_state() -> Option<SaveState> {
        let path = Self::state_file_path();
        let data = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&data) {
            Ok(state) => {
                log::info!("Restored state from {:?}", path);
                Some(state)
            }
            Err(e) => {
                log::warn!("Failed to parse state file: {}", e);
                None
            }
        }
    }

    fn has_meaningful_state(&self) -> bool {
        self.panes.values().any(|p| p.directory.is_some()) || !self.tree.is_single_leaf()
    }

    fn save_state(&self) {
        // Don't overwrite saved state with empty defaults
        if !self.has_meaningful_state() {
            return;
        }
        let state = SaveState {
            tree: self.tree.clone(),
            panes: self.panes.iter().map(|(k, v)| (*k, v.clone_config())).collect(),
            next_pane_id: self.next_pane_id,
        };
        let path = Self::state_file_path();
        match serde_json::to_string_pretty(&state) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::warn!("Failed to save state: {}", e);
                }
            }
            Err(e) => log::warn!("Failed to serialize state: {}", e),
        }
    }

    fn alloc_id(&mut self) -> PaneId {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        id
    }

    fn update_timers(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let mut min_remaining = Duration::from_secs(60);
        let rescan_interval = Duration::from_secs(10);

        for pane in self.panes.values_mut() {
            // Periodic rescan: every 10 seconds, refresh the file list
            if pane.directory.is_some() {
                let should_rescan = pane.needs_rescan
                    || pane
                        .last_scan
                        .is_none_or(|t| now.duration_since(t) >= rescan_interval);
                if should_rescan {
                    if let Some(ref dir) = pane.directory {
                        let old_current = pane.current_image_path.clone();
                        pane.image_files = image_loader::scan_directory(dir);
                        // Adjust seq_index to match current image position after rescan
                        if let Some(ref current) = old_current {
                            if let Some(pos) = pane.image_files.iter().position(|f| f == current) {
                                pane.seq_index = pos + 1;
                            }
                        }
                    }
                    pane.last_scan = Some(now);
                    pane.needs_rescan = false;
                }
                // Keep repaint schedule aligned with next rescan
                if let Some(last_scan) = pane.last_scan {
                    let since_scan = now.duration_since(last_scan);
                    if since_scan < rescan_interval {
                        min_remaining = min_remaining.min(rescan_interval - since_scan);
                    }
                }
            }

            if pane.paused || pane.image_files.is_empty() {
                continue;
            }

            let duration = Duration::from_secs_f32(pane.display_duration);

            let should_switch = match pane.last_switch {
                None => true,
                Some(last) => now.duration_since(last) >= duration,
            };

            if should_switch {
                // Try loading with retry on failure (file may have been deleted externally)
                for _ in 0..5 {
                    let next = match pane.display_mode {
                        DisplayMode::Random => {
                            let current = pane.current_image_path.as_deref();
                            image_loader::pick_random_image(&pane.image_files, current)
                        }
                        DisplayMode::Sequential => {
                            if pane.image_files.is_empty() {
                                None
                            } else if pane.seq_index < pane.image_files.len() {
                                let path = pane.image_files[pane.seq_index].clone();
                                pane.seq_index += 1;
                                Some(path)
                            } else {
                                pane.paused = true;
                                None
                            }
                        }
                    };
                    let Some(path) = next else { break };
                    if let Some(tex) = image_loader::load_texture(ctx, &path) {
                        pane.texture = Some(tex);
                        pane.current_image_path = Some(path.clone());
                        pane.last_switch = Some(now);
                        if pane.history_pos > 0 {
                            let len = pane.history.len();
                            pane.history.truncate(len - pane.history_pos);
                            pane.history_pos = 0;
                        }
                        pane.history.push(path);
                        break;
                    } else {
                        // File unreadable (deleted?), remove from list and retry
                        pane.image_files.retain(|p| p != &path);
                    }
                }
            }

            if let Some(last) = pane.last_switch {
                let elapsed = now.duration_since(last);
                if elapsed < duration {
                    let remaining = duration - elapsed;
                    min_remaining = min_remaining.min(remaining);
                }
            }
        }

        if !self.panes.is_empty() {
            ctx.request_repaint_after(min_remaining);
        }
    }

    fn process_actions(&mut self, actions: Vec<PaneAction>, ctx: &egui::Context) {
        if actions.is_empty() {
            return;
        }
        for action in actions {
            match action {
                PaneAction::SplitVertical(pane_id) => {
                    self.split_pane(pane_id, SplitDirection::Vertical);
                }
                PaneAction::SplitHorizontal(pane_id) => {
                    self.split_pane(pane_id, SplitDirection::Horizontal);
                }
                PaneAction::Close(pane_id) => {
                    let removed = self.tree.unsplit(pane_id);
                    for id in removed {
                        self.panes.remove(&id);
                    }
                }
                PaneAction::SelectDirectory(pane_id) => {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        if let Some(pane) = self.panes.get_mut(&pane_id) {
                            pane.image_files = image_loader::scan_directory(&folder);
                            pane.directory = Some(folder);
                            pane.current_image_path = None;
                            pane.texture = None;
                            pane.last_switch = None;
                        }
                    }
                }
                PaneAction::TogglePause(pane_id) => {
                    if let Some(pane) = self.panes.get_mut(&pane_id) {
                        pane.paused = !pane.paused;
                    }
                }
                PaneAction::DeleteCurrentImage(pane_id) => {
                    self.delete_current_image(pane_id, ctx);
                }
                PaneAction::NavigateForward(pane_id) => {
                    self.navigate_image(pane_id, ctx, 1);
                }
                PaneAction::NavigateBackward(pane_id) => {
                    self.navigate_image(pane_id, ctx, -1);
                }
                PaneAction::Fullscreen(pane_id) => {
                    self.enter_fullscreen(pane_id);
                }
                PaneAction::CopyImage(pane_id) => {
                    self.copy_image_to_clipboard(pane_id);
                }
            }
        }
        self.dirty = true;
    }

    fn split_pane(&mut self, pane_id: PaneId, direction: SplitDirection) {
        let new_id_1 = self.alloc_id();
        let new_id_2 = self.alloc_id();

        let (pane1, pane2) = if let Some(original) = self.panes.get(&pane_id) {
            (
                ImagePane::inherit_from(original),
                ImagePane::inherit_from(original),
            )
        } else {
            (ImagePane::default(), ImagePane::default())
        };

        if self.tree.split(pane_id, direction, new_id_1, new_id_2) {
            self.panes.remove(&pane_id);
            self.panes.insert(new_id_1, pane1);
            self.panes.insert(new_id_2, pane2);
        }
    }

    fn navigate_image(&mut self, pane_id: PaneId, ctx: &egui::Context, delta: i32) {
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return;
        };
        if pane.image_files.is_empty() {
            return;
        }

        if delta < 0 {
            // Go back in history, skipping deleted files
            let mut pos = pane.history_pos;
            while pos + 1 < pane.history.len() {
                pos += 1;
                let idx = pane.history.len() - 1 - pos;
                if pane.history[idx].exists() {
                    pane.history_pos = pos;
                    let path = pane.history[idx].clone();
                    pane.texture = image_loader::load_texture(ctx, &path);
                    pane.current_image_path = Some(path);
                    pane.last_switch = Some(Instant::now());
                    break;
                }
            }
        } else {
            // Go forward in history, skipping deleted files
            if pane.history_pos > 0 {
                let mut pos = pane.history_pos;
                let mut found = false;
                while pos > 0 {
                    pos -= 1;
                    let idx = pane.history.len() - 1 - pos;
                    if pane.history[idx].exists() {
                        pane.history_pos = pos;
                        let path = pane.history[idx].clone();
                        pane.texture = image_loader::load_texture(ctx, &path);
                        pane.current_image_path = Some(path);
                        pane.last_switch = Some(Instant::now());
                        found = true;
                        break;
                    }
                }
                if !found {
                    // All forward history deleted, reset to latest
                    pane.history_pos = 0;
                }
            } else {
                // At latest: pick next image based on mode
                let next = match pane.display_mode {
                    DisplayMode::Random => {
                        let current = pane.current_image_path.as_deref();
                        image_loader::pick_random_image(&pane.image_files, current)
                    }
                    DisplayMode::Sequential => {
                        let current_index = pane
                            .current_image_path
                            .as_ref()
                            .and_then(|p| pane.image_files.iter().position(|f| f == p))
                            .unwrap_or(0);
                        let next_index = (current_index + 1) % pane.image_files.len();
                        Some(pane.image_files[next_index].clone())
                    }
                };
                if let Some(path) = next {
                    pane.texture = image_loader::load_texture(ctx, &path);
                    pane.current_image_path = Some(path.clone());
                    pane.last_switch = Some(Instant::now());
                    pane.history.push(path);
                }
            }
        }
    }

    fn delete_current_image(&mut self, pane_id: PaneId, ctx: &egui::Context) {
        let Some(pane) = self.panes.get(&pane_id) else {
            return;
        };
        let Some(path) = pane.current_image_path.clone() else {
            return;
        };

        let pos = ctx.input(|i| {
            i.pointer.hover_pos().unwrap_or(egui::pos2(100.0, 100.0))
        });
        let saved_paused = self.panes.iter().map(|(&id, p)| (id, p.paused)).collect();
        for pane in self.panes.values_mut() {
            pane.paused = true;
        }
        self.pending_delete = Some(PendingDelete {
            pane_id,
            path,
            pos,
            saved_paused,
        });
    }

    fn restore_paused(&mut self, saved_paused: &HashMap<PaneId, bool>) {
        let now = Instant::now();
        for (&id, pane) in self.panes.iter_mut() {
            if let Some(&was_paused) = saved_paused.get(&id) {
                pane.paused = was_paused;
                if !was_paused {
                    pane.last_switch = Some(now);
                }
            }
        }
    }

    fn confirm_delete(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_delete.take() else {
            return;
        };

        self.restore_paused(&pending.saved_paused);

        if let Err(e) = trash::delete(&pending.path) {
            log::warn!("Failed to trash {:?}: {}", pending.path, e);
            return;
        }

        let Some(pane) = self.panes.get_mut(&pending.pane_id) else {
            return;
        };

        // Remove from image list
        pane.image_files.retain(|p| p != &pending.path);
        pane.current_image_path = None;
        pane.texture = None;
        pane.last_switch = None;

        // Show next image in history (forward/newer)
        let mut found = false;
        let mut pos = pane.history_pos;
        while pos > 0 {
            pos -= 1;
            let idx = pane.history.len() - 1 - pos;
            if pane.history[idx].exists() {
                pane.history_pos = pos;
                let path = pane.history[idx].clone();
                pane.texture = image_loader::load_texture(ctx, &path);
                pane.current_image_path = Some(path);
                pane.last_switch = Some(Instant::now());
                found = true;
                break;
            }
        }
        if !found {
            pane.history_pos = 0;
            if let Some(next) = image_loader::pick_random_image(&pane.image_files, None) {
                pane.texture = image_loader::load_texture(ctx, &next);
                pane.current_image_path = Some(next.clone());
                pane.last_switch = Some(Instant::now());
                pane.history.push(next);
            }
        }
        self.dirty = true;
    }

    fn enter_fullscreen(&mut self, pane_id: PaneId) {
        let saved_paused = self.panes.iter().map(|(&id, p)| (id, p.paused)).collect();
        for pane in self.panes.values_mut() {
            pane.paused = true;
        }
        self.fullscreen = Some(FullscreenState {
            pane_id,
            saved_paused,
        });
    }

    fn copy_image_to_clipboard(&mut self, pane_id: PaneId) {
        let Some(pane) = self.panes.get(&pane_id) else {
            return;
        };
        let Some(ref path) = pane.current_image_path else {
            return;
        };
        match copy_file_to_clipboard(path) {
            Ok(()) => self.copy_toast = Some(Instant::now()),
            Err(e) => log::warn!("Failed to copy to clipboard: {}", e),
        }
    }

    fn exit_fullscreen(&mut self) {
        if let Some(fs) = self.fullscreen.take() {
            self.restore_paused(&fs.saved_paused);
        }
    }
}

impl eframe::App for F2ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Q key to quit
        if ctx.input(|i| i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        self.update_timers(ctx);

        let mut actions = Vec::new();
        let mut exit_fs = false;

        if let Some(ref fs) = self.fullscreen {
            // Fullscreen: render single pane image filling the entire panel
            let fs_pane_id = fs.pane_id;
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::BLACK))
                .show(ctx, |ui| {
                    let rect = ui.max_rect();
                    ui.allocate_rect(rect, egui::Sense::hover());
                    if let Some(pane) = self.panes.get(&fs_pane_id) {
                        if let Some(ref texture) = pane.texture {
                            let tex_size = texture.size_vec2();
                            let scale = (rect.width() / tex_size.x).min(rect.height() / tex_size.y);
                            let fitted = egui::vec2(tex_size.x * scale, tex_size.y * scale);
                            let offset = (rect.size() - fitted) * 0.5;
                            let image_rect = egui::Rect::from_min_size(rect.min + offset, fitted);
                            ui.painter().image(
                                texture.id(),
                                image_rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                    }
                });

            // Keyboard shortcuts in fullscreen
            if ctx.input(|i| i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::Escape)) {
                exit_fs = true;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::D)) {
                self.delete_current_image(fs_pane_id, ctx);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowDown)) {
                self.navigate_image(fs_pane_id, ctx, 1);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowUp)) {
                self.navigate_image(fs_pane_id, ctx, -1);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::C)) {
                self.copy_image_to_clipboard(fs_pane_id);
            }
        } else {
            let is_root_single = self.tree.is_single_leaf();
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::BLACK))
                .show(ctx, |ui| {
                    let panel_rect = ui.max_rect();
                    tree_ui::render_tree(
                        ui,
                        &self.tree,
                        &mut self.panes,
                        is_root_single,
                        &mut actions,
                    );

                    tree_ui::handle_separator_drag(ui, &mut self.tree, panel_rect);
                });
        }

        self.process_actions(actions, ctx);

        if exit_fs {
            self.exit_fullscreen();
        }

        // Delete confirmation dialog at cursor position
        let mut do_confirm = false;
        let mut do_cancel = false;
        if let Some(ref pending) = self.pending_delete {
            let filename = pending
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            egui::Area::new(egui::Id::new("delete_confirm"))
                .fixed_pos(pending.pos)
                .pivot(egui::Align2::LEFT_TOP)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(200.0);
                        ui.vertical(|ui| {
                            ui.label(format!("「{}」をゴミ箱に移動しますか？", filename));
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui.button("はい").clicked() {
                                    do_confirm = true;
                                }
                                if ui.button("いいえ").clicked() {
                                    do_cancel = true;
                                }
                            });
                        });
                    });
                });

            // Escape key to cancel
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                do_cancel = true;
            }
            // Enter/Y/Space key to confirm
            if ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Y) || i.key_pressed(egui::Key::Space)) {
                do_confirm = true;
            }
        }
        if do_confirm {
            self.confirm_delete(ctx);
        } else if do_cancel {
            if let Some(pending) = self.pending_delete.take() {
                self.restore_paused(&pending.saved_paused);
            }
        }

        // Copy toast notification
        if let Some(toast_time) = self.copy_toast {
            let elapsed = Instant::now().duration_since(toast_time).as_secs_f32();
            if elapsed < 1.0 {
                let alpha = ((1.0 - elapsed) * 255.0) as u8;
                let screen = ctx.screen_rect();
                let pos = egui::pos2(screen.center().x, screen.bottom() - 40.0);
                egui::Area::new(egui::Id::new("copy_toast"))
                    .fixed_pos(pos)
                    .pivot(egui::Align2::CENTER_BOTTOM)
                    .order(egui::Order::Foreground)
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style())
                            .fill(Color32::from_rgba_unmultiplied(40, 40, 40, alpha))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("クリップボードにコピーしました")
                                        .color(Color32::from_rgba_unmultiplied(255, 255, 255, alpha)),
                                );
                            });
                    });
                ctx.request_repaint();
            } else {
                self.copy_toast = None;
            }
        }

        // Save state when dirty and separator drag ends
        if self.dirty || ctx.input(|i| i.pointer.any_released()) {
            self.save_state();
            self.dirty = false;
        }
    }
}
