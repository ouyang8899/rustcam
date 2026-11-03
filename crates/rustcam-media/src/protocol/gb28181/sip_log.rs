//! Full SIP message logging.
//!
//! Every SIP message that crosses the wire can be dumped with its full
//! content:
//! `sip: receive from <source> -> to <destination>: full receive message`
//! followed by the raw message, and `sip: send to <destination>: full send
//! message` for the outgoing direction.
//!
//! The dump is disabled by default and switched on with `log.sip = true` in
//! the configuration file.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables or disables the SIP message dump.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether the SIP message dump is enabled.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn addr_str(addr: Option<SocketAddr>) -> String {
    match addr {
        Some(a) => a.to_string(),
        None => "unknown".to_string(),
    }
}

/// Formats a received SIP message for the log.
pub fn format_received(
    source: Option<SocketAddr>,
    destination: Option<SocketAddr>,
    raw: &str,
) -> String {
    format!(
        "sip: receive from {} -> to {}: full receive message\n{}",
        addr_str(source),
        addr_str(destination),
        raw
    )
}

/// Formats a sent SIP message for the log.
pub fn format_sent(destination: Option<SocketAddr>, raw: &str) -> String {
    format!("sip: send to {}: full send message\n{}", addr_str(destination), raw)
}

/// Logs a received SIP message with its full content.
pub fn log_received(source: Option<SocketAddr>, destination: Option<SocketAddr>, raw: &str) {
    if !is_enabled() {
        return;
    }
    tracing::info!("{}", format_received(source, destination, raw));
}

/// Logs a sent SIP message with its full content.
pub fn log_sent(destination: Option<SocketAddr>, raw: &str) {
    if !is_enabled() {
        return;
    }
    tracing::info!("{}", format_sent(destination, raw));
}

/// Buffer the test subscriber writes to.
#[cfg(test)]
pub(crate) fn log_capture() -> std::sync::Arc<std::sync::Mutex<Vec<u8>>> {
    use std::io::Write;
    use std::sync::Arc;

    #[derive(Clone)]
    struct SharedWriter(Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
        type Writer = SharedWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    static BUFFER: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Vec<u8>>>> =
        std::sync::OnceLock::new();
    static INIT: std::sync::Once = std::sync::Once::new();

    let buffer = BUFFER
        .get_or_init(|| std::sync::Arc::new(std::sync::Mutex::new(Vec::new())))
        .clone();
    INIT.call_once(|| {
        let writer = SharedWriter(buffer.clone());
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(writer)
                .with_max_level(tracing::Level::INFO)
                .finish(),
        );
    });
    buffer
}

/// Serializes the tests that share the global log buffer.
#[cfg(test)]
pub(crate) fn log_capture_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_is_disabled_by_default() {
        let _guard = log_capture_guard();
        assert!(!is_enabled(), "the SIP dump must be off by default");
    }

    #[test]
    fn test_enabled_logging_is_written() {
        let _guard = log_capture_guard();
        set_enabled(true);

        let buffer = log_capture();
        buffer.lock().unwrap().clear();

        log_sent(
            Some("192.168.1.211:5060".parse().unwrap()),
            "OPTIONS sip:x SIP/2.0\r\n\r\n",
        );

        let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        set_enabled(false);
        assert!(
            logs.contains("sip: send to 192.168.1.211:5060: full send message"),
            "the message must be logged when the dump is enabled, got:\n{}",
            logs
        );
    }

    #[test]
    fn test_disabled_logging_writes_nothing() {
        let _guard = log_capture_guard();
        set_enabled(false);

        let buffer = log_capture();
        buffer.lock().unwrap().clear();

        log_sent(
            Some("192.168.1.211:5060".parse().unwrap()),
            "OPTIONS sip:x SIP/2.0\r\n\r\n",
        );

        let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert!(
            !logs.contains("full send message"),
            "nothing must be logged while the dump is disabled, got:\n{}",
            logs
        );
    }

    #[test]
    fn test_format_received_contains_both_addresses_and_the_message() {
        let source: SocketAddr = "192.168.1.211:5060".parse().unwrap();
        let destination: SocketAddr = "192.168.1.7:5061".parse().unwrap();
        let raw = "REGISTER sip:31011500001000000001@3101150000 SIP/2.0\r\nCSeq: 1 REGISTER\r\n\r\n";

        let formatted = format_received(Some(source), Some(destination), raw);

        assert!(
            formatted.starts_with(
                "sip: receive from 192.168.1.211:5060 -> to 192.168.1.7:5061: full receive message\n"
            ),
            "unexpected header: {}",
            formatted
        );
        assert!(
            formatted.ends_with(raw),
            "the full message must be part of the log: {}",
            formatted
        );
    }

    #[test]
    fn test_format_sent_contains_the_destination_and_the_message() {
        let destination: SocketAddr = "192.168.1.211:5060".parse().unwrap();
        let raw = "SIP/2.0 200 OK\r\nCSeq: 1 REGISTER\r\n\r\n";

        let formatted = format_sent(Some(destination), raw);

        assert!(
            formatted.starts_with("sip: send to 192.168.1.211:5060: full send message\n"),
            "unexpected header: {}",
            formatted
        );
        assert!(
            formatted.ends_with(raw),
            "the full message must be part of the log: {}",
            formatted
        );
    }

    #[test]
    fn test_unknown_addresses_are_reported() {
        let formatted = format_received(None, None, "OPTIONS sip:x SIP/2.0\r\n\r\n");
        assert!(
            formatted.starts_with("sip: receive from unknown -> to unknown: full receive message\n"),
            "unexpected header: {}",
            formatted
        );

        let formatted = format_sent(None, "OPTIONS sip:x SIP/2.0\r\n\r\n");
        assert!(
            formatted.starts_with("sip: send to unknown: full send message\n"),
            "unexpected header: {}",
            formatted
        );
    }
}
