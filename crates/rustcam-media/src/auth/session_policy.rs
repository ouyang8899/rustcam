use uuid::Uuid;

use super::config::AuthConfig;
use super::models::UserSession;

pub enum LimitDecision {
    Allow,
    Evict(Vec<Uuid>),
    Reject(String),
}

pub fn decide_limit(active: &[UserSession], client_type: &str, cfg: &AuthConfig) -> LimitDecision {
    let mut sorted: Vec<&UserSession> = active.iter().collect();
    sorted.sort_by_key(|s| (s.created_at, s.id));

    let mut evict: Vec<Uuid> = Vec::new();

    if cfg.max_sessions_per_user > 0 && sorted.len() + 1 > cfg.max_sessions_per_user {
        let excess = sorted.len() + 1 - cfg.max_sessions_per_user;
        evict.extend(sorted.iter().take(excess).map(|s| s.id));
    }

    if cfg.max_sessions_per_client_type > 0 {
        let same_type: Vec<&UserSession> = sorted
            .iter()
            .copied()
            .filter(|s| s.client_type == client_type)
            .collect();
        if same_type.len() + 1 > cfg.max_sessions_per_client_type {
            let excess = same_type.len() + 1 - cfg.max_sessions_per_client_type;
            evict.extend(same_type.iter().take(excess).map(|s| s.id));
        }
    }

    evict.sort_unstable();
    evict.dedup();

    if evict.is_empty() {
        return LimitDecision::Allow;
    }

    if cfg.session_limit_policy == "reject" {
        let per_type_exceeded = cfg.max_sessions_per_client_type > 0
            && active
                .iter()
                .filter(|s| s.client_type == client_type)
                .count()
                + 1
                > cfg.max_sessions_per_client_type;
        let message = if per_type_exceeded {
            format!("Session limit exceeded for client type '{}'", client_type)
        } else {
            "Account session limit exceeded".to_string()
        };
        return LimitDecision::Reject(message);
    }

    LimitDecision::Evict(evict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn session(seconds: i64, client_type: &str) -> UserSession {
        let ts = DateTime::from_timestamp(seconds, 0).unwrap().with_timezone(&Utc);
        UserSession {
            id: Uuid::new_v4(),
            user_id: Uuid::nil(),
            client_type: client_type.to_string(),
            client_name: None,
            client_ip: None,
            user_agent: None,
            created_at: ts,
            last_seen_at: ts,
        }
    }

    fn config(max_user: usize, max_type: usize, policy: &str) -> AuthConfig {
        AuthConfig {
            max_sessions_per_user: max_user,
            max_sessions_per_client_type: max_type,
            session_limit_policy: policy.to_string(),
            ..AuthConfig::default()
        }
    }

    #[test]
    fn unlimited_allows() {
        let active = vec![session(1, "web"), session(2, "web")];
        assert!(matches!(
            decide_limit(&active, "web", &config(0, 0, "evict")),
            LimitDecision::Allow
        ));
    }

    #[test]
    fn evicts_oldest_when_user_limit_exceeded() {
        let oldest = session(1, "web");
        let middle = session(2, "desktop");
        let newest = session(3, "web");
        let active = vec![newest.clone(), oldest.clone(), middle.clone()];
        match decide_limit(&active, "web", &config(3, 0, "evict")) {
            LimitDecision::Evict(ids) => assert_eq!(ids, vec![oldest.id]),
            _ => panic!("expected eviction"),
        }
    }

    #[test]
    fn evicts_oldest_of_same_type() {
        let old_web = session(1, "web");
        let old_desktop = session(2, "desktop");
        let new_web = session(3, "web");
        let active = vec![old_web.clone(), old_desktop.clone(), new_web.clone()];
        match decide_limit(&active, "web", &config(0, 2, "evict")) {
            LimitDecision::Evict(ids) => assert_eq!(ids, vec![old_web.id]),
            _ => panic!("expected eviction"),
        }
    }

    #[test]
    fn reject_reports_type_limit() {
        let active = vec![session(1, "web")];
        match decide_limit(&active, "web", &config(0, 1, "reject")) {
            LimitDecision::Reject(message) => assert!(message.contains("client type")),
            _ => panic!("expected reject"),
        }
    }

    #[test]
    fn reject_reports_account_limit() {
        let active = vec![session(1, "web"), session(2, "desktop")];
        match decide_limit(&active, "web", &config(2, 0, "reject")) {
            LimitDecision::Reject(message) => assert!(message.contains("Account")),
            _ => panic!("expected reject"),
        }
    }
}
