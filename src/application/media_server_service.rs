use std::sync::Arc;
use dashmap::DashMap;

use crate::config::MediaServerConfig;
use crate::domain::server::{Server, ServerProtocolPorts, ServerType};
use crate::infrastructure::DbRepository;
use crate::infrastructure::cluster::ClusterManager;

#[derive(Debug, Clone)]
pub struct ManagedServer {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub api_key: String,
    pub server_type: ServerType,
    pub weight: u32,
    pub enabled: bool,
    pub server_tag: String,
    pub protocol_ports: ServerProtocolPorts,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ManagedServer {
    pub fn new(
        name: String,
        url: String,
        api_key: String,
        server_type: ServerType,
        weight: u32,
        server_tag: String,
        protocol_ports: ServerProtocolPorts,
    ) -> Self {
        Self {
            id: 0,
            name,
            url,
            api_key,
            server_type,
            weight,
            enabled: true,
            server_tag,
            protocol_ports,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn to_domain(&self) -> Server {
        Server {
            id: self.id,
            name: self.name.clone(),
            url: self.url.clone(),
            api_key: self.api_key.clone(),
            server_type: self.server_type,
            weight: self.weight,
            enabled: self.enabled,
            server_tag: self.server_tag.clone(),
            protocol_ports: self.protocol_ports.clone(),
            created_at: self.created_at,
            updated_at: chrono::Utc::now(),
        }
    }

    pub fn from_domain(s: Server) -> Self {
        Self {
            id: s.id,
            name: s.name,
            url: s.url,
            api_key: s.api_key,
            server_type: s.server_type,
            weight: s.weight,
            enabled: s.enabled,
            server_tag: s.server_tag,
            protocol_ports: s.protocol_ports,
            created_at: s.created_at,
        }
    }

    pub fn to_config(&self) -> MediaServerConfig {
        MediaServerConfig {
            name: self.name.clone(),
            url: self.url.clone(),
            api_key: self.api_key.clone(),
            server_type: self.server_type.to_string(),
            weight: self.weight,
            enabled: self.enabled,
            server_tag: self.server_tag.clone(),
            protocol_ports: self.protocol_ports.clone(),
        }
    }
}

pub struct MediaServerService {

    servers: Arc<DashMap<String, ManagedServer>>,

    repo: Arc<DbRepository>,

    cluster: Arc<ClusterManager>,

    config_servers: Vec<crate::config::MediaServerConfig>,
}

impl MediaServerService {

    pub fn new(
        repo: Arc<DbRepository>,
        cluster: Arc<ClusterManager>,
        config_servers: Vec<crate::config::MediaServerConfig>,
    ) -> Self {
        let servers = Arc::new(DashMap::new());
        let config_servers = config_servers;

        let _db_count = repo.servers_cache().len();

        let db_servers: Vec<_> = {
            repo.servers_cache().iter().map(|r| r.value().clone()).collect()
        };

        for db_server in db_servers {
            let tag = if db_server.server_tag.is_empty() {
                db_server.name.clone()
            } else {
                db_server.server_tag.clone()
            };

            let server_type = ServerType::from_str(&db_server.server_type)
                .unwrap_or(ServerType::Zlmediakit);

            let mut protocol_ports: ServerProtocolPorts =
                serde_json::from_value(db_server.protocol_ports.clone()).unwrap_or_default();
            if protocol_ports.is_empty() {
                if let Some(cfg) = config_servers.iter().find(|c| {
                    c.server_tag == db_server.server_tag || c.name == db_server.name
                }) {
                    protocol_ports = cfg.protocol_ports.clone();
                }
            }

            let ms = ManagedServer {
                id: db_server.id,
                name: db_server.name,
                url: db_server.url,
                api_key: db_server.api_key,
                server_type,
                weight: db_server.weight as u32,
                enabled: db_server.enabled,
                server_tag: tag,
                protocol_ports,
                created_at: chrono::DateTime::from_timestamp(
                    db_server.created_at.unix_timestamp(), 0,
                )
                    .unwrap_or_else(chrono::Utc::now),
            };

            servers.insert(ms.server_tag.clone(), ms.clone());

            if ms.enabled {
                cluster.register_server(ms.to_config());
            }
        }

        Self { servers, repo, cluster, config_servers }
    }

    pub fn list(&self) -> Vec<ManagedServer> {
        self.servers.iter().map(|r| r.value().clone()).collect()
    }

    pub fn get(&self, tag: &str) -> Option<ManagedServer> {
        self.servers.get(tag).map(|r| r.value().clone())
    }

    pub fn get_by_name(&self, name: &str) -> Option<ManagedServer> {
        self.servers
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.value().clone())
    }

    pub fn insert(&self, ms: ManagedServer) {
        self.servers.insert(ms.server_tag.clone(), ms);
    }

    pub async fn create(
        &self,
        name: String,
        url: String,
        api_key: String,
        server_type: ServerType,
        weight: u32,
        server_tag: String,
        protocol_ports: ServerProtocolPorts,
    ) -> ManagedServer {
        let mut ms = ManagedServer::new(
            name, url, api_key, server_type, weight, server_tag, protocol_ports,
        );

        let domain = ms.to_domain();
        match self.repo.create_server(&domain).await {
            Ok(id) => {
                ms.id = id;
            }
            Err(e) => {
                tracing::warn!(
                    "[MediaServerService] Failed to persist server to DB: {}",
                    e
                );
            }
        }

        self.servers.insert(ms.server_tag.clone(), ms.clone());

        if ms.enabled {
            self.cluster.register_server(ms.to_config());
        }

        ms
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        tag: &str,
        name: Option<String>,
        url: Option<String>,
        api_key: Option<String>,
        server_type: Option<ServerType>,
        weight: Option<u32>,
        enabled: Option<bool>,
        server_tag: Option<String>,
        protocol_ports: Option<ServerProtocolPorts>,
    ) -> Option<ManagedServer> {
        if let Some(mut entry) = self.servers.get_mut(tag) {

            let old_tag = entry.server_tag.clone();
            if let Some(v) = name { entry.name = v; }
            if let Some(v) = url { entry.url = v; }
            if let Some(v) = api_key { entry.api_key = v; }
            if let Some(v) = server_type { entry.server_type = v; }
            if let Some(v) = weight { entry.weight = v; }
            if let Some(v) = enabled { entry.enabled = v; }
            if let Some(v) = server_tag { entry.server_tag = v; }
            if let Some(v) = protocol_ports { entry.protocol_ports = v; }

            let updated = entry.value().clone();
            drop(entry);

            if updated.server_tag != old_tag {
                self.servers.remove(&old_tag);
                self.servers.insert(updated.server_tag.clone(), updated.clone());
            }

            let domain = updated.to_domain();
            if let Err(e) = self.repo.update_server(&domain).await {
                tracing::warn!(
                    "[MediaServerService] Failed to persist server update: {}",
                    e
                );
            }

            self.cluster.unregister_server(&updated.server_tag);
            if updated.enabled {
                self.cluster.register_server(updated.to_config());
            }

            return Some(updated);
        }

        tracing::warn!("[MediaServerService] Server not found for update: tag={}", tag);
        None
    }

    pub async fn delete(&self, tag: &str) -> bool {
        if let Some((_, ms)) = self.servers.remove(tag) {

            self.cluster.unregister_server(tag);

            if let Err(e) = self.repo.delete_server(ms.id).await {
                tracing::warn!(
                    "[MediaServerService] Failed to delete server from DB: {}",
                    e
                );
            }
            return true;
        }

        tracing::warn!("[MediaServerService] Server not found for delete: tag={}", tag);
        false
    }

    pub fn update_enabled(&self, tag: &str, enabled: bool) -> Option<ManagedServer> {
        if let Some(mut entry) = self.servers.get_mut(tag) {
            entry.enabled = enabled;
            let updated = entry.value().clone();
            drop(entry);

            return Some(updated);
        }
        None
    }

    pub fn get_all_configs(&self) -> Vec<MediaServerConfig> {
        self.servers
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.value().to_config())
            .collect()
    }

    pub fn get_enabled_servers(&self) -> Vec<ManagedServer> {
        self.servers
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.value().clone())
            .collect()
    }

    pub fn count(&self) -> usize {
        self.servers.len()
    }

    pub fn count_online(&self) -> usize {
        self.servers.iter().filter(|r| r.enabled).count()
    }

    pub fn exists(&self, tag: &str) -> bool {
        self.servers.contains_key(tag)
    }

    pub fn get_stats(&self) -> serde_json::Value {
        let total = self.servers.len();
        let enabled = self.servers.iter().filter(|r| r.enabled).count();
        let types: std::collections::HashMap<String, usize> = self
            .servers
            .iter()
            .fold(std::collections::HashMap::new(), |mut acc, r| {
                *acc.entry(r.server_type.to_string()).or_insert(0) += 1;
                acc
            });

        serde_json::json!({
            "total": total,
            "enabled": enabled,
            "disabled": total - enabled,
            "by_type": types,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_repo() -> Arc<DbRepository> {

        unimplemented!("Use test database for integration tests")
    }

    #[tokio::test]
    async fn test_create_and_get_server() {

    }
}