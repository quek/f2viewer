use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::Color32;

use crate::image_loader;
use crate::pane::ImagePane;
use crate::split_tree::{PaneId, SplitDirection, SplitTree};
use crate::ui::controls::PaneAction;
use crate::ui::tree_ui;

pub struct F2ViewerApp {
    tree: SplitTree,
    panes: HashMap<PaneId, ImagePane>,
    next_pane_id: PaneId,
}

impl F2ViewerApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        Self::setup_fonts(&cc.egui_ctx);

        let id = 0;
        let mut panes = HashMap::new();
        panes.insert(id, ImagePane::default());
        Self {
            tree: SplitTree::new_leaf(id),
            panes,
            next_pane_id: 1,
        }
    }

    fn setup_fonts(ctx: &egui::Context) {
        let font_path = dirs::font_dir()
            .unwrap_or_default()
            .join("HackGen35ConsoleNF-Regular.ttf");
        let font_data = std::fs::read(&font_path).unwrap_or_else(|_| {
            // Fallback: user-local fonts on Windows
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
        // Prepend HackGen to proportional and monospace families
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

    fn alloc_id(&mut self) -> PaneId {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        id
    }

    /// Update image timers for all panes. Loads next image when duration elapses.
    fn update_timers(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let mut min_remaining = Duration::from_secs(60);

        for pane in self.panes.values_mut() {
            // Rescan directory if needed
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

            // Calculate remaining time for repaint scheduling
            if let Some(last) = pane.last_switch {
                let elapsed = now.duration_since(last);
                if elapsed < duration {
                    let remaining = duration - elapsed;
                    min_remaining = min_remaining.min(remaining);
                }
            }
        }

        // Schedule next repaint
        if !self.panes.is_empty() {
            ctx.request_repaint_after(min_remaining);
        }
    }

    /// Process deferred actions from UI interactions.
    fn process_actions(&mut self, actions: Vec<PaneAction>) {
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
    }

    fn split_pane(&mut self, pane_id: PaneId, direction: SplitDirection) {
        let new_id_1 = self.alloc_id();
        let new_id_2 = self.alloc_id();

        // Create new panes inheriting from the original
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

                // Handle separator dragging within the same UI
                tree_ui::handle_separator_drag(ui, &mut self.tree, panel_rect);
            });

        self.process_actions(actions);
    }
}
