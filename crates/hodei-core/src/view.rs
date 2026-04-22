use std::collections::HashMap;
use crate::types::ViewId;

pub struct View {
    pub id: ViewId,
    pub url: String,
    pub title: String,
    pub dirty: bool,
    pub project_override: Option<String>,
}

pub struct ViewManager {
    views: HashMap<ViewId, View>,
    next_id: u64,
}

impl ViewManager {
    pub fn new() -> Self {
        log::debug!("ViewManager::new");
        Self {
            views: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn next_id(&self) -> ViewId {
        ViewId(self.next_id)
    }

    pub fn create(&mut self, url: &str) -> ViewId {
        let id = ViewId(self.next_id);
        self.next_id += 1;
        log::info!("ViewManager::create: id={:?} url={} (total views: {})", id, url, self.views.len() + 1);
        self.views.insert(id, View {
            id,
            url: url.to_string(),
            title: String::new(),
            dirty: true,
            project_override: None,
        });
        id
    }

    pub fn create_with_id(&mut self, id: ViewId, url: &str) {
        log::info!("ViewManager::create_with_id: id={:?} url={}", id, url);
        self.views.insert(id, View {
            id,
            url: url.to_string(),
            title: String::new(),
            dirty: true,
            project_override: None,
        });
        if id.0 >= self.next_id {
            self.next_id = id.0 + 1;
        }
    }

    pub fn remove(&mut self, id: ViewId) -> Option<View> {
        log::info!("ViewManager::remove: id={:?} (remaining will be: {})", id, self.views.len().saturating_sub(1));
        self.views.remove(&id)
    }

    pub fn get(&self, id: ViewId) -> Option<&View> {
        self.views.get(&id)
    }

    pub fn get_mut(&mut self, id: ViewId) -> Option<&mut View> {
        self.views.get_mut(&id)
    }

    pub fn mark_dirty(&mut self, id: ViewId) {
        log::trace!("ViewManager::mark_dirty: id={:?}", id);
        if let Some(v) = self.views.get_mut(&id) {
            v.dirty = true;
        }
    }

    pub fn clear_dirty(&mut self, id: ViewId) {
        log::trace!("ViewManager::clear_dirty: id={:?}", id);
        if let Some(v) = self.views.get_mut(&id) {
            v.dirty = false;
        }
    }

    pub fn dirty_views(&self) -> Vec<ViewId> {
        let ids: Vec<ViewId> = self.views.values().filter(|v| v.dirty).map(|v| v.id).collect();
        log::trace!("ViewManager::dirty_views: count={}", ids.len());
        ids
    }

    pub fn all_views(&self) -> Vec<ViewId> {
        let ids: Vec<ViewId> = self.views.keys().copied().collect();
        log::trace!("ViewManager::all_views: count={}", ids.len());
        ids
    }

    pub fn count(&self) -> usize {
        self.views.len()
    }
}

pub fn effective_project<'a>(view: Option<&'a View>, workspace_project: Option<&'a str>) -> Option<&'a str> {
    if let Some(v) = view {
        if let Some(p) = v.project_override.as_deref() { return Some(p); }
    }
    workspace_project
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
    fn create_with_id_preserves_id() {
        let mut vm = ViewManager::new();
        vm.create_with_id(ViewId(42), "https://test.com");
        let view = vm.get(ViewId(42)).unwrap();
        assert_eq!(view.url, "https://test.com");
        assert_eq!(view.id, ViewId(42));
        let auto = vm.create("https://other.com");
        assert_eq!(auto, ViewId(43));
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

    #[test]
    fn effective_project_prefers_override() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://x.com");
        vm.get_mut(id).unwrap().project_override = Some("override".into());
        let v = vm.get(id);
        assert_eq!(effective_project(v, Some("workspace")), Some("override"));
    }

    #[test]
    fn effective_project_falls_back_to_workspace() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://x.com");
        let v = vm.get(id);
        assert_eq!(effective_project(v, Some("workspace")), Some("workspace"));
    }

    #[test]
    fn effective_project_none_when_neither_set() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://x.com");
        let v = vm.get(id);
        assert_eq!(effective_project(v, None), None);
    }
}
