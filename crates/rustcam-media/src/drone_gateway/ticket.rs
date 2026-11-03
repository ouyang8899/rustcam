use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 一次性 WS 票据：60s 有效，核销即失效。
pub struct TicketStore {
    ttl: Duration,
    tickets: Mutex<HashMap<String, Instant>>,
}

impl TicketStore {
    pub fn new(ttl_secs: u64) -> TicketStore {
        TicketStore {
            ttl: Duration::from_secs(ttl_secs),
            tickets: Mutex::new(HashMap::new()),
        }
    }

    pub fn issue(&self) -> String {
        // 用 uuid v4 生成票据（uuid 已是依赖，且 v4 feature 开启）
        let ticket = uuid::Uuid::new_v4().simple().to_string();
        let now = Instant::now();
        let mut map = self.tickets.lock().unwrap();
        map.retain(|_, exp| *exp > now);
        map.insert(ticket.clone(), now + self.ttl);
        ticket
    }

    pub fn consume(&self, ticket: &str) -> bool {
        let mut map = self.tickets.lock().unwrap();
        match map.remove(ticket) {
            Some(exp) => exp > Instant::now(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_is_single_use() {
        let store = TicketStore::new(60);
        let t = store.issue();
        assert!(store.consume(&t));
        assert!(!store.consume(&t), "票据只能核销一次");
    }

    #[test]
    fn unknown_ticket_is_rejected() {
        let store = TicketStore::new(60);
        assert!(!store.consume("nope"));
    }
}
