use std::sync::Arc;
use tokio::sync::RwLock;

use crate::context::{InfraContext, MediaContext};
use crate::domain::{Session, SessionType, SessionState};
use crate::adapter::media_server::StreamInfo;
use crate::domain::traits::CacheStore;
use crate::error::{AppError, Result};
use crate::protocol::event::SignalEvent;

pub struct SessionService {
    infra: InfraContext,
    media: MediaContext,
    shutdown_tx: Arc<RwLock<Option<tokio::sync::watch::Sender<()>>>>,
}

impl SessionService {
    pub fn new(infra: InfraContext, media: MediaContext) -> Self {
        Self {
            infra,
            media,
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self, expiration_secs: i64) {
        let (tx, _rx) = tokio::sync::watch::channel(());
        *self.shutdown_tx.write().await = Some(tx);

        let infra = self.infra.clone();
        let media = self.media.clone();
        let mut event_rx = self.infra.subscribe_events();

        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        if let Err(e) = Self::handle_event(&infra, &media, &event).await {
                            tracing::error!("[SessionService] Event handler error: {}", e);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("[SessionService] Lagged {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let repo = self.infra.db.clone();
        let redis = self.infra.redis.clone();
        let cluster = self.media.cluster.clone();
        let exp_secs = expiration_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                Self::cleanup_expired_sessions(&repo, &redis, &cluster, exp_secs).await;
            }
        });

    }

    pub async fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.read().await.as_ref() {
            let _ = tx.send(());
        }
    }

    async fn handle_event(
        infra: &InfraContext,
        _media: &MediaContext,
        event: &SignalEvent,
    ) -> anyhow::Result<()> {
        match event {
            SignalEvent::StartPlay { device_tag, session_id: _, channel_id, session_type, .. } => {
                if *session_type == SessionType::Broadcast {
                    return Ok(());
                }
                let mut session = Session::new(*session_type, 0i64);
                session.device_tag = device_tag.clone();
                session.channel_tag = channel_id.clone();
                match infra.db.create_session(&session).await {
                    Ok(id) => {
                        session.id = id;
                        infra.db.sessions_cache().insert(session.id, session);
                    }
                    Err(e) => {
                        tracing::error!("[SessionService] Failed to persist session: {}", e);
                    }
                }
            }

            SignalEvent::StopPlay { session_id, session_type, .. } => {
                if *session_type == SessionType::Broadcast {
                    return Ok(());
                }

                let target_id = {
                    let cache = infra.db.sessions_cache();
                    cache
                        .iter()
                        .find(|s| s.stream_key() == *session_id)
                        .map(|s| s.value().id)
                };
                if let Some(target_id) = target_id {
                    infra.db.sessions_cache().remove(&target_id);
                    if let Err(e) = infra.db.delete_session(target_id).await {
                        tracing::error!("[SessionService] Failed to delete session {}: {}", target_id, e);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn create_session(&self, session_type: SessionType, user_id: i64) -> Result<Session> {
        let mut session = Session::new(session_type, user_id);
        match self.infra.db.create_session(&session).await {
            Ok(id) => {
                session.id = id;
                self.infra.db.sessions_cache().insert(session.id, session.clone());
            }
            Err(e) => {
                tracing::error!("[SessionService] Failed to persist session: {}", e);
            }
        }
        Ok(session)
    }

    pub async fn activate_session(&self, id: i64, rtsp_url: &str) -> Result<StreamInfo> {
        let session = self.get_session(id)?
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))?;

        let server = self.media.cluster.select_server().await
            .ok_or_else(|| AppError::MediaServerError("No available media server".to_string()))?;

        let device_tag = session.device_tag.as_ref()
            .ok_or_else(|| AppError::NotFound("Session has no device_tag".to_string()))?;
        let device = self.infra.db.get_device_by_device_tag(device_tag)
            .ok_or_else(|| AppError::NotFound(format!("Device with tag {} not found", device_tag)))?;
        let app = device.app.unwrap_or_else(|| "live".to_string());

        let stream_key = crate::common::device_utils::format_stream_key(
            session.device_tag.as_deref().unwrap_or(""),
            session.channel_tag.as_deref().unwrap_or("")
        );
        let stream_info = server.add_stream_proxy(&app, &stream_key, rtsp_url).await?;

        self.infra.redis.set_stream_info(&stream_key, &stream_info).await?;

        let mut session = session;
        session.set_active(server.name().to_string());
        self.infra.db.sessions_cache().insert(id, session);

        Ok(stream_info)
    }

    pub async fn deactivate_session(&self, id: i64) -> Result<()> {
        let session = self.get_session(id)?
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))?;

        let stream_key = session.stream_key();
        if !stream_key.is_empty() {
            if let Some(server_id) = &session.media_server_tag {
                if let Some(server) = self.media.cluster.get_all_servers().iter().find(|s| s.name() == server_id) {
                    if let Err(e) = server.remove_stream_proxy("live", &stream_key).await {
                        tracing::warn!("[SessionService] Failed to remove stream: {}", e);
                    }
                }
            }
            if let Err(e) = self.infra.redis.delete_stream_info(&stream_key).await {
                tracing::warn!("[SessionService] Failed to delete stream info: {}", e);
            }
        }

        let mut session = session;
        session.terminate();
        self.infra.db.sessions_cache().insert(id, session.clone());
        // The state has to be persisted, the database would keep the old
        // state and the session would show up as running forever.
        if let Err(e) = self.infra.db.update_session_state(&session).await {
            tracing::error!("[SessionService] Failed to persist session state: {}", e);
        }
        Ok(())
    }

    pub fn get_session(&self, id: i64) -> Result<Option<Session>> {
        Ok(self.infra.db.sessions_cache().get(&id).map(|s| s.clone()))
    }

    pub fn get_active_count(&self) -> usize {
        self.infra.db.sessions_cache().iter()
            .filter(|s| s.state == SessionState::Active)
            .count()
    }

    /// Number of sessions that are not terminated yet.
    pub fn get_live_count(&self) -> usize {
        self.infra.db.sessions_cache().iter()
            .filter(|s| s.state != SessionState::Terminated)
            .count()
    }

    pub async fn list_sessions_paginated(&self, search: Option<&str>, state: Option<&str>, limit: usize, offset: usize) -> Vec<Session> {
        self.infra.db.list_sessions_paginated(search, state, limit, offset).await
    }

    pub async fn count_sessions(&self, search: Option<&str>, state: Option<&str>) -> usize {
        self.infra.db.count_sessions(search, state).await
    }

    async fn cleanup_expired_sessions(
        repo: &Arc<crate::infrastructure::DbRepository>,
        redis: &Arc<crate::infrastructure::RedisStore>,
        cluster: &Arc<crate::infrastructure::cluster::ClusterManager>,
        expiration_secs: i64,
    ) {
        let mut to_delete = Vec::new();

        let expired: Vec<(i64, String, Option<String>)> = {
            let cache = repo.sessions_cache();
            let sessions: Vec<Session> = cache.iter().map(|e| e.value().clone()).collect();
            let expired_ids = select_expired_sessions(&sessions, expiration_secs);

            cache
                .iter()
                .filter(|e| expired_ids.contains(&e.value().id))
                .map(|e| {
                    let s = e.value();
                    (s.id, s.stream_key(), s.media_server_tag.clone())
                })
                .collect()
        };

        for (session_id, stream_key, media_server_tag) in expired {
            if !stream_key.is_empty() {
                if let Some(server_id) = &media_server_tag {
                    if let Some(server) = cluster.get_all_servers().iter().find(|s| s.name() == server_id) {
                        let _ = server.remove_stream_proxy("live", &stream_key).await;
                    }
                    let _ = redis.delete_stream_info(&stream_key).await;
                }
            }
            let updated = {
                let cache = repo.sessions_cache();
                cache.get(&session_id).map(|s| {
                    let mut s = s.clone();
                    s.mark_terminated();
                    s
                })
            };
            if let Some(s) = updated {
                repo.sessions_cache().insert(s.id, s);
            }
            to_delete.push(session_id);
        }

        for session_id in to_delete {
            if let Err(e) = repo.delete_session(session_id).await {
                tracing::error!("[SessionService] Failed to delete expired session {}: {}", session_id, e);
            }
        }
    }
}

impl Clone for SessionService {
    fn clone(&self) -> Self {
        Self {
            infra: self.infra.clone(),
            media: self.media.clone(),
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }
}

/// A session is expired when its expiry time passed. Sessions that never got
/// an expiry time - they were created but never activated - expire when they
/// did not see any activity for longer than the configured window. Without
/// this fallback those sessions are never reaped and pile up in the database.
pub fn is_session_expired(session: &Session, expiration_secs: i64) -> bool {
    if session.expires_at.is_some() {
        return session.is_expired();
    }
    let idle = chrono::Utc::now() - session.last_activity;
    idle.num_seconds() > expiration_secs
}

/// Selects the sessions that have to be reaped: expired ones and sessions
/// that are not terminated yet but did not see any activity for longer than
/// the configured window.
pub fn select_expired_sessions(sessions: &[Session], expiration_secs: i64) -> Vec<i64> {
    sessions
        .iter()
        .filter(|s| s.state != SessionState::Terminated && is_session_expired(s, expiration_secs))
        .map(|s| s.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SessionType;

    fn zombie() -> Session {
        // A session that was created but never activated: no expiry time.
        let mut session = Session::new(SessionType::Play, 0);
        session.id = 128;
        session.last_activity = chrono::Utc::now() - chrono::TimeDelta::seconds(600);
        session
    }

    #[test]
    fn test_fresh_session_without_expiry_is_not_expired() {
        let session = Session::new(SessionType::Play, 0);
        assert!(
            !is_session_expired(&session, 300),
            "a session that was just created must not be expired"
        );
    }

    #[test]
    fn test_session_without_expiry_expires_after_the_configured_window() {
        let mut session = Session::new(SessionType::Play, 0);
        session.last_activity = chrono::Utc::now() - chrono::TimeDelta::seconds(600);

        assert!(
            is_session_expired(&session, 300),
            "a session that never got an expiry must expire after the idle window"
        );
        assert!(
            !is_session_expired(&session, 3600),
            "the idle window must be respected"
        );
    }

    #[test]
    fn test_session_with_expiry_uses_its_expiry_time() {
        let mut session = Session::new(SessionType::Play, 0);
        session.expires_at = Some(chrono::Utc::now() - chrono::TimeDelta::seconds(10));
        assert!(is_session_expired(&session, 300));

        let mut session = Session::new(SessionType::Play, 0);
        session.expires_at = Some(chrono::Utc::now() + chrono::TimeDelta::seconds(600));
        assert!(!is_session_expired(&session, 300));
    }

    #[test]
    fn test_zombie_sessions_are_selected_for_cleanup() {
        let mut fresh = Session::new(SessionType::Play, 0);
        fresh.id = 1;

        let mut terminated = zombie();
        terminated.id = 2;
        terminated.mark_terminated();

        let sessions = vec![fresh, zombie(), terminated];

        let expired = select_expired_sessions(&sessions, 300);

        assert_eq!(
            expired,
            vec![128],
            "only the zombie session must be reaped, terminated and fresh ones stay"
        );
    }
}
