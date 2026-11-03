use std::sync::Arc;
use crate::context::ServiceRegistry;
use crate::auth::AuthState;
use crate::drone_gateway::client::{DroneApi, DroneClient};

#[derive(Clone)]
pub struct FullState {
    pub app: AppState,
    pub auth: AuthState,
}

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<ServiceRegistry>,
    /// drone-service 客户端；未配置时为 None（drone 接口返回 503）
    pub drone: Option<Arc<dyn DroneApi>>,
}

impl AppState {
    pub fn from_registry(registry: &Arc<ServiceRegistry>) -> Self {
        Self {
            registry: registry.clone(),
            drone: None,
        }
    }

    pub fn with_drone(mut self, client: Arc<DroneClient>) -> Self {
        self.drone = Some(client);
        self
    }
}
