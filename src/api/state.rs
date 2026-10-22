use std::sync::Arc;
use crate::context::ServiceRegistry;
use crate::auth::AuthState;

#[derive(Clone)]
pub struct FullState {
    pub app: AppState,
    pub auth: AuthState,
}

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<ServiceRegistry>,
}

impl AppState {
    pub fn from_registry(registry: &Arc<ServiceRegistry>) -> Self {
        Self {
            registry: registry.clone(),
        }
    }
}