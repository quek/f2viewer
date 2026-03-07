use std::collections::HashMap;

use egui::{Color32, CursorIcon, Rect, Stroke, UiBuilder, vec2};

use crate::pane::ImagePane;
use crate::split_tree::{PaneId, SplitDirection, SplitTree};
use crate::ui::controls::PaneAction;
use crate::ui::pane_ui;

const SEPARATOR_WIDTH: f32 = 4.0;
const MIN_PANE_RATIO: f32 = 0.05;
const MAX_PANE_RATIO: f32 = 0.95;

/// Recursively render the split tree into the available UI area.
/// Collects all triggered actions into the `actions` vec.
pub fn render_tree(
    ui: &mut egui::Ui,
    tree: &SplitTree,
    panes: &mut HashMap<PaneId, ImagePane>,
    is_root_single: bool,
    actions: &mut Vec<PaneAction>,
) {
    render_node(ui, tree, panes, is_root_single, actions);
}

fn render_node(
    ui: &mut egui::Ui,
    tree: &SplitTree,
    panes: &mut HashMap<PaneId, ImagePane>,
    is_root_single: bool,
    actions: &mut Vec<PaneAction>,
) {
    match tree {
        SplitTree::Leaf { id } => {
            if let Some(pane) = panes.get_mut(id) {
                if let Some(action) = pane_ui::render_pane(ui, *id, pane, is_root_single) {
                    actions.push(action);
                }
            }
        }
        SplitTree::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let rect = ui.available_rect_before_wrap();
            ui.allocate_rect(rect, egui::Sense::hover());

            let (first_rect, _sep_rect, second_rect) = split_rect(rect, *direction, *ratio);

            // Render first child
            let mut child_ui = ui.new_child(UiBuilder::new().max_rect(first_rect));
            render_node(&mut child_ui, first, panes, false, actions);

            // Render second child
            let mut child_ui = ui.new_child(UiBuilder::new().max_rect(second_rect));
            render_node(&mut child_ui, second, panes, false, actions);

            // Draw separator
            draw_separator(ui, _sep_rect, *direction);
        }
    }
}

/// Handle separator dragging. Must be called separately with mutable tree access.
pub fn handle_separator_drag(ui: &mut egui::Ui, tree: &mut SplitTree, full_rect: Rect) {
    handle_drag_recursive(ui, tree, full_rect);
}

fn handle_drag_recursive(ui: &mut egui::Ui, tree: &mut SplitTree, rect: Rect) {
    if let SplitTree::Split {
        direction,
        ratio,
        first,
        second,
    } = tree
    {
        let (_first_rect, sep_rect, _second_rect) = split_rect(rect, *direction, *ratio);

        let sep_id = egui::Id::new(("separator", sep_rect.min.x as i32, sep_rect.min.y as i32));
        let response = ui.interact(sep_rect.expand(2.0), sep_id, egui::Sense::drag());

        if response.hovered() {
            match direction {
                SplitDirection::Vertical => {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal)
                }
                SplitDirection::Horizontal => {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical)
                }
            }
        }

        if response.dragged() {
            let delta = response.drag_delta();
            match direction {
                SplitDirection::Vertical => {
                    let total = rect.width() - SEPARATOR_WIDTH;
                    if total > 0.0 {
                        *ratio = (*ratio + delta.x / total).clamp(MIN_PANE_RATIO, MAX_PANE_RATIO);
                    }
                }
                SplitDirection::Horizontal => {
                    let total = rect.height() - SEPARATOR_WIDTH;
                    if total > 0.0 {
                        *ratio = (*ratio + delta.y / total).clamp(MIN_PANE_RATIO, MAX_PANE_RATIO);
                    }
                }
            }
        }

        // Recurse into children
        let (first_rect, _, second_rect) = split_rect(rect, *direction, *ratio);
        handle_drag_recursive(ui, first, first_rect);
        handle_drag_recursive(ui, second, second_rect);
    }
}

fn split_rect(rect: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Rect, Rect) {
    match direction {
        SplitDirection::Vertical => {
            let available = rect.width() - SEPARATOR_WIDTH;
            let first_w = available * ratio;
            let first_rect = Rect::from_min_size(rect.min, vec2(first_w, rect.height()));
            let sep_rect = Rect::from_min_size(
                rect.min + vec2(first_w, 0.0),
                vec2(SEPARATOR_WIDTH, rect.height()),
            );
            let second_rect = Rect::from_min_size(
                rect.min + vec2(first_w + SEPARATOR_WIDTH, 0.0),
                vec2(available - first_w, rect.height()),
            );
            (first_rect, sep_rect, second_rect)
        }
        SplitDirection::Horizontal => {
            let available = rect.height() - SEPARATOR_WIDTH;
            let first_h = available * ratio;
            let first_rect = Rect::from_min_size(rect.min, vec2(rect.width(), first_h));
            let sep_rect = Rect::from_min_size(
                rect.min + vec2(0.0, first_h),
                vec2(rect.width(), SEPARATOR_WIDTH),
            );
            let second_rect = Rect::from_min_size(
                rect.min + vec2(0.0, first_h + SEPARATOR_WIDTH),
                vec2(rect.width(), available - first_h),
            );
            (first_rect, sep_rect, second_rect)
        }
    }
}

fn draw_separator(ui: &egui::Ui, rect: Rect, direction: SplitDirection) {
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_gray(60));
    // Draw a thin highlight line in the center
    let center = rect.center();
    let (p1, p2) = if matches!(direction, SplitDirection::Vertical) {
        (
            egui::pos2(center.x, rect.top() + 10.0),
            egui::pos2(center.x, rect.bottom() - 10.0),
        )
    } else {
        (
            egui::pos2(rect.left() + 10.0, center.y),
            egui::pos2(rect.right() - 10.0, center.y),
        )
    };
    ui.painter()
        .line_segment([p1, p2], Stroke::new(1.0, Color32::from_gray(100)));
}
