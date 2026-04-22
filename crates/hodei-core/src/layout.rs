use crate::types::{ViewId, Rect, Direction, SplitDirection, LayoutNodeRow};

#[derive(Clone)]
pub enum Node {
    Leaf { view_id: ViewId },
    Branch {
        direction: SplitDirection,
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

pub struct BspLayout {
    root: Option<Node>,
    viewport: Rect,
    focused: Option<ViewId>,
}

impl BspLayout {
    pub fn new(viewport: Rect) -> Self {
        log::debug!("BspLayout::new: viewport={:?}", viewport);
        Self { root: None, viewport, focused: None }
    }

    pub fn add_first_view(&mut self, view_id: ViewId) {
        assert!(self.root.is_none(), "add_first_view called on non-empty layout");
        log::info!("BspLayout::add_first_view: view_id={:?}", view_id);
        self.root = Some(Node::Leaf { view_id });
        self.focused = Some(view_id);
    }

    pub fn resolve(&self) -> Vec<(ViewId, Rect)> {
        let mut result = Vec::new();
        if let Some(ref root) = self.root {
            Self::resolve_node(root, self.viewport, &mut result);
        }
        log::trace!("BspLayout::resolve: {} tiles", result.len());
        result
    }

    fn resolve_node(node: &Node, rect: Rect, out: &mut Vec<(ViewId, Rect)>) {
        match node {
            Node::Leaf { view_id } => out.push((*view_id, rect)),
            Node::Branch { direction, ratio, first, second } => {
                let (r1, r2) = Self::split_rect(rect, *direction, *ratio);
                Self::resolve_node(first, r1, out);
                Self::resolve_node(second, r2, out);
            }
        }
    }

    fn split_rect(rect: Rect, dir: SplitDirection, ratio: f32) -> (Rect, Rect) {
        match dir {
            SplitDirection::Vertical => {
                let w1 = rect.width * ratio;
                (
                    Rect::new(rect.x, rect.y, w1, rect.height),
                    Rect::new(rect.x + w1, rect.y, rect.width - w1, rect.height),
                )
            }
            SplitDirection::Horizontal => {
                let h1 = rect.height * ratio;
                (
                    Rect::new(rect.x, rect.y, rect.width, h1),
                    Rect::new(rect.x, rect.y + h1, rect.width, rect.height - h1),
                )
            }
        }
    }

    pub fn focused(&self) -> Option<ViewId> {
        self.focused
    }

    pub fn set_focused(&mut self, view_id: ViewId) {
        log::debug!("BspLayout::set_focused: {:?}", view_id);
        self.focused = Some(view_id);
    }

    pub fn set_viewport(&mut self, viewport: Rect) {
        log::debug!("BspLayout::set_viewport: {:?}", viewport);
        self.viewport = viewport;
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn split(&mut self, target: ViewId, dir: SplitDirection, new_id: ViewId) {
        log::info!("BspLayout::split: target={:?} direction={:?} new_id={:?}", target, dir, new_id);
        if let Some(ref mut root) = self.root {
            Self::split_node(root, target, dir, new_id);
            self.focused = Some(new_id);
            log::info!("BspLayout::split: done, focused set to {:?}", new_id);
        } else {
            log::warn!("BspLayout::split: root is empty, cannot split");
        }
    }

    fn split_node(node: &mut Node, target: ViewId, dir: SplitDirection, new_id: ViewId) -> bool {
        match node {
            Node::Leaf { view_id } if *view_id == target => {
                let old_leaf = Node::Leaf { view_id: *view_id };
                let new_leaf = Node::Leaf { view_id: new_id };
                *node = Node::Branch {
                    direction: dir,
                    ratio: 0.5,
                    first: Box::new(old_leaf),
                    second: Box::new(new_leaf),
                };
                true
            }
            Node::Branch { first, second, .. } => {
                Self::split_node(first, target, dir, new_id)
                    || Self::split_node(second, target, dir, new_id)
            }
            _ => false,
        }
    }

    pub fn close(&mut self, target: ViewId) {
        log::info!("BspLayout::close: target={:?}", target);
        if let Some(ref mut root) = self.root {
            match root {
                Node::Leaf { view_id } if *view_id == target => {
                    log::debug!("BspLayout::close: closing sole leaf");
                    self.root = None;
                    self.focused = None;
                    return;
                }
                _ => {}
            }
            if let Some(replacement) = Self::close_node(root, target) {
                *root = replacement;
            }
        }
        // Update focus if we closed the focused view
        if self.focused == Some(target) {
            let new_focus = self.first_leaf();
            log::debug!("BspLayout::close: closed focused view, new focus={:?}", new_focus);
            self.focused = new_focus;
        }
    }

    /// Returns Some(sibling) if the target was found and removed at this level.
    fn close_node(node: &mut Node, target: ViewId) -> Option<Node> {
        match node {
            Node::Branch { first, second, .. } => {
                // Check if first child is the target leaf
                if matches!(first.as_ref(), Node::Leaf { view_id } if *view_id == target) {
                    return Some(*second.clone());
                }
                // Check if second child is the target leaf
                if matches!(second.as_ref(), Node::Leaf { view_id } if *view_id == target) {
                    return Some(*first.clone());
                }
                // Recurse into children
                if let Some(replacement) = Self::close_node(first, target) {
                    *first = Box::new(replacement);
                } else if let Some(replacement) = Self::close_node(second, target) {
                    *second = Box::new(replacement);
                }
                None
            }
            _ => None,
        }
    }

    fn first_leaf(&self) -> Option<ViewId> {
        fn walk(node: &Node) -> ViewId {
            match node {
                Node::Leaf { view_id } => *view_id,
                Node::Branch { first, .. } => walk(first),
            }
        }
        self.root.as_ref().map(walk)
    }

    pub fn focus_neighbor(&self, from: ViewId, dir: Direction) -> Option<ViewId> {
        log::trace!("BspLayout::focus_neighbor: from={:?} dir={:?}", from, dir);
        let resolved = self.resolve();
        let current = resolved.iter().find(|(id, _)| *id == from)?;
        let current_rect = current.1;

        // Center point of current tile
        let cx = current_rect.x + current_rect.width / 2.0;
        let cy = current_rect.y + current_rect.height / 2.0;

        // Find the closest tile in the given direction
        let mut best: Option<(ViewId, f32)> = None;
        for (id, rect) in &resolved {
            if *id == from {
                continue;
            }
            let nx = rect.x + rect.width / 2.0;
            let ny = rect.y + rect.height / 2.0;
            let is_in_direction = match dir {
                Direction::Left => nx < cx,
                Direction::Right => nx > cx,
                Direction::Up => ny < cy,
                Direction::Down => ny > cy,
            };
            if !is_in_direction {
                continue;
            }
            let dist = (nx - cx).abs() + (ny - cy).abs();
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((*id, dist));
            }
        }
        let result = best.map(|(id, _)| id);
        log::trace!("BspLayout::focus_neighbor: from={:?} dir={:?} result={:?}", from, dir, result);
        result
    }

    pub fn resize_split(&mut self, target: ViewId, delta: f32) {
        log::trace!("BspLayout::resize_split: target={:?} delta={}", target, delta);
        if let Some(ref mut root) = self.root {
            Self::resize_node(root, target, delta);
        }
    }

    fn resize_node(node: &mut Node, target: ViewId, delta: f32) -> bool {
        match node {
            Node::Branch { ratio, first, second, .. } => {
                // Check if target is in first child
                if Self::contains_view(first, target) {
                    *ratio = (*ratio + delta).clamp(0.1, 0.9);
                    return true;
                }
                // Check if target is in second child (resize opposite direction)
                if Self::contains_view(second, target) {
                    *ratio = (*ratio - delta).clamp(0.1, 0.9);
                    return true;
                }
                // Recurse
                Self::resize_node(first, target, delta) || Self::resize_node(second, target, delta)
            }
            _ => false,
        }
    }

    fn contains_view(node: &Node, target: ViewId) -> bool {
        match node {
            Node::Leaf { view_id } => *view_id == target,
            Node::Branch { first, second, .. } => {
                Self::contains_view(first, target) || Self::contains_view(second, target)
            }
        }
    }

    pub fn reset_splits(&mut self) {
        log::debug!("BspLayout::reset_splits");
        if let Some(ref mut root) = self.root {
            Self::reset_node(root);
        }
    }

    fn reset_node(node: &mut Node) {
        match node {
            Node::Branch { ratio, first, second, .. } => {
                *ratio = 0.5;
                Self::reset_node(first);
                Self::reset_node(second);
            }
            _ => {}
        }
    }

    pub fn swap_tiles(&mut self, a: ViewId, b: ViewId) {
        log::info!("BspLayout::swap_tiles: a={:?} b={:?}", a, b);
        if let Some(ref mut root) = self.root {
            Self::swap_node(root, a, b);
        }
    }

    fn swap_node(node: &mut Node, a: ViewId, b: ViewId) {
        match node {
            Node::Leaf { view_id } => {
                if *view_id == a {
                    *view_id = b;
                } else if *view_id == b {
                    *view_id = a;
                }
            }
            Node::Branch { first, second, .. } => {
                Self::swap_node(first, a, b);
                Self::swap_node(second, a, b);
            }
        }
    }

    pub fn next_focus(&self, current: ViewId) -> Option<ViewId> {
        let resolved = self.resolve();
        let positions: Vec<ViewId> = resolved.into_iter().map(|(id, _)| id).collect();
        if positions.is_empty() {
            return None;
        }
        let result = if let Some(idx) = positions.iter().position(|&id| id == current) {
            let next_idx = (idx + 1) % positions.len();
            Some(positions[next_idx])
        } else {
            positions.first().copied()
        };
        log::trace!("BspLayout::next_focus: current={:?} result={:?}", current, result);
        result
    }

    pub fn prev_focus(&self, current: ViewId) -> Option<ViewId> {
        let resolved = self.resolve();
        let positions: Vec<ViewId> = resolved.into_iter().map(|(id, _)| id).collect();
        if positions.is_empty() {
            return None;
        }
        let result = if let Some(idx) = positions.iter().position(|&id| id == current) {
            let prev_idx = if idx == 0 { positions.len() - 1 } else { idx - 1 };
            Some(positions[prev_idx])
        } else {
            positions.first().copied()
        };
        log::trace!("BspLayout::prev_focus: current={:?} result={:?}", current, result);
        result
    }

    /// Serialize to BFS-order node rows for SQLite storage.
    pub fn serialize(&self) -> (Vec<LayoutNodeRow>, Option<ViewId>) {
        let mut rows = Vec::new();
        if let Some(ref root) = self.root {
            let mut queue = std::collections::VecDeque::new();
            queue.push_back((root, 0u32));
            while let Some((node, index)) = queue.pop_front() {
                match node {
                    Node::Leaf { view_id } => {
                        rows.push(LayoutNodeRow {
                            node_index: index,
                            is_leaf: true,
                            direction: None,
                            ratio: None,
                            view_id: Some(*view_id),
                        });
                    }
                    Node::Branch { direction, ratio, first, second } => {
                        rows.push(LayoutNodeRow {
                            node_index: index,
                            is_leaf: false,
                            direction: Some(*direction),
                            ratio: Some(*ratio),
                            view_id: None,
                        });
                        queue.push_back((first, index * 2 + 1));
                        queue.push_back((second, index * 2 + 2));
                    }
                }
            }
        }
        log::debug!("BspLayout::serialize: {} rows, focused={:?}", rows.len(), self.focused);
        (rows, self.focused)
    }

    /// Deserialize from BFS-order node rows.
    pub fn deserialize(viewport: Rect, rows: &[LayoutNodeRow], focused: Option<ViewId>) -> Self {
        log::debug!("BspLayout::deserialize: {} rows, focused={:?}", rows.len(), focused);
        if rows.is_empty() {
            return Self { root: None, viewport, focused };
        }
        let map: std::collections::HashMap<u32, &LayoutNodeRow> =
            rows.iter().map(|r| (r.node_index, r)).collect();

        fn build(map: &std::collections::HashMap<u32, &LayoutNodeRow>, index: u32) -> Option<Node> {
            let row = map.get(&index)?;
            if row.is_leaf {
                Some(Node::Leaf { view_id: row.view_id.unwrap() })
            } else {
                let first = build(map, index * 2 + 1)?;
                let second = build(map, index * 2 + 2)?;
                Some(Node::Branch {
                    direction: row.direction.unwrap(),
                    ratio: row.ratio.unwrap(),
                    first: Box::new(first),
                    second: Box::new(second),
                })
            }
        }

        let root = build(&map, 0);
        Self { root, viewport, focused }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> Rect {
        Rect::new(0.0, 0.0, 800.0, 600.0)
    }

    #[test]
    fn new_layout_is_empty() {
        let layout = BspLayout::new(vp());
        assert!(layout.is_empty());
        assert!(layout.resolve().is_empty());
        assert!(layout.focused().is_none());
    }

    #[test]
    fn add_first_view_single_leaf() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        let resolved = layout.resolve();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, ViewId(1));
        assert_eq!(resolved[0].1, vp());
        assert_eq!(layout.focused(), Some(ViewId(1)));
    }

    #[test]
    fn split_vertical_creates_two_tiles() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        let resolved = layout.resolve();
        assert_eq!(resolved.len(), 2);
        // First child: left half
        assert_eq!(resolved[0].0, ViewId(1));
        assert_eq!(resolved[0].1, Rect::new(0.0, 0.0, 400.0, 600.0));
        // Second child: right half
        assert_eq!(resolved[1].0, ViewId(2));
        assert_eq!(resolved[1].1, Rect::new(400.0, 0.0, 400.0, 600.0));
    }

    #[test]
    fn split_horizontal_creates_top_bottom() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Horizontal, ViewId(2));
        let resolved = layout.resolve();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].1, Rect::new(0.0, 0.0, 800.0, 300.0));
        assert_eq!(resolved[1].1, Rect::new(0.0, 300.0, 800.0, 300.0));
    }

    #[test]
    fn nested_split() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.split(ViewId(2), SplitDirection::Horizontal, ViewId(3));
        let resolved = layout.resolve();
        assert_eq!(resolved.len(), 3);
        // Left half unchanged
        assert_eq!(resolved[0].1, Rect::new(0.0, 0.0, 400.0, 600.0));
        // Right-top
        assert_eq!(resolved[1].1, Rect::new(400.0, 0.0, 400.0, 300.0));
        // Right-bottom
        assert_eq!(resolved[2].1, Rect::new(400.0, 300.0, 400.0, 300.0));
    }

    #[test]
    fn close_last_view_empties_layout() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.close(ViewId(1));
        assert!(layout.is_empty());
    }

    #[test]
    fn close_promotes_sibling() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.close(ViewId(2));
        let resolved = layout.resolve();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, ViewId(1));
        assert_eq!(resolved[0].1, vp()); // full viewport restored
    }

    #[test]
    fn close_in_nested_tree() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.split(ViewId(2), SplitDirection::Horizontal, ViewId(3));
        // Close v2 (top-right) — its sibling v3 (bottom-right) takes the right half
        layout.close(ViewId(2));
        let resolved = layout.resolve();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].0, ViewId(1)); // left
        // ViewId(3) takes over the right half
        assert_eq!(resolved[1].1, Rect::new(400.0, 0.0, 400.0, 600.0));
    }

    #[test]
    fn focus_neighbor_vertical_split() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        // From left (v1), go right → v2
        assert_eq!(layout.focus_neighbor(ViewId(1), Direction::Right), Some(ViewId(2)));
        // From right (v2), go left → v1
        assert_eq!(layout.focus_neighbor(ViewId(2), Direction::Left), Some(ViewId(1)));
        // From left (v1), go left → None (no neighbor)
        assert_eq!(layout.focus_neighbor(ViewId(1), Direction::Left), None);
    }

    #[test]
    fn focus_neighbor_horizontal_split() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Horizontal, ViewId(2));
        assert_eq!(layout.focus_neighbor(ViewId(1), Direction::Down), Some(ViewId(2)));
        assert_eq!(layout.focus_neighbor(ViewId(2), Direction::Up), Some(ViewId(1)));
        assert_eq!(layout.focus_neighbor(ViewId(1), Direction::Up), None);
    }

    #[test]
    fn resize_split_adjusts_ratio() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.resize_split(ViewId(1), 0.1); // increase left side
        let resolved = layout.resolve();
        // Left should be 60% (0.5 + 0.1)
        let left_width = resolved[0].1.width;
        assert!((left_width - 480.0).abs() < 1.0);
    }

    #[test]
    fn resize_split_clamps_ratio() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.resize_split(ViewId(1), 1.0); // try to push way past limit
        let resolved = layout.resolve();
        let left_width = resolved[0].1.width;
        // Should be clamped to 90% max
        assert!((left_width - 720.0).abs() < 1.0);
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.split(ViewId(2), SplitDirection::Horizontal, ViewId(3));
        layout.set_focused(ViewId(1));

        let (nodes, focused) = layout.serialize();
        let restored = BspLayout::deserialize(vp(), &nodes, focused);
        assert_eq!(layout.resolve(), restored.resolve());
        assert_eq!(restored.focused(), Some(ViewId(1)));
    }

    #[test]
    fn reset_splits_equalizes() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.resize_split(ViewId(1), 0.2);
        layout.reset_splits();
        let resolved = layout.resolve();
        assert!((resolved[0].1.width - 400.0).abs() < 1.0);
    }

    #[test]
    fn swap_tiles_exchanges_ids() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        layout.swap_tiles(ViewId(1), ViewId(2));
        let resolved = layout.resolve();
        assert_eq!(resolved[0].0, ViewId(2));
        assert_eq!(resolved[1].0, ViewId(1));
    }

    #[test]
    fn next_focus_cycles() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        assert_eq!(layout.next_focus(ViewId(1)), Some(ViewId(2)));
        assert_eq!(layout.next_focus(ViewId(2)), Some(ViewId(1)));
    }

    #[test]
    fn prev_focus_cycles() {
        let mut layout = BspLayout::new(vp());
        layout.add_first_view(ViewId(1));
        layout.split(ViewId(1), SplitDirection::Vertical, ViewId(2));
        assert_eq!(layout.prev_focus(ViewId(2)), Some(ViewId(1)));
        assert_eq!(layout.prev_focus(ViewId(1)), Some(ViewId(2)));
    }
}
