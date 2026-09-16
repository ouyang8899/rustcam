use crate::domain::device::Protocol;
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::traits::ProtocolDeps;
use std::sync::Arc;
use tokio::sync::Mutex;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::RwLock;

pub type AdapterEntry = Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>;
pub type AdapterFactory = Arc<dyn Fn(ProtocolDeps, &str) -> Box<dyn SignalAdapter> + Send + Sync>;

static ADAPTER_MAP: std::sync::LazyLock<DashMap<String, AdapterEntry>> =
    std::sync::LazyLock::new(|| DashMap::new());

static ADAPTER_FACTORIES: std::sync::LazyLock<RwLock<HashMap<Protocol, AdapterFactory>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

static DEFAULT_DEPS: std::sync::LazyLock<RwLock<Option<ProtocolDeps>>> =
    std::sync::LazyLock::new(|| RwLock::new(None));

#[inline]
pub fn get_adapter(key: &str) -> Option<AdapterEntry> {
    ADAPTER_MAP.get(key).map(|r| r.value().clone())
}

#[inline]
pub fn set_adapter(key: String, adapter: AdapterEntry) {
    ADAPTER_MAP.insert(key, adapter);
}

#[inline]
pub fn remove_adapter(key: &str) {
    ADAPTER_MAP.remove(key);
}

#[inline]
pub fn clear_adapters() {
    ADAPTER_MAP.clear();
}

pub fn register_adapter_factory(protocol: Protocol, factory: AdapterFactory) {
    ADAPTER_FACTORIES.write().unwrap().insert(protocol, factory);
}

pub fn set_default_deps(deps: ProtocolDeps) {
    *DEFAULT_DEPS.write().unwrap() = Some(deps);
}

pub fn get_or_create_adapter(protocol: &Protocol, key: &str) -> Option<AdapterEntry> {
    let full_key = format!("{:?}_{}", protocol, key);

    use dashmap::mapref::entry::Entry;
    match ADAPTER_MAP.entry(full_key.clone()) {
        Entry::Occupied(entry) => Some(entry.get().clone()),
        Entry::Vacant(entry) => {
            let deps = DEFAULT_DEPS.read().unwrap().clone()?;
            let factory = ADAPTER_FACTORIES.read().unwrap().get(protocol)?.clone();
            let adapter: Box<dyn SignalAdapter + Send + Sync> = factory(deps, key);
            let adapter_arc: AdapterEntry = Arc::new(Mutex::new(adapter));
            entry.insert(adapter_arc.clone());
            Some(adapter_arc)
        }
    }
}

pub async fn cleanup_expired_subscriptions_all() -> usize {
    let mut total = 0;
    let keys: Vec<String> = ADAPTER_MAP.iter().map(|r| r.key().clone()).collect();
    for key in keys {
        if let Some(adapter) = get_adapter(&key) {
            let guard = adapter.lock().await;
            total += guard.cleanup_expired_subscriptions().await;
        }
    }
    total
}
