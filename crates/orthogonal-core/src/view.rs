use std::collections::HashMap;
use crate::types::ViewId;

pub struct View {
    pub id: ViewId,
    pub url: String,
    pub title: String,
    pub dirty: bool,
}

pub struct ViewManager {
    views: HashMap<ViewId, View>,
    next_id: u64,
}

impl ViewManager {
    pub fn new() -> Self {
        Self {
            views: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn create(&mut self, url: &str) -> ViewId {
        let id = ViewId(self.next_id);
        self.next_id += 1;
        self.views.insert(id, View {
            id,
            url: url.to_string(),
            title: String::new(),
            dirty: true,
        });
        id
    }

    pub fn remove(&mut self, id: ViewId) -> Option<View> {
        self.views.remove(&id)
    }

    pub fn get(&self, id: ViewId) -> Option<&View> {
        self.views.get(&id)
    }

    pub fn get_mut(&mut self, id: ViewId) -> Option<&mut View> {
        self.views.get_mut(&id)
    }

    pub fn mark_dirty(&mut self, id: ViewId) {
        if let Some(v) = self.views.get_mut(&id) {
            v.dirty = true;
        }
    }

    pub fn clear_dirty(&mut self, id: ViewId) {
        if let Some(v) = self.views.get_mut(&id) {
            v.dirty = false;
        }
    }

    pub fn dirty_views(&self) -> Vec<ViewId> {
        self.views.values().filter(|v| v.dirty).map(|v| v.id).collect()
    }

    pub fn all_views(&self) -> Vec<ViewId> {
        self.views.keys().copied().collect()
    }

    pub fn count(&self) -> usize {
        self.views.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_returns_incrementing_ids() {
        let mut vm = ViewManager::new();
        let a = vm.create("https://a.com");
        let b = vm.create("https://b.com");
        assert_ne!(a, b);
        assert_eq!(a, ViewId(1));
        assert_eq!(b, ViewId(2));
    }

    #[test]
    fn get_returns_created_view() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://test.com");
        let view = vm.get(id).unwrap();
        assert_eq!(view.url, "https://test.com");
        assert_eq!(view.title, "");
    }

    #[test]
    fn remove_returns_view_and_removes_it() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://test.com");
        assert_eq!(vm.count(), 1);
        let view = vm.remove(id).unwrap();
        assert_eq!(view.url, "https://test.com");
        assert_eq!(vm.count(), 0);
        assert!(vm.get(id).is_none());
    }

    #[test]
    fn dirty_tracking() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://test.com");
        assert!(vm.dirty_views().contains(&id)); // new views are dirty
        vm.clear_dirty(id);
        assert!(vm.dirty_views().is_empty());
        vm.mark_dirty(id);
        assert!(vm.dirty_views().contains(&id));
    }
}
