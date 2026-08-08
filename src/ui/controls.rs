use crate::split_tree::PaneId;

/// Actions that can be triggered from a pane's control overlay.
#[derive(Debug, Clone)]
pub enum PaneAction {
    SplitHorizontal(PaneId),
    SplitVertical(PaneId),
    Close(PaneId),
    SelectDirectory(PaneId),
    TogglePause(PaneId),
    DeleteCurrentImage(PaneId),
    NavigateForward(PaneId),
    NavigateBackward(PaneId),
    Fullscreen(PaneId),
    CopyImage(PaneId),
    /// Start an OLE drag so the current image can be dropped into another application.
    DragImage(PaneId),
}
