use serde::{Deserialize, Serialize};

pub type PaneId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SplitTree {
    Leaf {
        id: PaneId,
    },
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<SplitTree>,
        second: Box<SplitTree>,
    },
}

impl SplitTree {
    pub fn new_leaf(id: PaneId) -> Self {
        SplitTree::Leaf { id }
    }

    /// Split the leaf with `target_id` into two new leaves.
    /// Returns true if the split was performed.
    pub fn split(
        &mut self,
        target_id: PaneId,
        direction: SplitDirection,
        new_id_1: PaneId,
        new_id_2: PaneId,
    ) -> bool {
        match self {
            SplitTree::Leaf { id } if *id == target_id => {
                *self = SplitTree::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(SplitTree::new_leaf(new_id_1)),
                    second: Box::new(SplitTree::new_leaf(new_id_2)),
                };
                true
            }
            SplitTree::Split {
                first, second, ..
            } => {
                first.split(target_id, direction, new_id_1, new_id_2)
                    || second.split(target_id, direction, new_id_1, new_id_2)
            }
            _ => false,
        }
    }

    /// Remove the leaf with `target_id`, replacing the parent split with the sibling.
    /// Returns the list of removed pane IDs (including target_id and any nested leaves).
    pub fn unsplit(&mut self, target_id: PaneId) -> Vec<PaneId> {
        match self {
            SplitTree::Split {
                first, second, ..
            } => {
                // Check if first child is the target leaf
                if let SplitTree::Leaf { id } = first.as_ref() {
                    if *id == target_id {
                        let removed_id = *id;
                        let sibling = std::mem::replace(
                            second.as_mut(),
                            SplitTree::Leaf { id: 0 },
                        );
                        *self = sibling;
                        return vec![removed_id];
                    }
                }
                // Check if second child is the target leaf
                if let SplitTree::Leaf { id } = second.as_ref() {
                    if *id == target_id {
                        let removed_id = *id;
                        let sibling = std::mem::replace(
                            first.as_mut(),
                            SplitTree::Leaf { id: 0 },
                        );
                        *self = sibling;
                        return vec![removed_id];
                    }
                }
                // Recurse
                let mut removed = first.unsplit(target_id);
                if removed.is_empty() {
                    removed = second.unsplit(target_id);
                }
                removed
            }
            _ => vec![],
        }
    }

    /// Collect all leaf IDs in the tree.
    pub fn leaf_ids(&self) -> Vec<PaneId> {
        match self {
            SplitTree::Leaf { id } => vec![*id],
            SplitTree::Split { first, second, .. } => {
                let mut ids = first.leaf_ids();
                ids.extend(second.leaf_ids());
                ids
            }
        }
    }

    /// Returns true if this is the only leaf (root is a leaf).
    pub fn is_single_leaf(&self) -> bool {
        matches!(self, SplitTree::Leaf { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_and_unsplit() {
        let mut tree = SplitTree::new_leaf(0);
        assert!(tree.is_single_leaf());

        // Split leaf 0 into leaves 1 and 2
        assert!(tree.split(0, SplitDirection::Vertical, 1, 2));
        assert!(!tree.is_single_leaf());
        assert_eq!(tree.leaf_ids(), vec![1, 2]);

        // Split leaf 1 into leaves 3 and 4
        assert!(tree.split(1, SplitDirection::Horizontal, 3, 4));
        assert_eq!(tree.leaf_ids(), vec![3, 4, 2]);

        // Unsplit leaf 3 (sibling 4 replaces the parent)
        let removed = tree.unsplit(3);
        assert_eq!(removed, vec![3]);
        assert_eq!(tree.leaf_ids(), vec![4, 2]);
    }
}
