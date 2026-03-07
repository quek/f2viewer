use egui::{Color32, Rect, Stroke, StrokeKind, Vec2, vec2};

use crate::pane::ImagePane;
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

    // Draw border
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::DARK_GRAY),
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

    // Context menu for controls
    let mut action = None;
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
            ui.add(egui::Slider::new(&mut pane.display_duration, 0.5..=60.0).suffix("秒"));
        });

        // Pause toggle
        let pause_label = if pane.paused { "▶ 再開" } else { "⏸ 一時停止" };
        if ui.button(pause_label).clicked() {
            pane.paused = !pane.paused;
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
