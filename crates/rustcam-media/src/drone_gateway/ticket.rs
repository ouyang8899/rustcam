use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 一次性 WS 票据：60s 有效，核销即失效。
///
/// 票据同时携带签发时确定的消息类型范围（scope），WebSocket 升级时按路径
/// 校验，避免只凭「持有票据」就能订阅全部无人机事件流。
pub struct TicketStore {
    ttl: Duration,
    tickets: Mutex<HashMap<String, Ticket>>,
}

struct Ticket {
    expires_at: Instant,
    msg_types: Vec<String>,
}

impl TicketStore {
    pub fn new(ttl_secs: u64) -> TicketStore {
        TicketStore {
            ttl: Duration::from_secs(ttl_secs),
            tickets: Mutex::new(HashMap::new()),
        }
    }

    /// 签发票据，并绑定允许订阅的消息类型。
    pub fn issue(&self, msg_types: Vec<String>) -> String {
        // 用 uuid v4 生成票据（uuid 已是依赖，且 v4 feature 开启）
        let ticket = uuid::Uuid::new_v4().simple().to_string();
        let now = Instant::now();
        let mut map = self.tickets.lock().unwrap();
        map.retain(|_, t| t.expires_at > now);
        map.insert(
            ticket.clone(),
            Ticket {
                expires_at: now + self.ttl,
                msg_types,
            },
        );
        ticket
    }

    /// 核销票据；成功返回该票据允许的消息类型，已使用/不存在/过期返回 `None`。
    pub fn consume(&self, ticket: &str) -> Option<Vec<String>> {
        let mut map = self.tickets.lock().unwrap();
        match map.remove(ticket) {
            Some(t) if t.expires_at > Instant::now() => Some(t.msg_types),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_is_single_use() {
        let store = TicketStore::new(60);
        let t = store.issue(vec!["telemetry".into()]);
        assert!(store.consume(&t).is_some());
        assert!(store.consume(&t).is_none(), "票据只能核销一次");
    }

    #[test]
    fn unknown_ticket_is_rejected() {
        let store = TicketStore::new(60);
        assert!(store.consume("nope").is_none());
    }

    #[test]
    fn consumed_ticket_carries_msg_type_scope() {
        let store = TicketStore::new(60);
        let t = store.issue(vec!["telemetry".into(), "alert".into()]);
        let scope = store.consume(&t).unwrap();
        assert!(scope.contains(&"telemetry".to_string()));
        assert!(scope.contains(&"alert".to_string()));
        assert!(!scope.contains(&"control".to_string()));
    }

    #[test]
    fn expired_ticket_is_rejected() {
        let store = TicketStore::new(0);
        let t = store.issue(vec!["telemetry".into()]);
        std::thread::sleep(Duration::from_millis(5));
        assert!(store.consume(&t).is_none(), "过期票据必须被拒绝");
    }
}
