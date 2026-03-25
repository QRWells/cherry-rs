use std::sync::Arc;

use crate::{BackendId, ErasedRenderBackend};

pub type BackendFactory = Arc<dyn Fn() -> Box<dyn ErasedRenderBackend> + Send + Sync>;

pub struct BackendRegistry {
    factories: std::collections::HashMap<BackendId, BackendFactory>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self {
            factories: std::collections::HashMap::new(),
        }
    }

    pub fn register_factory(&mut self, backend_id: BackendId, factory: BackendFactory) {
        self.factories.insert(backend_id, factory);
    }

    pub fn contains(&self, backend_id: &BackendId) -> bool {
        self.factories.contains_key(backend_id)
    }

    pub fn create(&self, backend_id: &BackendId) -> Option<Box<dyn ErasedRenderBackend>> {
        self.factories.get(backend_id).map(|factory| factory())
    }

    pub fn list_ids(&self) -> Vec<BackendId> {
        let mut ids = self.factories.keys().cloned().collect::<Vec<_>>();
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        ids
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}
