use std::collections::BTreeMap;

/// 规则 §14.1 规定的指标名；drone 侧只引用常量，不写字面量。
pub mod names {
    pub const ADAPTER_CONNECTIONS_TOTAL: &str = "rustcam_drone_adapter_connections_total";
    pub const ADAPTER_RECONNECT_TOTAL: &str = "rustcam_drone_adapter_reconnect_total";
    pub const ADAPTER_HEARTBEAT_AGE_SECONDS: &str = "rustcam_drone_adapter_heartbeat_age_seconds";
    pub const COMMAND_DURATION_SECONDS: &str = "rustcam_drone_command_duration_seconds";
    pub const COMMAND_INFLIGHT: &str = "rustcam_drone_command_inflight";
    pub const COMMAND_TIMEOUT_TOTAL: &str = "rustcam_drone_command_timeout_total";
    pub const TELEMETRY_LAG_SECONDS: &str = "rustcam_drone_telemetry_lag_seconds";
    pub const TELEMETRY_DEDUP_DROPPED_TOTAL: &str = "rustcam_drone_telemetry_dedup_dropped_total";
    pub const STREAM_PENDING: &str = "rustcam_drone_stream_pending";
    pub const STREAM_LENGTH: &str = "rustcam_drone_stream_length";
    pub const GEOFENCE_EVAL_DURATION_SECONDS: &str = "rustcam_drone_geofence_eval_duration_seconds";
    pub const GEOFENCE_ALERT_TOTAL: &str = "rustcam_drone_geofence_alert_total";
    pub const RISK_ALERT_TOTAL: &str = "rustcam_drone_risk_alert_total";
    pub const CONTROL_ACQUIRE_TOTAL: &str = "rustcam_drone_control_acquire_total";
    pub const CONTROL_PREEMPT_TOTAL: &str = "rustcam_drone_control_preempt_total";
    pub const WS_SUBSCRIBERS: &str = "rustcam_drone_ws_subscribers";
    pub const WS_LAGGED_TOTAL: &str = "rustcam_drone_ws_lagged_total";

    // 规则 §14.1 之外的补充指标（实现已产出，补充登记在案）。
    pub const TELEMETRY_TOTAL: &str = "rustcam_drone_telemetry_total";
    pub const TELEMETRY_STALE_TOTAL: &str = "rustcam_drone_telemetry_stale_total";
    pub const TELEMETRY_REJECTED_TOTAL: &str = "rustcam_drone_telemetry_rejected_total";
    pub const COMMAND_SENT_TOTAL: &str = "rustcam_drone_command_sent_total";
    pub const COMMAND_REPLY_TOTAL: &str = "rustcam_drone_command_reply_total";
    pub const ONLINE_DEVICES: &str = "rustcam_drone_online_devices";
    pub const ADAPTERS: &str = "rustcam_drone_adapters";
    pub const FENCES: &str = "rustcam_drone_fences";
    // 归档（时序库写入）：行数、丢弃、失败、队列深度。
    pub const ARCHIVE_ROWS_TOTAL: &str = "rustcam_drone_archive_rows_total";
    pub const ARCHIVE_DROPPED_TOTAL: &str = "rustcam_drone_archive_dropped_total";
    pub const ARCHIVE_ERRORS_TOTAL: &str = "rustcam_drone_archive_errors_total";
    pub const ARCHIVE_QUEUE_DEPTH: &str = "rustcam_drone_archive_queue_depth";
}

/// 一条时间序列：指标名 + 排序后的标签。
type SeriesKey = (String, Vec<(String, String)>);

/// 手写 Prometheus 文本输出；格式与主服务 `monitoring::Metrics` 一致（前缀 `rustcam_drone_`）。
#[derive(Debug, Default)]
pub struct MetricsText {
    counters: BTreeMap<SeriesKey, u64>,
    gauges: BTreeMap<SeriesKey, f64>,
}

impl MetricsText {
    pub fn inc(&mut self, name: &str) {
        self.inc_labeled(name, &[]);
    }

    /// 递增带标签的计数器；同名不同标签是不同的时间序列。
    pub fn inc_labeled(&mut self, name: &str, labels: &[(&str, &str)]) {
        *self.counters.entry(series_key(name, labels)).or_insert(0) += 1;
    }

    pub fn set_gauge(&mut self, name: &str, value: f64) {
        self.set_gauge_labeled(name, value, &[]);
    }

    pub fn set_gauge_labeled(&mut self, name: &str, value: f64, labels: &[(&str, &str)]) {
        self.gauges.insert(series_key(name, labels), value);
    }

    pub fn counter(&self, name: &str) -> u64 {
        self.counter_labeled(name, &[])
    }

    pub fn counter_labeled(&self, name: &str, labels: &[(&str, &str)]) -> u64 {
        self.counters
            .get(&series_key(name, labels))
            .copied()
            .unwrap_or(0)
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for ((name, labels), value) in &self.counters {
            out.push_str(&format!(
                "# TYPE {name} counter\n{}{value}\n",
                render_series(name, labels)
            ));
        }
        for ((name, labels), value) in &self.gauges {
            out.push_str(&format!(
                "# TYPE {name} gauge\n{}{value}\n",
                render_series(name, labels)
            ));
        }
        out
    }
}

/// 标签按名称排序，保证同一组标签只有一个键。
fn series_key(name: &str, labels: &[(&str, &str)]) -> SeriesKey {
    let mut labels: Vec<(String, String)> = labels
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    labels.sort_by(|a, b| a.0.cmp(&b.0));
    (name.to_string(), labels)
}

fn render_series(name: &str, labels: &[(String, String)]) -> String {
    if labels.is_empty() {
        return format!("{name} ");
    }
    let rendered: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape_label_value(v)))
        .collect();
    format!("{}{{{}}} ", name, rendered.join(","))
}

fn escape_label_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_prometheus_text() {
        let mut m = MetricsText::default();
        m.inc("rustcam_drone_adapter_connections_total");
        m.set_gauge("rustcam_drone_ws_subscribers", 3.0);
        let text = m.render();
        assert!(text.contains("rustcam_drone_adapter_connections_total 1"));
        assert!(text.contains("rustcam_drone_ws_subscribers 3"));
        assert!(text.contains("# TYPE rustcam_drone_ws_subscribers gauge"));
    }

    #[test]
    fn unknown_counter_is_zero() {
        let m = MetricsText::default();
        assert_eq!(m.counter("nope"), 0);
    }

    #[test]
    fn labeled_counter_renders_prometheus_labels() {
        let mut m = MetricsText::default();
        m.inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "granted")]);
        let text = m.render();
        assert!(
            text.contains("rustcam_drone_control_acquire_total{result=\"granted\"} 1"),
            "带标签的计数器渲染错误: {text}"
        );
    }

    #[test]
    fn labeled_series_are_independent() {
        let mut m = MetricsText::default();
        m.inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "granted")]);
        m.inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "denied")]);
        m.inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "denied")]);
        let text = m.render();
        assert!(text.contains("rustcam_drone_control_acquire_total{result=\"granted\"} 1"));
        assert!(text.contains("rustcam_drone_control_acquire_total{result=\"denied\"} 2"));
        assert_eq!(
            m.counter_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "denied")]),
            2
        );
    }

    #[test]
    fn label_order_does_not_create_duplicate_series() {
        let mut m = MetricsText::default();
        m.inc_labeled(
            names::COMMAND_DURATION_SECONDS,
            &[("type", "rtl"), ("status", "executed")],
        );
        m.inc_labeled(
            names::COMMAND_DURATION_SECONDS,
            &[("status", "executed"), ("type", "rtl")],
        );
        assert_eq!(
            m.counter_labeled(
                names::COMMAND_DURATION_SECONDS,
                &[("type", "rtl"), ("status", "executed")]
            ),
            2
        );
        let text = m.render();
        assert_eq!(
            text.matches("rustcam_drone_command_duration_seconds{")
                .count(),
            1,
            "标签顺序不同不应产生两条时间序列"
        );
    }

    #[test]
    fn labeled_gauge_renders_labels() {
        let mut m = MetricsText::default();
        m.set_gauge_labeled(names::WS_SUBSCRIBERS, 3.0, &[("channel", "telemetry")]);
        let text = m.render();
        assert!(text.contains("rustcam_drone_ws_subscribers{channel=\"telemetry\"} 3"));
        assert!(text.contains("# TYPE rustcam_drone_ws_subscribers gauge"));
    }

    #[test]
    fn label_values_are_escaped() {
        let mut m = MetricsText::default();
        m.inc_labeled(names::COMMAND_TIMEOUT_TOTAL, &[("type", "a\"b\\c\nd")]);
        let text = m.render();
        assert!(text.contains(r#"rustcam_drone_command_timeout_total{type="a\"b\\c\nd"} 1"#));
    }

    #[test]
    fn metric_names_match_rule_list() {
        let expected = [
            names::ADAPTER_CONNECTIONS_TOTAL,
            names::ADAPTER_RECONNECT_TOTAL,
            names::ADAPTER_HEARTBEAT_AGE_SECONDS,
            names::COMMAND_DURATION_SECONDS,
            names::COMMAND_INFLIGHT,
            names::COMMAND_TIMEOUT_TOTAL,
            names::TELEMETRY_LAG_SECONDS,
            names::TELEMETRY_DEDUP_DROPPED_TOTAL,
            names::STREAM_PENDING,
            names::STREAM_LENGTH,
            names::GEOFENCE_EVAL_DURATION_SECONDS,
            names::GEOFENCE_ALERT_TOTAL,
            names::CONTROL_ACQUIRE_TOTAL,
            names::CONTROL_PREEMPT_TOTAL,
            names::WS_SUBSCRIBERS,
            names::WS_LAGGED_TOTAL,
        ];
        for name in expected {
            assert!(name.starts_with("rustcam_drone_"), "指标缺少前缀: {name}");
        }
        assert_eq!(expected.len(), 16, "规则 §14.1 共 16 个指标");
    }
}
