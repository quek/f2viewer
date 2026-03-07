use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::Color32;
use serde::{Deserialize, Serialize};

use crate::image_loader;
use crate::pane::ImagePane;
use crate::split_tree::{PaneId, SplitDirection, SplitTree};
use crate::ui::controls::PaneAction;
use crate::ui::tree_ui;

/// Serializable state for persistence across restarts.
#[derive(Serialize, Deserialize)]
struct SaveState {
    tree: SplitTree,
    panes: HashMap<PaneId, ImagePane>,
    next_pane_id: PaneId,
}

pub struct F2ViewerApp {
    tree: SplitTree,
    panes: HashMap<PaneId, ImagePane>,
    next_pane_id: PaneId,
    dirty: bool,
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
            };
            // Rescan directories for all restored panes
            for pane in app.panes.values_mut() {
                if let Some(ref dir) = pane.directory {
                    pane.image_files = image_loader::scan_directory(dir);
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

    fn save_state(&self) {
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

        for pane in self.panes.values_mut() {
            if pane.needs_rescan {
                if let Some(ref dir) = pane.directory {
                    pane.image_files = image_loader::scan_directory(dir);
                }
                pane.needs_rescan = false;
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
                let current = pane.current_image_path.as_deref();
                if let Some(path) =
                    image_loader::pick_random_image(&pane.image_files, current)
                {
                    pane.texture = image_loader::load_texture(ctx, &path);
                    pane.current_image_path = Some(path);
                    pane.last_switch = Some(now);
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

    fn process_actions(&mut self, actions: Vec<PaneAction>) {
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
}

impl eframe::App for F2ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_timers(ctx);

        let mut actions = Vec::new();
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

        self.process_actions(actions);

        // Save state when dirty and separator drag ends
        if self.dirty || ctx.input(|i| i.pointer.any_released()) {
            self.save_state();
            self.dirty = false;
        }
    }
}
