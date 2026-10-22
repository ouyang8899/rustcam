use once_cell::sync::OnceCell;
use std::sync::Arc;
use crate::context::ServiceRegistry;

static GLOBAL_REGISTRY: OnceCell<Arc<ServiceRegistry>> = OnceCell::new();

pub fn init_registry(registry: Arc<ServiceRegistry>) {
    GLOBAL_REGISTRY
        .set(registry)
        .map_err(|_| tracing::warn!("[Registry] Already initialized"))
        .ok();
}

pub fn registry() -> Arc<ServiceRegistry> {
    GLOBAL_REGISTRY
        .get()
        .expect("ServiceRegistry not initialized")
        .clone()
}

pub fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&ServiceRegistry) -> R,
{
    let reg = registry();
    f(&reg)
}