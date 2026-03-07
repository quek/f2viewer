use egui::{Color32, Rect, Stroke, StrokeKind, Vec2, vec2};

use crate::pane::{DisplayMode, ImagePane};
use crate::split_tree::PaneId;
use crate::ui::controls::PaneAction;

/// Render a single image pane within the given rect.
/// Returns an optional action triggered by the user.
pub fn render_pane(
    ui: &mut egui::Ui,
    pane_id: PaneId,
    pane: &mut ImagePane,
    is_root: bool,
) -> Option<PaneAction> {
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, egui::Sense::hover());

    let hovered = ui.rect_contains_pointer(rect);

    // Draw border: highlight when hovered
    let border_color = if hovered {
        Color32::from_rgb(80, 140, 220)
    } else {
        Color32::DARK_GRAY
    };
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(if hovered { 2.0 } else { 1.0 }, border_color),
        StrokeKind::Inside,
    );

    // Display image or placeholder
    if let Some(ref texture) = pane.texture {
        let tex_size = texture.size_vec2();
        let fitted = fit_to_rect(tex_size, rect);
        let image_rect = center_in_rect(fitted, rect);
        ui.painter().image(
            texture.id(),
            image_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        let center = rect.center();
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "右クリックでディレクトリを選択",
            egui::FontId::proportional(14.0),
            Color32::GRAY,
        );
    }

    // Filename overlay at bottom
    if let Some(ref path) = pane.current_image_path {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        let text_pos = egui::pos2(rect.left() + 4.0, rect.bottom() - 2.0);
        // Shadow
        ui.painter().text(
            text_pos + vec2(1.0, 1.0),
            egui::Align2::LEFT_BOTTOM,
            &*filename,
            egui::FontId::proportional(12.0),
            Color32::BLACK,
        );
        ui.painter().text(
            text_pos,
            egui::Align2::LEFT_BOTTOM,
            &*filename,
            egui::FontId::proportional(12.0),
            Color32::from_gray(200),
        );
    }

    // Pause indicator
    if pane.paused && pane.texture.is_some() {
        ui.painter().text(
            egui::pos2(rect.right() - 6.0, rect.top() + 6.0),
            egui::Align2::RIGHT_TOP,
            "⏸",
            egui::FontId::proportional(16.0),
            Color32::from_white_alpha(180),
        );
    }

    let mut action = None;

    // Keyboard shortcuts when hovered
    if hovered {
        ui.ctx().input(|i| {
            if i.key_pressed(egui::Key::Space) {
                action = Some(PaneAction::TogglePause(pane_id));
            }
            if i.key_pressed(egui::Key::D) {
                action = Some(PaneAction::DeleteCurrentImage(pane_id));
            }
            if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowDown) {
                action = Some(PaneAction::NavigateForward(pane_id));
            }
            if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowUp) {
                action = Some(PaneAction::NavigateBackward(pane_id));
            }
        });
    }

    // Context menu for controls
    let interact = ui.interact(rect, ui.id().with(("pane_ctx", pane_id)), egui::Sense::click());
    interact.context_menu(|ui| {
        if ui.button("↔ 縦分割").clicked() {
            action = Some(PaneAction::SplitVertical(pane_id));
            ui.close_menu();
        }
        if ui.button("↕ 横分割").clicked() {
            action = Some(PaneAction::SplitHorizontal(pane_id));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("📁 ディレクトリ選択").clicked() {
            action = Some(PaneAction::SelectDirectory(pane_id));
            ui.close_menu();
        }
        ui.separator();

        // Duration slider
        ui.horizontal(|ui| {
            ui.label("表示間隔:");
            let min = 0.5_f32;
            let max = 60.0_f32;
            let t = ((pane.display_duration - min) / (max - min)).sqrt();
            let mut t_slider = t;
            ui.add(egui::Slider::new(&mut t_slider, 0.0..=1.0).custom_formatter(
                |t, _| {
                    let val = t as f32 * t as f32 * (max - min) + min;
                    format!("{:.1}秒", val)
                },
            ));
            if t_slider != t {
                pane.display_duration = t_slider * t_slider * (max - min) + min;
            }
        });

        // Display mode toggle
        let mode_label = match pane.display_mode {
            DisplayMode::Random => "🔀 ランダム → 順番に変更",
            DisplayMode::Sequential => "🔢 順番 → ランダムに変更",
        };
        if ui.button(mode_label).clicked() {
            pane.display_mode = match pane.display_mode {
                DisplayMode::Random => {
                    // Sort and start from beginning
                    pane.seq_index = 0;
                    DisplayMode::Sequential
                }
                DisplayMode::Sequential => DisplayMode::Random,
            };
            pane.paused = false;
            ui.close_menu();
        }

        // Pause toggle
        let pause_label = if pane.paused { "▶ 再開" } else { "⏸ 一時停止" };
        if ui.button(pause_label).clicked() {
            pane.paused = !pane.paused;
            if !pane.paused && pane.display_mode == DisplayMode::Sequential {
                // Resume from current position (reset if at end)
                if pane.seq_index >= pane.image_files.len() {
                    pane.seq_index = 0;
                }
            }
            ui.close_menu();
        }

        // Show current directory
        if let Some(ref dir) = pane.directory {
            ui.separator();
            ui.label(format!("📂 {}", dir.display()));
            ui.label(format!("画像数: {}", pane.image_files.len()));
        }

        if !is_root {
            ui.separator();
            if ui.button("✕ 閉じる").clicked() {
                action = Some(PaneAction::Close(pane_id));
                ui.close_menu();
            }
        }
    });

    action
}

/// Scale size to fit within rect while maintaining aspect ratio.
fn fit_to_rect(size: Vec2, rect: Rect) -> Vec2 {
    let scale_x = rect.width() / size.x;
    let scale_y = rect.height() / size.y;
    let scale = scale_x.min(scale_y);
    vec2(size.x * scale, size.y * scale)
}

/// Center a size within a rect.
fn center_in_rect(size: Vec2, rect: Rect) -> Rect {
    let offset = (rect.size() - size) * 0.5;
    Rect::from_min_size(rect.min + offset, size)
}
